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
  dialect::{Dialect, QuestionMark, YearField},
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

  /// How far `*` expands, given the field's own ceiling.
  ///
  /// Only the year field ever narrows this, and it has to. Quartz's year field admits
  /// `1970..=2099` while `Years<1>` stops at 2097, so expanding `*` against the
  /// dialect's ceiling would make the commonest expression in the dialect fail on the
  /// width of the value it is stored in. A year *written out* is still checked against
  /// the dialect's ceiling, so `2098` is still rejected — and rejected by name, with
  /// the `N` that would hold it.
  fn wildcard_ceiling(&self, field_max: u32) -> u32 {
    field_max
  }
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
  /// False only when the field is a lone `*` or `?`. `0-59` counts as a restriction
  /// even though it admits every minute, which is what Vixie's day-of-month against
  /// day-of-week rule keys off: the rule asks whether the field was written as a star,
  /// not whether it happens to cover everything.
  pub(crate) restricted: bool,
  /// Whether the field was written as `?`.
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
  };

  loop {
    let start = cursor.next_span();
    let bare = parse_item::<D, S>(cursor, spec, sink, &mut state)?;
    items += 1;
    every_item_was_bare &= bare;

    if cursor.peek_token() == Some(Token::Comma) {
      // A predicate is not a set member, so it cannot be one alternative among
      // several. Catching it here rather than at the end names the cause.
      if state.modifier.is_some() {
        return Err(error(ErrorKind::ModifierMustBeAlone, start, spec));
      }
      cursor.bump();
    } else {
      break;
    }
  }

  if items > 1 && state.modifier.is_some() {
    return Err(error(
      ErrorKind::ModifierMustBeAlone,
      cursor.next_span(),
      spec,
    ));
  }

  if let Some(token) = cursor.peek_token() {
    if token != Token::Space {
      return Err(trailing_error::<D>(spec, token, cursor.next_span()));
    }
  }

  Ok(FieldOutcome {
    restricted: !(items == 1 && every_item_was_bare),
    question_mark: state.question_mark,
    modifier: state.modifier,
  })
}

/// What the items parsed so far have set aside for the whole field.
struct ItemState {
  question_mark: bool,
  modifier: Option<Modifier>,
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
      let step = optional_step(cursor, spec)?;
      let ceiling = sink.wildcard_ceiling(spec.max);
      insert_range::<D, S>(spec, sink, spec.min, ceiling, step.unwrap_or(1), &span)?;
      Ok(step.is_none())
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
      state.question_mark = true;
      let ceiling = sink.wildcard_ceiling(spec.max);
      insert_range::<D, S>(spec, sink, spec.min, ceiling, 1, &span)?;
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

  if cursor.peek_token() == Some(Token::Hyphen) {
    cursor.bump();
    let (token, span) = bump_token(cursor, spec)?;
    end = value_of::<D>(spec, token, &span)?;
    had_range = true;
    if start > end {
      return Err(error(
        ErrorKind::ReversedRange { start, end },
        first_span.start..span.end,
        spec,
      ));
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

  insert_range::<D, S>(spec, sink, start, end, step, &first_span)
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
  debug_assert!(step >= 1, "a zero step is rejected before it gets here");
  let mut value = start;
  while value <= end {
    if let Some(canonical) = canonical_value::<D>(spec.kind, value) {
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
