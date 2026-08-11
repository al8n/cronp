//! The parser as it stood before the scanner was fused into it.
//!
//! The scanner used to hand the grammar a `(Result<Token, LexError>, Range<usize>)` for
//! every lexeme, and the grammar matched on the `Token`. Fusing the two deleted that
//! value — and with it the token-level differential against logos, because there is no
//! longer a token stream to hold against one. Deleting an oracle along with the thing it
//! watched is how a rewrite ships a divergence, so the parser that had one is kept here
//! and [`tests`] holds the fused parser against it: the parsed `Schedule` on success,
//! and the error kind *and* span on failure.
//!
//! The chain is what makes this evidence rather than assertion. logos checks
//! [`token::Scanner`] in `token/tests/differential.rs`, that scanner feeds
//! [`field::parse_field`], and this module's [`parse`] is what the fused parser is
//! measured against. Every link is a test.
//!
//! Only what the fusion touched is copied. `Mask`, `ValueSink`, `FieldSpec`, `Calendar`'s
//! constructor, the name table, `count_fields`, the nickname calendars and the
//! day-of-month against day-of-week rule are used from the production modules: they are
//! shared assembly, not input reading, and a differential over code both sides call
//! proves nothing about it.
//!
//! # When this is allowed to change
//!
//! Four decisions here were changed after the fusion, in step with the shipped parser: a
//! lexical failure at the head of an expression is no longer read as an empty one, a
//! field reports the failure it ends on instead of leaving it to the next field, a field
//! is a restriction when the *union* it denotes narrows something rather than when it was
//! written with more than one item, and an item that has to be the whole field records
//! the bytes it was written as when it claims the field. All four were faults, all four
//! were in this parser and the shipped one identically, and *that is precisely why no
//! differential could find them* — an oracle proves that a change preserved behaviour, and
//! says nothing about whether the behaviour was right. Both sides can be wrong together.
//!
//! The third is the sharpest instance of that. `!(items == 1 && every_item_was_bare)`
//! computes a semantic property — does this field constrain anything — from a syntactic
//! one, so `*,2025` was a restriction and `*` was not, though they name the same years.
//! In the year field the parser then wrote out `1970..=2099` to back a restriction the
//! caller had not asked for, and refused the whole expression because 2098 does not fit
//! `Years<1>`. Both parsers agreed about that perfectly, for as long as it was wrong.
//!
//! It has a second half, and it arrived a round later because the first half was shipped
//! with an argument that hid it. A value the *caller* writes and the storage cannot hold
//! — `*,2098` — is not a fault in the expression either: it is legal cron, and a union
//! containing a wildcard stores nothing for it to overflow. `ValueSink` failures are
//! therefore held by `field::record` and answered once the field is classified, while
//! every failure that *is* a fault in the expression is raised before a value reaches a
//! sink and is unaffected. Refusing `*,2098` "for the same reason as `*,2100`" sorted the
//! two by what they looked like rather than by where they came from.
//!
//! The fourth is the same blindness in the *span* rather than in the kind. Both parsers
//! reported `ModifierMustBeAlone` for a date predicate written as one item of a list, and
//! both reported it over the wrong bytes: the three predicates that begin with a value
//! recorded no span at all, so a fallback answered with whatever the cursor was looking at
//! — the leading digit of `6#3`, the whitespace after the field, or an empty range past
//! the end of the input. `field::SoleItem` now holds the claim and its span in one slot,
//! so the fallback is gone rather than corrected. The differential compares spans and had
//! compared them for every one of these inputs; it agreed, because both sides were wrong
//! in the same way.
//!
//! So the rule is: this parser may only be edited to make a behaviour change deliberate
//! and simultaneous, in the same commit, with the reason written down. An oracle quietly
//! brought into line with the thing it watches is worse than no oracle, because it still
//! reads as evidence.
//!
//! That rule is no longer only prose. `schedule/tests/lexical_contract.rs` pins a digest
//! of every source file in this module, so an edit to one of them fails
//! `the_reference_parser_cannot_change_without_this_contract_changing_with_it` by name
//! and says what is owed rather than waiting for a reviewer to notice.
//!
//! And what holds the changed behaviour up is not [`tests`] — it stayed green through
//! both faults — but the contract cases, which are not differential: the table in
//! `schedule/tests.rs` that names the kind, the span and the field outright, and the
//! matrix generated over every dialect, every field position, every lexical failure and
//! every place a bad token can sit inside a field. That matrix's expectations are
//! computed from the templates that write the expression, never from a parser. Asking
//! this parser what to expect is the blindness, not a shortcut around it.

use crate::{
  dialect::Dialect,
  error::{ErrorKind, ParseError, Span},
  every,
  field::{FieldOutcome, FieldSpec, Mask, Parsed},
  years::Years,
};

use super::{
  Calendar, Fields, Nickname, Schedule, check_dom_dow, count_fields, lowercase_name,
  nickname_calendar,
};
use field::parse_field;
use token::{Cursor, Token};

pub(crate) mod field;
pub(crate) mod token;

mod tests;

