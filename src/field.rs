//! One parser for every field that is a set of small integers.
//!
//! The fields differ only in their bounds and in whether they know names, so they share
//! a parser parameterised by a [`FieldSpec`] rather than getting one copy each.
//!
//! Where the values *go* is a second parameter. The sub-year fields collect into a
//! [`Mask`] of 64 bits; the year field collects into a `Years<N>`, which is not a `u64`
//! and cannot be. Both are [`ValueSink`]s, so the grammar — lists, ranges, steps,
//! open-ended steps, names — is written once and the year field does not get a second,
//! subtly different copy of it.

use core::ops::Range;

use crate::{
  date::Weekday,
  dialect::{Dialect, QuestionMark, RangePolicy, YearField},
  error::{ErrorKind, FieldKind, ParseError},
  modifier::{DayOfMonthModifier, DayOfWeekModifier},
  token::{Cursor, LexError, Token},
};

#[cfg(test)]
mod tests;

/// The month names, in order, as the value `index + 1`.
const MONTH_NAMES: [&str; 12] = [
  "JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC",
];

/// Somewhere a field's values can be recorded.
///
/// `insert` takes a value already converted to the canonical numbering, and returns an
/// [`ErrorKind`] rather than a [`ParseError`] because the sink does not know where in
/// the input the value came from; the parser attaches the span.
pub(crate) trait ValueSink {
  /// Records one value the field admits.
  fn insert(&mut self, value: u32) -> Result<(), ErrorKind>;
}

/// A bitset of up to 64 values, one bit per admitted value.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Mask(u64);

impl Mask {
  /// The raw bits, bit `n` set when the field admits `n`.
  pub(crate) const fn bits(self) -> u64 {
    self.0
  }
}

impl ValueSink for Mask {
  fn insert(&mut self, value: u32) -> Result<(), ErrorKind> {
    match 1u64.checked_shl(value) {
      Some(bit) => {
        self.0 |= bit;
        Ok(())
      }
      // Not reachable from any field this crate declares: every value is checked
      // against a bound below 64 before it arrives. Returning rather than panicking
      // keeps the no-panic promise if a wider field is ever added.
      None => Err(ErrorKind::ValueOutOfRange {
        value,
        min: 0,
        max: 63,
      }),
    }
  }
}

/// The bounds and identity of one field.
///
/// `min` and `max` are in the *dialect's* numbering, not the canonical one, because that
/// is the numbering the input is written in and therefore the one an out-of-range
/// message has to quote back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FieldSpec {
  /// Which field this is.
  pub(crate) kind: FieldKind,
  /// The lowest value the field admits.
  pub(crate) min: u32,
  /// The highest value the field admits.
  pub(crate) max: u32,
}

impl FieldSpec {
  /// Seconds, `0..=59`.
  pub(crate) const SECOND: Self = Self {
    kind: FieldKind::Second,
    min: 0,
    max: 59,
  };
  /// Minutes, `0..=59`.
  pub(crate) const MINUTE: Self = Self {
    kind: FieldKind::Minute,
    min: 0,
    max: 59,
  };
  /// Hours, `0..=23`.
  pub(crate) const HOUR: Self = Self {
    kind: FieldKind::Hour,
    min: 0,
    max: 23,
  };
  /// Day of month, `1..=31`.
  pub(crate) const DAY_OF_MONTH: Self = Self {
    kind: FieldKind::DayOfMonth,
    min: 1,
    max: 31,
  };
  /// Month, `1..=12`.
  pub(crate) const MONTH: Self = Self {
    kind: FieldKind::Month,
    min: 1,
    max: 12,
  };

  /// The year field, when the dialect has one.
  ///
  /// The bounds come from `D::YEAR`, the same place [`Dialect::YEAR`]'s `admits` reads
  /// them, so a year written out and a year left implicit cannot disagree about which
  /// years the dialect allows.
  pub(crate) fn year<D: Dialect>() -> Option<Self> {
    match D::YEAR {
      YearField::Absent => None,
      YearField::Optional { min, max } => Some(Self {
        kind: FieldKind::Year,
        min: u32::from(min),
        max: u32::from(max),
      }),
    }
  }

