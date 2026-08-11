//! The field grammar as it stood when it consumed a token stream.
//!
//! `FieldSpec`, `FieldOutcome`, `Modifier`, `Mask` and `ValueSink` come from
//! [`crate::field`]: they are where the values go, not how the input is read, and the
//! fusion did not touch them.
//!
//! The grammar is the one the fusion replaced, with four deliberate changes. A field
//! reports the lexical failure it ends on, rather than leaving it for the next field to
//! trip over. The wildcard witness the day rule keys off is folded across the items,
//! through [`witnesses_wildcard`], rather than read off the field's first token before
//! the loop — a first-token test and the items it claims to describe can disagree, and
//! for three inputs under `Robfig` they did. And whether the field is a restriction is
//! read off the *union* it denotes rather than off the item count: `!items == 1 &&
//! every_item_was_bare` called `*,2025` a restriction and `*` not one, though the two
//! name the same set, and in the year field it then materialised a range nobody asked
//! for and refused the expression because its own expansion did not fit `Years<1>`. Its
//! second half is that a value the *caller* wrote and the storage cannot hold is answered
//! against the same classification, through [`record`], rather than raised where it is
//! found. And an item that has to be the whole field now records its span as part of
//! claiming the field, through the shared [`SoleItem`], rather than setting a flag and
//! leaving an optional span for a fallback to guess at — the three predicates that begin
//! with a value never filled that span, so both parsers pointed `ModifierMustBeAlone` at
//! text no item occupied and, at the end of the last field, at no text at all. See
//! [`super`] for why all four were changed on both sides at once.

use core::ops::Range;

use super::token::{Cursor, LexError, Token};
use crate::{
  date::Weekday,
  dialect::{Dialect, QuestionMark, RangePolicy},
  error::{ErrorKind, FieldKind, ParseError},
  field::{
    FieldOutcome, FieldSpec, ItemFacts, Modifier, Run, Sole, SoleItem, ValueSink, record,
    witnesses_wildcard,
  },
  modifier::{DayOfMonthModifier, DayOfWeekModifier},
};

/// The month names, in order, as the value `index + 1`.
pub(crate) const MONTH_NAMES: [&str; 12] = [
  "JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC",
];

/// Parses one whitespace-delimited field, stopping at whitespace or end of input.
pub(crate) fn parse_field<D: Dialect, S: ValueSink>(
  cursor: &mut Cursor<'_>,
  spec: FieldSpec,
  sink: &mut S,
) -> Result<FieldOutcome, ParseError> {
  let mut items = 0usize;
  let mut wildcard = false;
  let mut state = ItemState {
    sole: SoleItem::none(),
    unconstrained: false,
    deferred: None,
  };

  loop {
    let facts = parse_item::<D, S>(cursor, spec, sink, &mut state)?;
    wildcard |= witnesses_wildcard::<D>(facts, items == 0);
    items += 1;

    if cursor.peek_token() == Some(Token::Comma) {
      if let Some(violation) = state.sole.violation::<D>(spec) {
        return Err(violation);
      }
      cursor.bump();
    } else {
      break;
    }
  }

  if items > 1 {
    if let Some(violation) = state.sole.violation::<D>(spec) {
      return Err(violation);
    }
  }

  // A lexical failure is the field's own to report, over the bytes that failed. Reading
  // the lookahead's result rather than `peek_token` is what makes that reachable: a
  // failure and the end of the input are both `None` there.
  match cursor.peek_spanned() {
    None | Some((Ok(Token::Space), _)) => {}
    Some((Ok(token), span)) => return Err(trailing_error::<D>(spec, *token, span.clone())),
    Some((Err(lex), span)) => return Err(error(lex_error_kind(*lex), span.clone(), spec)),
  }

  // Changed with the shipped parser, in one commit: one item that constrains nothing
  // makes the whole union the whole domain, so the field narrows nothing however many
  // items are beside it. See [`super`] on when this file may move, and the module
  // comment above for what was wrong with counting the items instead.
  let restricted = !state.unconstrained;

  // The second half of the same change, and for the same reason: a value the storage
  // could not hold is a fact about storage, and a field that stores nothing has none for
  // it to be a fact about. Held by `record` and answered here.
  if !restricted {
    sink.clear();
  } else if let Some(deferred) = state.deferred {
    return Err(deferred);
  }

  Ok(FieldOutcome {
    restricted,
    question_mark: state.sole.question_mark(),
    wildcard,
    modifier: state.sole.modifier(),
  })
}

