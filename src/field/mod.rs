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
  token::{Cursor, Lexeme, MONTHS, Word, is_space_byte},
};

#[cfg(test)]
mod tests;

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
  #[inline(always)]
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
  #[inline(always)]
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
  #[inline(always)]
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
  /// Whether the field's *text* begins with a `*`.
  ///
  /// A question about the bytes, not about the set, and the two disagree: `*,10` starts
  /// with a star and is a restriction, `10,*` is the same set and does not. Vixie's
  /// day-of-month against day-of-week rule keys on the text, so
  /// [`restricted`](Self::restricted) cannot answer it.
  pub(crate) starts_with_star: bool,
  /// The date predicate the field carried, if any.
  pub(crate) modifier: Option<Modifier>,
}

/// A value as the scanner found it: a digit run's value, or a name's place in the table.
///
/// Not a token. The scanner had to fold and match a name's three letters to recognise it
/// at all, so its index is already computed and nothing here borrows the input. This is
/// what a fused parser carries instead of a lexeme: the one thing the grammar is about to
/// use, for exactly as long as it takes to use it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Atom {
  /// A digit run's value, in the dialect's own numbering.
  Number(u32),
  /// One of the nineteen names, by its index in [`NAMES`](crate::token::NAMES).
  Name(u8),
}