  /// Day of week, in the dialect's own numbering.
  pub(crate) fn day_of_week<D: Dialect>() -> Self {
    let (min, max) = D::WEEKDAY.raw_bounds();
    Self {
      kind: FieldKind::DayOfWeek,
      min,
      max,
    }
  }
}

/// A date predicate a field carried instead of, or beside, its values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Modifier {
  /// From the day-of-month field.
  DayOfMonth(DayOfMonthModifier),
  /// From the day-of-week field.
  DayOfWeek(DayOfWeekModifier),
}

/// What a parsed field says about itself, beyond the values it admits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FieldOutcome {
  /// Whether the field narrows anything.
  ///
  /// False for `*`, for `?`, and for `*/1`, which denotes the same set as `*` because a
  /// stride of one drops nothing. True for anything written out — `0-59` counts as a
  /// restriction even though it admits every minute, because the stored set is then
  /// what answers for the field.
  pub(crate) restricted: bool,
  /// Whether the field was written as a `?` that means "no specific value".
  ///
  /// Only a dialect whose `?` carries that meaning sets it. Where `?` is another
  /// spelling of `*` this stays false, because such a field says no more about itself
  /// than a star does.
  pub(crate) question_mark: bool,
  /// The date predicate the field carried, if any.
  pub(crate) modifier: Option<Modifier>,
}

/// Parses one whitespace-delimited field, stopping at whitespace or end of input.
pub(crate) fn parse_field<D: Dialect, S: ValueSink>(
  cursor: &mut Cursor<'_>,
  spec: FieldSpec,
  sink: &mut S,
) -> Result<FieldOutcome, ParseError> {
  let mut items = 0usize;
  let mut every_item_was_bare = true;
  let mut state = ItemState {
    question_mark: false,
    modifier: None,
    sole_span: None,
    pending_wildcard: None,
  };

  loop {
    let start = cursor.next_span();
    let bare = parse_item::<D, S>(cursor, spec, sink, &mut state)?;
    items += 1;
    every_item_was_bare &= bare;

    if cursor.peek_token() == Some(Token::Comma) {
      // Catching it at the comma rather than at the end of the field names the cause
      // and points at the item that has to stand alone.
      if let Some(violation) = sole_item_violation::<D>(&state, spec, &start) {
        return Err(violation);
      }
      cursor.bump();
    } else {
      break;
    }
  }

  if items > 1 {
    if let Some(violation) = sole_item_violation::<D>(&state, spec, &cursor.next_span()) {
      return Err(violation);
    }
  }

  if let Some(token) = cursor.peek_token() {
    if token != Token::Space {
      return Err(trailing_error::<D>(spec, token, cursor.next_span()));
    }
  }

  let restricted = !(items == 1 && every_item_was_bare);

  // An unrestricted field stores no bits at all. `*` means "no constraint", not "the
  // set from min to whatever this sink happens to hold": with nothing written there is
  // nothing for a storage ceiling to truncate, so the width problem cannot arise. A
  // field that turned out to be restricted materialises its wildcard against the
  // *dialect's* ceiling, and the sink reports whatever it cannot hold — which for
  // years names the `N` the caller needs instead of quietly dropping the year.
  if restricted {
    if let Some(span) = state.pending_wildcard.clone() {
      insert_range::<D, S>(spec, sink, spec.min, spec.max, 1, &span)?;
    }
  }

  Ok(FieldOutcome {
    restricted,
    question_mark: state.question_mark,
    modifier: state.modifier,
  })
}

/// What the items parsed so far have set aside for the whole field.
struct ItemState {
  /// Set by a `?` that means "no specific value" — Quartz's `?`, not the Go dialect's,
  /// whose `?` is only another spelling of `*` and says nothing about the field.
  question_mark: bool,
  modifier: Option<Modifier>,
  /// Where an item that has to be the whole field was written.
  sole_span: Option<Range<usize>>,
  /// Where a `*`, `*/1` or `?` was written, if one was and it has not been expanded.
  ///
  /// Held rather than expanded because unrestrictedness is a property of the *field*
  /// and is not known until the field ends. Expanding at item granularity is what let
  /// a `*` beside another item be narrowed to the storage ceiling and then read back
  /// as a restriction.
  pending_wildcard: Option<Range<usize>>,
}