/// What the items parsed so far have set aside for the whole field.
struct ItemState {
  /// The item that has to be the whole field, if one has been read, and where it was
  /// written.
  ///
  /// The fourth deliberate change, shared with the shipped parser rather than copied: see
  /// [`SoleItem`] for what the flag-beside-an-optional-span it replaced got wrong.
  sole: SoleItem,
  /// Whether some item so far constrained nothing: a `*`, a `*/1`, or a `?`.
  unconstrained: bool,
  /// The first value the storage could not hold, if one arrived, with its span.
  deferred: Option<ParseError>,
}

/// The error for a token the field cannot end on.
fn trailing_error<D: Dialect>(spec: FieldSpec, token: Token<'_>, span: Range<usize>) -> ParseError {
  let kind = match token {
    Token::Last | Token::Weekday | Token::Hash if !D::MODIFIERS => {
      ErrorKind::ModifierNotSupported { dialect: D::NAME }
    }
    Token::Hashed if !D::HASHED_VALUES => ErrorKind::HashedValueNotSupported { dialect: D::NAME },
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
) -> Result<ItemFacts, ParseError> {
  let (token, span) = bump_token(cursor, spec)?;

  match token {
    Token::Star => {
      let stride = optional_step(cursor, spec)?.unwrap_or(1);
      if stride == 1 {
        state.unconstrained = true;
        return Ok(ItemFacts::star(true));
      }
      insert_run::<D, S>(
        spec,
        sink,
        &mut state.deferred,
        Run::plain(spec.min, spec.max, stride),
        &span,
      );
      Ok(ItemFacts::star(false))
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
      if D::QUESTION_MARK.must_be_alone() {
        state.sole.claim(Sole::QuestionMark, span);
      }
      state.unconstrained = true;
      Ok(ItemFacts::QUESTION)
    }
    Token::Last | Token::Weekday | Token::Hash if !D::MODIFIERS => Err(error(
      ErrorKind::ModifierNotSupported { dialect: D::NAME },
      span,
      spec,
    )),
    Token::Last => parse_last_item::<S>(cursor, spec, sink, span, state),
    // This parser is never given a seed, so a dialect that admits `H` reports the missing
    // one. That is what the fused parser does for `Schedule::parse` too, which is the
    // only entry point the two are held against; `parse_with` is newer than the fusion
    // and is pinned by the contract cases rather than here.
    Token::Hashed => Err(error(
      if D::HASHED_VALUES {
        ErrorKind::HashedValueNeedsSeed
      } else {
        ErrorKind::HashedValueNotSupported { dialect: D::NAME }
      },
      span,
      spec,
    )),
    Token::Number(_) | Token::Name(_) => {
      if D::MODIFIERS {
        if let Some(facts) = parse_value_modifier::<D>(cursor, spec, token, &span, state)? {
          return Ok(facts);
        }
      }
      parse_value_item::<D, S>(cursor, spec, sink, &mut state.deferred, token, span)?;
      Ok(ItemFacts::VALUE)
    }
    _ => Err(error(ErrorKind::UnexpectedToken, span, spec)),
  }
}

/// Parses an item beginning with `L`.
fn parse_last_item<S: ValueSink>(
  cursor: &mut Cursor<'_>,
  spec: FieldSpec,
  sink: &mut S,
  span: Range<usize>,
  state: &mut ItemState,
) -> Result<ItemFacts, ParseError> {
  match spec.kind {
    FieldKind::DayOfMonth => {
      let start = span.start;
      let mut end = span.end;
      let modifier = match cursor.peek_token() {
        Some(Token::Weekday) => {
          end = cursor.next_span().end;
          cursor.bump();
          DayOfMonthModifier::LastWeekday
        }
        Some(Token::Hyphen) => {
          cursor.bump();
          let (token, offset_span) = bump_token(cursor, spec)?;
          end = offset_span.end;
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
              ));
            }
            _ => return Err(error(ErrorKind::UnexpectedToken, offset_span, spec)),
          };
          DayOfMonthModifier::LastOffset { days: days as u8 }
        }
        _ => DayOfMonthModifier::Last,
      };
      state
        .sole
        .claim(Sole::Modifier(Modifier::DayOfMonth(modifier)), start..end);
      Ok(ItemFacts::VALUE)
    }
    FieldKind::DayOfWeek => {
      record(
        sink,
        &mut state.deferred,
        spec,
        u32::from(Weekday::Saturday.to_canonical()),
        &span,
      );
      Ok(ItemFacts::VALUE)
    }
    _ => Err(error(ErrorKind::ModifierNotValidHere, span, spec)),
  }
}