/// Parses an expression in the dialect `D`, through a token stream.
pub(crate) fn parse<D: Dialect, const N: usize>(input: &str) -> Result<Schedule<D, N>, ParseError> {
  let mut cursor = Cursor::new(input);
  skip_space(&mut cursor);

  // `at_end` rather than `peek_token().is_none()`: the lookahead holds no token for a
  // lexical failure either, and a failure is not an empty expression. It is an ordinary
  // bad byte, and `parse_calendar` is what gives it a span and a field.
  if cursor.at_end() {
    return Err(ParseError::new(
      ErrorKind::EmptyExpression,
      cursor.end_span().into(),
    ));
  }

  match cursor.peek_token() {
    Some(Token::Macro(name)) => parse_macro::<D, N>(&mut cursor, name),
    _ => parse_calendar::<D, N>(&mut cursor, input).map(Schedule::Calendar),
  }
}

fn skip_space(cursor: &mut Cursor<'_>) {
  while cursor.peek_token() == Some(Token::Space) {
    cursor.bump();
  }
}

fn parse_macro<D: Dialect, const N: usize>(
  cursor: &mut Cursor<'_>,
  raw: &str,
) -> Result<Schedule<D, N>, ParseError> {
  let span: Span = cursor.next_span().into();
  cursor.bump();

  let name = raw.get(1..).unwrap_or("");
  let lowered = |candidate: &str| name.eq_ignore_ascii_case(candidate);

  if lowered("every") {
    if !D::EVERY {
      return Err(ParseError::new(
        ErrorKind::EveryNotSupported { dialect: D::NAME },
        span,
      ));
    }
    if cursor.peek_token() != Some(Token::Space) {
      return Err(ParseError::new(
        ErrorKind::EmptyDuration,
        cursor.next_span().into(),
      ));
    }
    skip_space(cursor);
    let (text, base) = cursor.rest();
    return every::parse(text.trim_end(), base).map(Schedule::Every);
  }

  if lowered("reboot") {
    if !D::REBOOT {
      return Err(ParseError::new(
        ErrorKind::RebootNotSupported { dialect: D::NAME },
        span,
      ));
    }
    expect_end(cursor)?;
    return Ok(Schedule::Reboot);
  }

  let Some(nickname) = Nickname::from_name(&lowercase_name(name)) else {
    return Err(ParseError::new(ErrorKind::UnknownMacro, span));
  };
  if !D::MACROS {
    return Err(ParseError::new(
      ErrorKind::MacroNotSupported { dialect: D::NAME },
      span,
    ));
  }
  expect_end(cursor)?;
  Ok(Schedule::Calendar(nickname_calendar::<D, N>(nickname)))
}

fn expect_end(cursor: &mut Cursor<'_>) -> Result<(), ParseError> {
  skip_space(cursor);
  if cursor.at_end() {
    Ok(())
  } else {
    Err(ParseError::new(
      ErrorKind::TrailingInput,
      cursor.next_span().into(),
    ))
  }
}

fn parse_calendar<D: Dialect, const N: usize>(
  cursor: &mut Cursor<'_>,
  input: &str,
) -> Result<Calendar<D, N>, ParseError> {
  let found = count_fields(input);
  let min = usize::from(D::MIN_FIELDS);
  let max = usize::from(D::MAX_FIELDS);
  if found < min || found > max {
    return Err(ParseError::new(
      ErrorKind::WrongFieldCount {
        found,
        min,
        max,
        dialect: D::NAME,
      },
      Span::new(0, input.len()),
    ));
  }

  let seconds = if D::HAS_SECONDS {
    read_mask::<D>(cursor, FieldSpec::SECOND)?
  } else {
    Parsed {
      values: 1,
      outcome: FieldOutcome::value::<D>(),
    }
  };

  let minutes = read_mask::<D>(cursor, FieldSpec::MINUTE)?;
  let hours = read_mask::<D>(cursor, FieldSpec::HOUR)?;
  let days_of_month = read_mask::<D>(cursor, FieldSpec::DAY_OF_MONTH)?;
  let months = read_mask::<D>(cursor, FieldSpec::MONTH)?;
  let days_of_week = read_mask::<D>(cursor, FieldSpec::day_of_week::<D>())?;

  skip_space(cursor);
  let years = if cursor.at_end() {
    Parsed {
      values: Years::new(),
      outcome: FieldOutcome::star::<D>(),
    }
  } else {
    let Some(spec) = FieldSpec::year::<D>() else {
      return Err(ParseError::new(
        ErrorKind::TrailingInput,
        cursor.next_span().into(),
      ));
    };
    let mut values = Years::new();
    let outcome = parse_field::<D, _>(cursor, spec, &mut values)?;
    skip_space(cursor);
    if !cursor.at_end() {
      return Err(ParseError::new(
        ErrorKind::TrailingInput,
        cursor.next_span().into(),
      ));
    }
    Parsed { values, outcome }
  };

  check_dom_dow::<D>(input, days_of_month.outcome, days_of_week.outcome)?;
  Ok(Calendar::new(Fields {
    seconds,
    minutes,
    hours,
    days_of_month,
    months,
    days_of_week,
    years,
  }))
}

/// Parses one bitset field and the whitespace after it.
fn read_mask<D: Dialect>(
  cursor: &mut Cursor<'_>,
  spec: FieldSpec,
) -> Result<Parsed<u64>, ParseError> {
  let mut mask = Mask::default();
  let outcome = parse_field::<D, Mask>(cursor, spec, &mut mask)?;
  skip_space(cursor);
  Ok(Parsed {
    values: mask.bits(),
    outcome,
  })
}