/// The error for an item that has to be the whole field appearing in a list.
///
/// Two kinds of item make that demand, and for the same reason: neither is a member of
/// the set the field denotes. A date predicate is a property of a date, and Quartz's `?`
/// is a statement about the field itself. A dialect whose `?` is merely another spelling
/// of `*` makes no such demand, and this returns `None` for it.
fn sole_item_violation<D: Dialect>(
  state: &ItemState,
  spec: FieldSpec,
  fallback: &Range<usize>,
) -> Option<ParseError> {
  let kind = if state.modifier.is_some() {
    ErrorKind::ModifierMustBeAlone
  } else if state.question_mark {
    ErrorKind::QuestionMarkMustBeAlone { dialect: D::NAME }
  } else {
    return None;
  };
  let span = state.sole_span.clone().unwrap_or_else(|| fallback.clone());
  Some(error(kind, span, spec))
}

/// The error for a token the field cannot end on.
fn trailing_error<D: Dialect>(spec: FieldSpec, token: Token<'_>, span: Range<usize>) -> ParseError {
  let kind = match token {
    Token::Last | Token::Weekday | Token::Hash if !D::MODIFIERS => {
      ErrorKind::ModifierNotSupported { dialect: D::NAME }
    }
    _ => ErrorKind::UnexpectedToken,
  };
  error(kind, span, spec)
}

/// Parses one comma-separated item. Returns whether it was a lone `*` or `?`.
fn parse_item<D: Dialect, S: ValueSink>(
  cursor: &mut Cursor<'_>,
  spec: FieldSpec,
  sink: &mut S,
  state: &mut ItemState,
) -> Result<bool, ParseError> {
  let (token, span) = bump_token(cursor, spec)?;

  match token {
    Token::Star => {
      // `*` and `*/1` denote the same set: a stride of one narrows nothing. Neither
      // writes any bits here, because whether this *field* is a restriction depends on
      // items that have not been read yet. The expansion is settled at field end,
      // where that is known.
      let stride = optional_step(cursor, spec)?.unwrap_or(1);
      if stride == 1 {
        state.pending_wildcard.get_or_insert(span);
        return Ok(true);
      }
      // A stride above one narrows, so the field is restricted whatever follows and
      // the set is built against the dialect's ceiling right away.
      insert_range::<D, S>(spec, sink, spec.min, spec.max, stride, &span)?;
      Ok(false)
    }
    Token::Question => {
      if D::QUESTION_MARK == QuestionMark::Forbidden {
        return Err(error(
          ErrorKind::QuestionMarkNotSupported { dialect: D::NAME },
          span,
          spec,
        ));
      }
      if !matches!(spec.kind, FieldKind::DayOfMonth | FieldKind::DayOfWeek) {
        return Err(error(ErrorKind::QuestionMarkNotValidHere, span, spec));
      }
      // Only a `?` that means "no specific value" is recorded. Where `?` is another
      // spelling of `*` there is nothing to record: it says as much about the field
      // as a star does, which is nothing.
      if D::QUESTION_MARK.must_be_alone() {
        state.question_mark = true;
        state.sole_span = Some(span.clone());
      }
      // Deferred for the same reason `*` is: `?` admits everything, and whether that
      // has to be written down depends on what follows it.
      state.pending_wildcard.get_or_insert(span);
      Ok(true)
    }
    Token::Last | Token::Weekday | Token::Hash if !D::MODIFIERS => Err(error(
      ErrorKind::ModifierNotSupported { dialect: D::NAME },
      span,
      spec,
    )),
    Token::Last => parse_last_item::<S>(cursor, spec, sink, span, state),
    Token::Number(_) | Token::Name(_) => {
      if D::MODIFIERS {
        if let Some(bare) = parse_value_modifier::<D>(cursor, spec, token, &span, state)? {
          return Ok(bare);
        }
      }
      parse_value_item::<D, S>(cursor, spec, sink, token, span)?;
      Ok(false)
    }
    _ => Err(error(ErrorKind::UnexpectedToken, span, spec)),
  }
}