/// Parses the predicates that begin with a value: `nW`, `nL` and `n#m`.
fn parse_value_modifier<D: Dialect>(
  cursor: &mut Cursor<'_>,
  spec: FieldSpec,
  first: Token<'_>,
  first_span: &Range<usize>,
  state: &mut ItemState,
) -> Result<Option<ItemFacts>, ParseError> {
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
      let end = cursor.next_span().end;
      cursor.bump();
      state.sole.claim(
        Sole::Modifier(Modifier::DayOfMonth(DayOfMonthModifier::NearestWeekday {
          day: day as u8,
        })),
        first_span.start..end,
      );
      Ok(Some(ItemFacts::VALUE))
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
      let end = cursor.next_span().end;
      cursor.bump();
      let weekday = canonical_weekday::<D>(spec, raw, first_span)?;
      state.sole.claim(
        Sole::Modifier(Modifier::DayOfWeek(DayOfWeekModifier::Last { weekday })),
        first_span.start..end,
      );
      Ok(Some(ItemFacts::VALUE))
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
          ));
        }
        _ => return Err(error(ErrorKind::UnexpectedToken, nth_span, spec)),
      };
      let weekday = canonical_weekday::<D>(spec, raw, first_span)?;
      state.sole.claim(
        Sole::Modifier(Modifier::DayOfWeek(DayOfWeekModifier::Nth {
          weekday,
          nth: nth as u8,
        })),
        first_span.start..nth_span.end,
      );
      Ok(Some(ItemFacts::VALUE))
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
  deferred: &mut Option<ParseError>,
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

  insert_run::<D, S>(
    spec,
    sink,
    deferred,
    Run {
      start,
      end,
      step,
      wrap,
    },
    &first_span,
  );
  Ok(())
}

/// How many distinct values the field admits.
fn span_of(spec: FieldSpec) -> u32 {
  spec.max.saturating_sub(spec.min).saturating_add(1)
}

/// Whether a backwards range wraps in this dialect, in this field.
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

/// Resolves a three-letter name for a field that knows names, by string comparison.
///
/// The fused parser resolves a name from the index the scanner already found it at.
/// This is the spelling that index has to reproduce, which is why it is kept written
/// out rather than deferring to the production resolver.
pub(crate) fn name_value<D: Dialect>(kind: FieldKind, name: &str) -> Option<u32> {
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

/// Records every value a [`Run`] names.
fn insert_run<D: Dialect, S: ValueSink>(
  spec: FieldSpec,
  sink: &mut S,
  deferred: &mut Option<ParseError>,
  run: Run,
  span: &Range<usize>,
) {
  debug_assert!(run.step >= 1, "a zero step is rejected before it gets here");
  let mut value = run.start;
  while value <= run.end {
    let folded = match run.wrap {
      Some(modulus) if modulus > 0 => (value.saturating_sub(spec.min) % modulus) + spec.min,
      _ => value,
    };
    if let Some(canonical) = canonical_value::<D>(spec.kind, folded) {
      record(sink, deferred, spec, canonical, span);
    }
    value = match value.checked_add(run.step) {
      Some(next) => next,
      None => break,
    };
  }
}

/// Converts a value from the dialect's numbering to the stored one.
fn canonical_value<D: Dialect>(kind: FieldKind, value: u32) -> Option<u32> {
  match kind {
    FieldKind::DayOfWeek => D::WEEKDAY.canonical(value).map(u32::from),
    _ => Some(value),
  }
}

/// What a lexical failure is, as a parse error.
fn lex_error_kind(lex: LexError) -> ErrorKind {
  match lex {
    LexError::UnexpectedCharacter => ErrorKind::UnexpectedCharacter,
    LexError::NumberTooLarge => ErrorKind::NumberTooLarge,
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
    Some((Err(lex), span)) => Err(error(lex_error_kind(lex), span, spec)),
  }
}

fn error(kind: ErrorKind, span: Range<usize>, spec: FieldSpec) -> ParseError {
  ParseError::new(kind, span.into()).in_field(spec.kind)
}