/// Parses one whitespace-delimited field, stopping at whitespace or end of input.
///
/// `seed` is what an `H` resolves against. It is `None` for a parse that was given no
/// seed, which is a different thing from a dialect that has no `H`: the first is a
/// missing input and the second is a grammar that never had the construct.
pub(crate) fn parse_field<D: Dialect, S: ValueSink>(
  cursor: &mut Cursor<'_>,
  spec: FieldSpec,
  sink: &mut S,
  seed: Option<u64>,
) -> Result<FieldOutcome, ParseError> {
  // The field's first byte, read before anything is parsed. Vixie's day-of-month against
  // day-of-week rule asks whether the field was *written* starting with a star, which is
  // a different question from whether it restricts anything, and this is the only point
  // at which the answer is in hand for free.
  let starts_with_star = cursor.at(b'*');

  let mut items = 0usize;
  let mut every_item_was_bare = true;
  let mut state = ItemState {
    question_mark: false,
    modifier: None,
    sole_span: None,
    pending_wildcard: None,
  };

  loop {
    let start = cursor.pos();
    let (bare, first_end) = parse_item::<D, S>(cursor, spec, sink, &mut state, seed)?;
    items += 1;
    every_item_was_bare &= bare;

    if cursor.at(b',') {
      // Catching it at the comma rather than at the end of the field names the cause
      // and points at the item that has to stand alone.
      if let Some(violation) = sole_item_violation::<D>(&state, spec, || start..first_end) {
        return Err(violation);
      }
      cursor.advance();
    } else {
      break;
    }
  }

  if items > 1 {
    if let Some(violation) = sole_item_violation::<D>(&state, spec, || cursor.next_span()) {
      return Err(violation);
    }
  }

  // End of input and whitespace are the two ways a field ends well, and both are a byte
  // test. Anything else has to be named before it can be reported, and naming it costs a
  // scan that the ending a well-formed expression takes never pays.
  if cursor.peek().is_some_and(|byte| !is_space_byte(byte)) {
    if let Some(violation) = trailing_error::<D>(cursor, spec) {
      return Err(violation);
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
    starts_with_star,
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
///
/// The fallback span arrives as a closure because one of the two call sites has to scan
/// the next lexeme to produce it, and that site is reached by every list.
fn sole_item_violation<D: Dialect>(
  state: &ItemState,
  spec: FieldSpec,
  fallback: impl FnOnce() -> Range<usize>,
) -> Option<ParseError> {
  let kind = if state.modifier.is_some() {
    ErrorKind::ModifierMustBeAlone
  } else if state.question_mark {
    ErrorKind::QuestionMarkMustBeAlone { dialect: D::NAME }
  } else {
    return None;
  };
  let span = state.sole_span.clone().unwrap_or_else(fallback);
  Some(error(kind, span, spec))
}

/// The error for a lexeme the field cannot end on, if it cannot end on it.
///
/// A lexical failure is one of them, and is reported here — in *this* field, over the
/// bytes that failed. The field it is in is the field it is in; a bad byte after the
/// minute is a fault in the minute, not in whichever field the parser happened to be
/// reading when it finally tripped over it.
fn trailing_error<D: Dialect>(cursor: &Cursor<'_>, spec: FieldSpec) -> Option<ParseError> {
  let (lexeme, span) = cursor.peek_lexeme()?;
  let kind = match lexeme {
    // The caller's byte test has already ruled this out; it is written down so that the
    // two agree by construction rather than by both being remembered.
    Lexeme::Space => return None,
    Lexeme::UnexpectedCharacter => ErrorKind::UnexpectedCharacter,
    Lexeme::NumberTooLarge => ErrorKind::NumberTooLarge,
    Lexeme::Last | Lexeme::Weekday | Lexeme::Hash if !D::MODIFIERS => {
      ErrorKind::ModifierNotSupported { dialect: D::NAME }
    }
    Lexeme::Hashed if !D::HASHED_VALUES => ErrorKind::HashedValueNotSupported { dialect: D::NAME },
    _ => ErrorKind::UnexpectedToken,
  };
  Some(error(kind, span, spec))
}

/// Parses one comma-separated item.
///
/// Returns whether it was a lone `*` or `?`, and where the item's *first* lexeme ended.
/// That end is the span a list violation points at when the item set no span of its own,
/// and it is the only reason it is reported at all.
fn parse_item<D: Dialect, S: ValueSink>(
  cursor: &mut Cursor<'_>,
  spec: FieldSpec,
  sink: &mut S,
  state: &mut ItemState,
  seed: Option<u64>,
) -> Result<(bool, usize), ParseError> {
  let start = cursor.pos();
  let Some(first) = cursor.peek() else {
    return Err(error(ErrorKind::UnexpectedEnd, cursor.end_span(), spec));
  };

  match first {
    b'*' => {
      cursor.advance();
      let span = start..cursor.pos();
      let first_end = span.end;
      // `*` and `*/1` denote the same set: a stride of one narrows nothing. Neither
      // writes any bits here, because whether this *field* is a restriction depends on
      // items that have not been read yet. The expansion is settled at field end,
      // where that is known.
      let stride = optional_step(cursor, spec)?.unwrap_or(1);
      if stride == 1 {
        state.pending_wildcard.get_or_insert(span);
        return Ok((true, first_end));
      }
      // A stride above one narrows, so the field is restricted whatever follows and
      // the set is built against the dialect's ceiling right away.
      insert_range::<D, S>(spec, sink, spec.min, spec.max, stride, &span)?;
      Ok((false, first_end))
    }

    b'?' => {
      cursor.advance();
      let span = start..cursor.pos();
      let first_end = span.end;
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
      Ok((true, first_end))
    }

    b'0'..=b'9' => {
      let Some(value) = cursor.take_number() else {
        return Err(error(ErrorKind::NumberTooLarge, start..cursor.pos(), spec));
      };
      let span = start..cursor.pos();
      let first_end = span.end;
      parse_value::<D, S>(cursor, spec, sink, Atom::Number(value), span, state)
        .map(|bare| (bare, first_end))
    }

    byte if byte.is_ascii_alphabetic() => {
      let word = cursor.take_word();
      let span = start..cursor.pos();
      let first_end = span.end;
      match word {
        Word::Name(index) => {
          parse_value::<D, S>(cursor, spec, sink, Atom::Name(index), span, state)
            .map(|bare| (bare, first_end))
        }
        Word::Last | Word::Weekday if !D::MODIFIERS => Err(error(
          ErrorKind::ModifierNotSupported { dialect: D::NAME },
          span,
          spec,
        )),
        Word::Last => {
          parse_last_item::<S>(cursor, spec, sink, span, state).map(|bare| (bare, first_end))
        }
        Word::Weekday => Err(error(ErrorKind::UnexpectedToken, span, spec)),
        Word::Hashed => {
          parse_hashed_item::<D, S>(spec, sink, span, seed).map(|()| (false, first_end))
        }
        Word::Unexpected => Err(error(ErrorKind::UnexpectedCharacter, span, spec)),
      }
    }

    b'#' => {
      cursor.advance();
      let kind = if D::MODIFIERS {
        ErrorKind::UnexpectedToken
      } else {
        ErrorKind::ModifierNotSupported { dialect: D::NAME }
      };
      Err(error(kind, start..cursor.pos(), spec))
    }

    b'/' | b'-' | b',' => {
      cursor.advance();
      Err(error(ErrorKind::UnexpectedToken, start..cursor.pos(), spec))
    }

    // Whitespace, an `@`-nickname, and every byte that begins no token at all. Each is
    // an error, and which error it is takes a scan the arms above never pay for.
    _ => {
      let lexeme = cursor.take_lexeme();
      let kind = match lexeme {
        Some(Lexeme::UnexpectedCharacter) => ErrorKind::UnexpectedCharacter,
        Some(Lexeme::NumberTooLarge) => ErrorKind::NumberTooLarge,
        _ => ErrorKind::UnexpectedToken,
      };
      Err(error(kind, start..cursor.pos(), spec))
    }
  }
}

/// Parses an item that began with a value: a date predicate, or a member of the set.
fn parse_value<D: Dialect, S: ValueSink>(
  cursor: &mut Cursor<'_>,
  spec: FieldSpec,
  sink: &mut S,
  first: Atom,
  span: Range<usize>,
  state: &mut ItemState,
) -> Result<bool, ParseError> {
  if D::MODIFIERS {
    if let Some(bare) = parse_value_modifier::<D>(cursor, spec, first, &span, state)? {
      return Ok(bare);
    }
  }
  parse_value_item::<D, S>(cursor, spec, sink, first, span)?;
  Ok(false)
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
      let modifier = if at_weekday(cursor) {
        cursor.take_word();
        DayOfMonthModifier::LastWeekday
      } else if cursor.at(b'-') {
        cursor.advance();
        let (atom, offset_span) = value_atom(cursor, spec)?;
        let days = match atom {
          Atom::Number(days) if (1..=30).contains(&days) => days,
          Atom::Number(days) => {
            return Err(error(
              ErrorKind::ValueOutOfRange {
                value: days,
                min: 1,
                max: 30,
              },
              offset_span,
              spec,
            ));
          }
          Atom::Name(_) => return Err(error(ErrorKind::UnexpectedToken, offset_span, spec)),
        };
        DayOfMonthModifier::LastOffset { days: days as u8 }
      } else {
        DayOfMonthModifier::Last
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

/// Records the value an `H` stands for.
///
/// Two refusals, and they are different questions. A dialect without `H` never had the
/// construct; a dialect with it, parsed without a seed, has the construct and is missing
/// the one input that gives it a value. Reporting them apart is what tells a caller to
/// change dialect from what tells them to call `parse_with`.
fn parse_hashed_item<D: Dialect, S: ValueSink>(
  spec: FieldSpec,
  sink: &mut S,
  span: Range<usize>,
  seed: Option<u64>,
) -> Result<(), ParseError> {
  if !D::HASHED_VALUES {
    return Err(error(
      ErrorKind::HashedValueNotSupported { dialect: D::NAME },
      span,
      spec,
    ));
  }
  let Some(seed) = seed else {
    return Err(error(ErrorKind::HashedValueNeedsSeed, span, spec));
  };

  // Folded through the values the field has, and the result is already canonical. Every
  // other construct in the grammar folds through the dialect's own digits and converts
  // afterwards, which is right for them and wrong here: see [`canonical_domain`].
  let (first, values) = canonical_domain::<D>(spec);
  let canonical = if values == 0 {
    first
  } else {
    first.saturating_add((seed % u64::from(values)) as u32)
  };

  sink
    .insert(canonical)
    .map_err(|kind| error(kind, span, spec))
}

/// The values a field admits, in the canonical numbering: the first, and how many.
///
/// The image of [`canonical_value`], which for every field this crate declares is a
/// contiguous run — so the whole domain is `first..first + count`, and `nth` of it is
/// `first + n`. Deliberately the same one-armed match as that function, because it is the
/// same question asked the other way round: `canonical_value` maps a spelling to a value,
/// and this names the values it can produce. `field/tests.rs` holds the two against each
/// other by enumeration, field by field and dialect by dialect.
///
/// # Why this is not [`span_of`]
///
/// `span_of` counts the *spellings* a field takes, which is a different number the moment
/// a value has two of them: a [`ZeroSunday`](crate::WeekdayNumbering::ZeroSunday)
/// day-of-week field is written with eight digits and names seven days, because `0` and
/// `7` are both Sunday.
///
/// Which of the two counts a construct wants depends on what it does with the count.
/// Anything that *reads* values wants spellings — a range walks the digits it was written
/// in and folds each one afterwards, and a digit that names a day another digit already
/// named just sets the same bit twice, which a set does not notice. Anything that *picks*
/// a value wants values: a modulus over eight digits hands Sunday two of the eight
/// buckets and the other six days one each, so `H` in a day-of-week field puts twice as
/// much work on Sunday as on any other day. That is a scheduling defect rather than a
/// cosmetic one, and it is invisible to any test that looks at a single seed.
#[inline(always)]
fn canonical_domain<D: Dialect>(spec: FieldSpec) -> (u32, u32) {
  match spec.kind {
    FieldKind::DayOfWeek => {
      let (first, last) = D::WEEKDAY.canonical_bounds();
      (u32::from(first), u32::from(last.saturating_sub(first)) + 1)
    }
    _ => (spec.min, span_of(spec)),
  }
}

/// Whether the next lexeme is a bare `W` rather than the `WED` that begins with one.
#[inline(always)]
fn at_weekday(cursor: &Cursor<'_>) -> bool {
  matches!(cursor.peek(), Some(b'W' | b'w')) && cursor.peek_word() == Some(Word::Weekday)
}

/// Whether the next lexeme is a bare `L`.
#[inline(always)]
fn at_last(cursor: &Cursor<'_>) -> bool {
  matches!(cursor.peek(), Some(b'L' | b'l')) && cursor.peek_word() == Some(Word::Last)
}

/// Parses the predicates that begin with a value: `nW`, `nL` and `n#m`.
///
/// Returns `None` when the item is an ordinary value after all, so the caller falls
/// through to the set grammar. The byte test comes first in each arm: only `W`, `L` and
/// `#` can start a predicate, and everything else — a hyphen, a slash, a comma, a space,
/// the end of the input — is answered without looking at a second byte.
fn parse_value_modifier<D: Dialect>(
  cursor: &mut Cursor<'_>,
  spec: FieldSpec,
  first: Atom,
  first_span: &Range<usize>,
  state: &mut ItemState,
) -> Result<Option<bool>, ParseError> {
  match cursor.peek() {
    Some(b'W' | b'w') if at_weekday(cursor) => {
      if spec.kind != FieldKind::DayOfMonth {
        return Err(error(
          ErrorKind::ModifierNotValidHere,
          cursor.next_span(),
          spec,
        ));
      }
      let day = value_of::<D>(spec, first, first_span)?;
      cursor.take_word();
      state.modifier = Some(Modifier::DayOfMonth(DayOfMonthModifier::NearestWeekday {
        day: day as u8,
      }));
      Ok(Some(false))
    }
    Some(b'L' | b'l') if at_last(cursor) => {
      if spec.kind != FieldKind::DayOfWeek {
        return Err(error(
          ErrorKind::ModifierNotValidHere,
          cursor.next_span(),
          spec,
        ));
      }
      let raw = value_of::<D>(spec, first, first_span)?;
      cursor.take_word();
      let weekday = canonical_weekday::<D>(spec, raw, first_span)?;
      state.modifier = Some(Modifier::DayOfWeek(DayOfWeekModifier::Last { weekday }));
      Ok(Some(false))
    }
    Some(b'#') => {
      if spec.kind != FieldKind::DayOfWeek {
        return Err(error(
          ErrorKind::ModifierNotValidHere,
          cursor.next_span(),
          spec,
        ));
      }
      let raw = value_of::<D>(spec, first, first_span)?;
      cursor.advance();
      let (atom, nth_span) = value_atom(cursor, spec)?;
      let nth = match atom {
        Atom::Number(nth) if (1..=5).contains(&nth) => nth,
        Atom::Number(nth) => {
          return Err(error(
            ErrorKind::ValueOutOfRange {
              value: nth,
              min: 1,
              max: 5,
            },
            nth_span,
            spec,
          ));
        }
        Atom::Name(_) => return Err(error(ErrorKind::UnexpectedToken, nth_span, spec)),
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
  first: Atom,
  first_span: Range<usize>,
) -> Result<(), ParseError> {
  let start = value_of::<D>(spec, first, &first_span)?;
  let mut end = start;
  let mut had_range = false;
  let mut wrap = None;

  if cursor.at(b'-') {
    cursor.advance();
    let (atom, span) = value_atom(cursor, spec)?;
    end = value_of::<D>(spec, atom, &span)?;
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
  if cursor.at(b'/') {
    let slash = cursor.pos()..cursor.pos().saturating_add(1);
    cursor.advance();
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

/// How many ways a value can be written in the field, in the dialect's own numbering.
///
/// This is the modulus a wrapping range folds through, and it is the right count there:
/// the fold happens before the conversion to canonical, so it has to be in the numbering
/// the range was written in. For a zero-based field it is `max + 1` and for a one-based
/// field it is `max`, which is exactly what Quartz special cases; expressing it as the
/// count rather than as the ceiling removes the special case.
///
/// It is *not* the count of values the field admits, which is [`canonical_domain`]. The
/// two differ wherever a value has more than one spelling, and a construct that picks a
/// value rather than reading one has to fold through the second.
#[inline(always)]
fn span_of(spec: FieldSpec) -> u32 {
  spec.max.saturating_sub(spec.min).saturating_add(1)
}

/// Whether a backwards range wraps in this dialect, in this field.
///
/// The year is the exception in every dialect. A year has no modulus to wrap through, so
/// `2030-2020` names nothing; Quartz refuses it outright and so does this.
#[inline(always)]
fn wraps<D: Dialect>(spec: FieldSpec) -> bool {
  matches!(D::RANGES, RangePolicy::Wrapping) && spec.kind != FieldKind::Year
}

/// Reads `/step` if it is there.
#[inline(always)]
fn optional_step(cursor: &mut Cursor<'_>, spec: FieldSpec) -> Result<Option<u32>, ParseError> {
  if !cursor.at(b'/') {
    return Ok(None);
  }
  cursor.advance();
  read_step(cursor, spec).map(Some)
}

/// Reads the number after a `/`.
#[inline(always)]
fn read_step(cursor: &mut Cursor<'_>, spec: FieldSpec) -> Result<u32, ParseError> {
  let (atom, span) = value_atom(cursor, spec)?;
  match atom {
    Atom::Number(0) => Err(error(ErrorKind::ZeroStep, span, spec)),
    Atom::Number(step) => Ok(step),
    Atom::Name(_) => Err(error(ErrorKind::UnexpectedToken, span, spec)),
  }
}

/// Reads the next lexeme as a value, or fails saying what was there instead.
///
/// A lexeme that is a perfectly good token but not a value — a star, a comma, a
/// nickname — is an [`ErrorKind::UnexpectedToken`], which is what the token stream's
/// `value_of` produced for one. Naming it costs a scan, and every path that gets one is
/// on its way to an error anyway.
fn value_atom(
  cursor: &mut Cursor<'_>,
  spec: FieldSpec,
) -> Result<(Atom, Range<usize>), ParseError> {
  let start = cursor.pos();
  match cursor.peek() {
    None => Err(error(ErrorKind::UnexpectedEnd, cursor.end_span(), spec)),
    Some(b'0'..=b'9') => match cursor.take_number() {
      Some(value) => Ok((Atom::Number(value), start..cursor.pos())),
      None => Err(error(ErrorKind::NumberTooLarge, start..cursor.pos(), spec)),
    },
    Some(byte) if byte.is_ascii_alphabetic() => match cursor.take_word() {
      Word::Name(index) => Ok((Atom::Name(index), start..cursor.pos())),
      // `H` is a whole item, never one end of a range or the number after a `/`, which is
      // also where `cronexpr` draws the line: its hashed item has to be the entire item.
      Word::Last | Word::Weekday | Word::Hashed => {
        Err(error(ErrorKind::UnexpectedToken, start..cursor.pos(), spec))
      }
      Word::Unexpected => Err(error(
        ErrorKind::UnexpectedCharacter,
        start..cursor.pos(),
        spec,
      )),
    },
    Some(_) => {
      let lexeme = cursor.take_lexeme();
      let kind = match lexeme {
        Some(Lexeme::UnexpectedCharacter) => ErrorKind::UnexpectedCharacter,
        Some(Lexeme::NumberTooLarge) => ErrorKind::NumberTooLarge,
        _ => ErrorKind::UnexpectedToken,
      };
      Err(error(kind, start..cursor.pos(), spec))
    }
  }
}

/// Resolves an atom to a value in the dialect's numbering.
fn value_of<D: Dialect>(
  spec: FieldSpec,
  atom: Atom,
  span: &Range<usize>,
) -> Result<u32, ParseError> {
  match atom {
    Atom::Number(value) => {
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
    Atom::Name(index) => name_value::<D>(spec.kind, index)
      .ok_or_else(|| error(ErrorKind::UnknownName, span.clone(), spec)),
  }
}

/// Resolves a name from where the scanner found it in the table.
///
/// [`NAMES`](crate::token::NAMES) is the twelve months in order and then the seven
/// weekdays from Sunday, so a month's value is its index plus one and a weekday's
/// canonical number is its index less [`MONTHS`]. A field that knows neither kind of
/// name knows none, and a month name in a weekday field is not a name this field knows —
/// both of which fall out of the subtraction rather than needing a case.
///
/// `names_resolve_by_index_exactly_as_by_spelling` holds this against the string
/// comparison it replaced, name by name, field by field and dialect by dialect.
#[inline(always)]
fn name_value<D: Dialect>(kind: FieldKind, index: u8) -> Option<u32> {
  match kind {
    FieldKind::Month => (index < MONTHS).then(|| u32::from(index) + 1),
    FieldKind::DayOfWeek => index
      .checked_sub(MONTHS)
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
#[inline(always)]
const fn canonical_value<D: Dialect>(kind: FieldKind, value: u32) -> Option<u32> {
  match kind {
    FieldKind::DayOfWeek => match D::WEEKDAY.canonical(value) {
      Some(val) => Some(val as u32),
      None => None,
    },
    _ => Some(value),
  }
}

#[inline(always)]
fn error(kind: ErrorKind, span: Range<usize>, spec: FieldSpec) -> ParseError {
  ParseError::new(kind, span.into()).in_field(spec.kind)
}