/// Parses an item beginning with `L`.
///
/// In the day-of-month field `L` is a predicate: `L`, `LW` or `L-n`. In the day-of-week
/// field a bare `L` is not a predicate at all — Quartz defines it as another spelling of
/// Saturday — so it goes into the bitset like any other value.
fn parse_last_item<S: ValueSink>(
  cursor: &mut Cursor<'_>,
  spec: FieldSpec,
  sink: &mut S,
  span: Range<usize>,
  state: &mut ItemState,
) -> Result<bool, ParseError> {
  match spec.kind {
    FieldKind::DayOfMonth => {
      let modifier = match cursor.peek_token() {
        Some(Token::Weekday) => {
          cursor.bump();
          DayOfMonthModifier::LastWeekday
        }
        Some(Token::Hyphen) => {
          cursor.bump();
          let (token, offset_span) = bump_token(cursor, spec)?;
          let days = match token {
            Token::Number(days) if (1..=30).contains(&days) => days,
            Token::Number(days) => {
              return Err(error(
                ErrorKind::ValueOutOfRange {
                  value: days,
                  min: 1,
                  max: 30,
                },
                offset_span,
                spec,
              ))
            }
            _ => return Err(error(ErrorKind::UnexpectedToken, offset_span, spec)),
          };
          DayOfMonthModifier::LastOffset { days: days as u8 }
        }
        _ => DayOfMonthModifier::Last,
      };
      state.modifier = Some(Modifier::DayOfMonth(modifier));
      state.sole_span = Some(span);
      Ok(false)
    }
    FieldKind::DayOfWeek => {
      // Quartz: a lone `L` in the day-of-week field "simply means 7 or SAT".
      sink
        .insert(u32::from(Weekday::Saturday.to_canonical()))
        .map_err(|kind| error(kind, span, spec))?;
      Ok(false)
    }
    _ => Err(error(ErrorKind::ModifierNotValidHere, span, spec)),
  }
}

/// Parses the predicates that begin with a value: `nW`, `nL` and `n#m`.
///
/// Returns `None` when the item is an ordinary value after all, so the caller falls
/// through to the set grammar.
fn parse_value_modifier<D: Dialect>(
  cursor: &mut Cursor<'_>,
  spec: FieldSpec,
  first: Token<'_>,
  first_span: &Range<usize>,
  state: &mut ItemState,
) -> Result<Option<bool>, ParseError> {
  match cursor.peek_token() {
    Some(Token::Weekday) => {
      if spec.kind != FieldKind::DayOfMonth {
        return Err(error(
          ErrorKind::ModifierNotValidHere,
          cursor.next_span(),
          spec,
        ));
      }
      let day = value_of::<D>(spec, first, first_span)?;
      cursor.bump();
      state.modifier = Some(Modifier::DayOfMonth(DayOfMonthModifier::NearestWeekday {
        day: day as u8,
      }));
      Ok(Some(false))
    }
    Some(Token::Last) => {
      if spec.kind != FieldKind::DayOfWeek {
        return Err(error(
          ErrorKind::ModifierNotValidHere,
          cursor.next_span(),
          spec,
        ));
      }
      let raw = value_of::<D>(spec, first, first_span)?;
      cursor.bump();
      let weekday = canonical_weekday::<D>(spec, raw, first_span)?;
      state.modifier = Some(Modifier::DayOfWeek(DayOfWeekModifier::Last { weekday }));
      Ok(Some(false))
    }
    Some(Token::Hash) => {
      if spec.kind != FieldKind::DayOfWeek {
        return Err(error(
          ErrorKind::ModifierNotValidHere,
          cursor.next_span(),
          spec,
        ));
      }
      let raw = value_of::<D>(spec, first, first_span)?;
      cursor.bump();
      let (token, nth_span) = bump_token(cursor, spec)?;
      let nth = match token {
        Token::Number(nth) if (1..=5).contains(&nth) => nth,
        Token::Number(nth) => {
          return Err(error(
            ErrorKind::ValueOutOfRange {
              value: nth,
              min: 1,
              max: 5,
            },
            nth_span,
            spec,
          ))
        }
        _ => return Err(error(ErrorKind::UnexpectedToken, nth_span, spec)),
      };
      let weekday = canonical_weekday::<D>(spec, raw, first_span)?;
      state.modifier = Some(Modifier::DayOfWeek(DayOfWeekModifier::Nth {
        weekday,
        nth: nth as u8,
      }));
      Ok(Some(false))
    }
    _ => Ok(None),
  }
}

/// Turns a day-of-week value in the dialect's numbering into a [`Weekday`].
fn canonical_weekday<D: Dialect>(
  spec: FieldSpec,
  raw: u32,
  span: &Range<usize>,
) -> Result<Weekday, ParseError> {
  D::WEEKDAY
    .canonical(raw)
    .and_then(Weekday::from_canonical)
    .ok_or_else(|| {
      error(
        ErrorKind::ValueOutOfRange {
          value: raw,
          min: spec.min,
          max: spec.max,
        },
        span.clone(),
        spec,
      )
    })
}

/// Parses `value`, `value-value`, `value-value/step` or `value/step`.
fn parse_value_item<D: Dialect, S: ValueSink>(
  cursor: &mut Cursor<'_>,
  spec: FieldSpec,
  sink: &mut S,
  first: Token<'_>,
  first_span: Range<usize>,
) -> Result<(), ParseError> {
  let start = value_of::<D>(spec, first, &first_span)?;
  let mut end = start;
  let mut had_range = false;
  let mut wrap = None;

  if cursor.peek_token() == Some(Token::Hyphen) {
    cursor.bump();
    let (token, span) = bump_token(cursor, spec)?;
    end = value_of::<D>(spec, token, &span)?;
    had_range = true;
    if start > end {
      if !wraps::<D>(spec) {
        return Err(error(
          ErrorKind::ReversedRange { start, end },
          first_span.start..span.end,
          spec,
        ));
      }
      // Run on past the ceiling and let `insert_range` fold each value back. The
      // modulus is the count of values the field admits, so a walk from `start` to
      // `end + modulus` visits every value once, in order, across the seam.
      let modulus = span_of(spec);
      wrap = Some(modulus);
      end = end.saturating_add(modulus);
    }
  }

  let mut step = 1;
  if cursor.peek_token() == Some(Token::Slash) {
    let slash = cursor.next_span();
    cursor.bump();
    if !had_range {
      if !D::OPEN_ENDED_STEP {
        return Err(error(
          ErrorKind::OpenEndedStepNotSupported { dialect: D::NAME },
          slash,
          spec,
        ));
      }
      end = spec.max;
    }
    step = read_step(cursor, spec)?;
  }

  insert_range_wrapping::<D, S>(spec, sink, start, end, step, wrap, &first_span)
}

/// How many distinct values the field admits.
///
/// This is the modulus a wrapping range folds through. For a zero-based field it is
/// `max + 1` and for a one-based field it is `max`, which is exactly what Quartz special
/// cases; expressing it as the count rather than as the ceiling removes the special case.
fn span_of(spec: FieldSpec) -> u32 {
  spec.max.saturating_sub(spec.min).saturating_add(1)
}

/// Whether a backwards range wraps in this dialect, in this field.
///
/// The year is the exception in every dialect. A year has no modulus to wrap through, so
/// `2030-2020` names nothing; Quartz refuses it outright and so does this.
fn wraps<D: Dialect>(spec: FieldSpec) -> bool {
  matches!(D::RANGES, RangePolicy::Wrapping) && spec.kind != FieldKind::Year
}

/// Reads `/step` if it is there.
fn optional_step(cursor: &mut Cursor<'_>, spec: FieldSpec) -> Result<Option<u32>, ParseError> {
  if cursor.peek_token() != Some(Token::Slash) {
    return Ok(None);
  }
  cursor.bump();
  read_step(cursor, spec).map(Some)
}

/// Reads the number after a `/`.
fn read_step(cursor: &mut Cursor<'_>, spec: FieldSpec) -> Result<u32, ParseError> {
  let (token, span) = bump_token(cursor, spec)?;
  match token {
    Token::Number(0) => Err(error(ErrorKind::ZeroStep, span, spec)),
    Token::Number(step) => Ok(step),
    _ => Err(error(ErrorKind::UnexpectedToken, span, spec)),
  }
}

/// Resolves a number or a name to a value in the dialect's numbering.
fn value_of<D: Dialect>(
  spec: FieldSpec,
  token: Token<'_>,
  span: &Range<usize>,
) -> Result<u32, ParseError> {
  match token {
    Token::Number(value) => {
      if value < spec.min || value > spec.max {
        Err(error(
          ErrorKind::ValueOutOfRange {
            value,
            min: spec.min,
            max: spec.max,
          },
          span.clone(),
          spec,
        ))
      } else {
        Ok(value)
      }
    }
    Token::Name(name) => name_value::<D>(spec.kind, name)
      .ok_or_else(|| error(ErrorKind::UnknownName, span.clone(), spec)),
    _ => Err(error(ErrorKind::UnexpectedToken, span.clone(), spec)),
  }
}

/// Resolves a three-letter name for a field that knows names.
fn name_value<D: Dialect>(kind: FieldKind, name: &str) -> Option<u32> {
  match kind {
    FieldKind::Month => MONTH_NAMES
      .iter()
      .position(|month| month.eq_ignore_ascii_case(name))
      .and_then(|index| u32::try_from(index).ok())
      .map(|index| index + 1),
    FieldKind::DayOfWeek => crate::dialect::WeekdayNumbering::canonical_name(name)
      .map(|canonical| D::WEEKDAY.raw_from_canonical(canonical)),
    _ => None,
  }
}

/// Records `start..=end` stepping by `step`, converting each value to the canonical
/// numbering on the way in.
fn insert_range<D: Dialect, S: ValueSink>(
  spec: FieldSpec,
  sink: &mut S,
  start: u32,
  end: u32,
  step: u32,
  span: &Range<usize>,
) -> Result<(), ParseError> {
  insert_range_wrapping::<D, S>(spec, sink, start, end, step, None, span)
}

/// As [`insert_range`], but folding each value back into the field when `wrap` is set.
///
/// The fold is `((value - min) % modulus) + min`, which reproduces Quartz's `value %
/// max` together with its "a zero becomes the maximum" rule for one-based fields,
/// without needing that rule as a case of its own. It happens *before* the conversion to
/// the canonical numbering, so a wrapped weekday is converted from the dialect's own
/// digit, not from a digit past its ceiling.
fn insert_range_wrapping<D: Dialect, S: ValueSink>(
  spec: FieldSpec,
  sink: &mut S,
  start: u32,
  end: u32,
  step: u32,
  wrap: Option<u32>,
  span: &Range<usize>,
) -> Result<(), ParseError> {
  debug_assert!(step >= 1, "a zero step is rejected before it gets here");
  let mut value = start;
  while value <= end {
    let folded = match wrap {
      Some(modulus) if modulus > 0 => (value.saturating_sub(spec.min) % modulus) + spec.min,
      _ => value,
    };
    if let Some(canonical) = canonical_value::<D>(spec.kind, folded) {
      sink
        .insert(canonical)
        .map_err(|kind| error(kind, span.clone(), spec))?;
    }
    value = match value.checked_add(step) {
      Some(next) => next,
      None => break,
    };
  }
  Ok(())
}

/// Converts a value from the dialect's numbering to the stored one.
///
/// Only the day of week differs, and it is the reason this exists: Vixie writes Sunday
/// as either `0` or `7` and Quartz writes it as `1`, so `5-7` under Vixie is Friday,
/// Saturday and Sunday while under Quartz it is Thursday, Friday and Saturday. Doing the
/// conversion per value rather than per field is what makes the Vixie case fold onto
/// Sunday instead of wrapping.
fn canonical_value<D: Dialect>(kind: FieldKind, value: u32) -> Option<u32> {
  match kind {
    FieldKind::DayOfWeek => D::WEEKDAY.canonical(value).map(u32::from),
    _ => Some(value),
  }
}

/// Takes the next token, turning end-of-input and lexical failure into parse errors.
fn bump_token<'a>(
  cursor: &mut Cursor<'a>,
  spec: FieldSpec,
) -> Result<(Token<'a>, Range<usize>), ParseError> {
  match cursor.bump() {
    None => Err(error(ErrorKind::UnexpectedEnd, cursor.end_span(), spec)),
    Some((Ok(token), span)) => Ok((token, span)),
    Some((Err(lex), span)) => Err(error(
      match lex {
        LexError::UnexpectedCharacter => ErrorKind::UnexpectedCharacter,
        LexError::NumberTooLarge => ErrorKind::NumberTooLarge,
      },
      span,
      spec,
    )),
  }
}

fn error(kind: ErrorKind, span: Range<usize>, spec: FieldSpec) -> ParseError {
  ParseError::new(kind, span.into()).in_field(spec.kind)
}
