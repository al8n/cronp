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
//! Only what the fusion touched is copied. `Mask`, `ValueSink`, `FieldSpec`, the name
//! table, `count_fields`, the nickname calendars and the day-of-month against
//! day-of-week rule are used from the production modules: they are shared assembly, not
//! input reading, and a differential over code both sides call proves nothing about it.
//!
//! # When this is allowed to change
//!
//! Two decisions here were changed after the fusion, in step with the shipped parser: a
//! lexical failure at the head of an expression is no longer read as an empty one, and a
//! field reports the failure it ends on instead of leaving it to the next field. Both
//! were faults, both were in this parser and the shipped one identically, and *that is
//! precisely why no differential could find them* — an oracle proves that a change
//! preserved behaviour, and says nothing about whether the behaviour was right. Both
//! sides can be wrong together.
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
  field::{FieldSpec, Mask, Modifier},
};

use super::{
  Calendar, Nickname, Schedule, check_dom_dow, count_fields, lowercase_name, nickname_calendar,
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

  let mut calendar = Calendar::<D, N>::empty();

  if D::HAS_SECONDS {
    let mut mask = Mask::default();
    let seconds = read_field::<D, _>(cursor, FieldSpec::SECOND, &mut mask)?;
    calendar.seconds = mask.bits();
    calendar.seconds_restricted = seconds.restricted;
  } else {
    calendar.seconds = 1;
    calendar.seconds_restricted = true;
  }

  let mut minutes = Mask::default();
  let minute = read_field::<D, _>(cursor, FieldSpec::MINUTE, &mut minutes)?;
  calendar.minutes = minutes.bits();
  calendar.minutes_restricted = minute.restricted;

  let mut hours = Mask::default();
  let hour = read_field::<D, _>(cursor, FieldSpec::HOUR, &mut hours)?;
  calendar.hours = hours.bits() as u32;
  calendar.hours_restricted = hour.restricted;

  let mut days = Mask::default();
  let dom = read_field::<D, _>(cursor, FieldSpec::DAY_OF_MONTH, &mut days)?;
  calendar.days_of_month = days.bits() as u32;
  calendar.day_of_month_restricted = dom.restricted;
  calendar.day_of_month_wildcard = dom.wildcard;
  if let Some(Modifier::DayOfMonth(modifier)) = dom.modifier {
    calendar.day_of_month_modifier = Some(modifier);
  }

  let mut months = Mask::default();
  let month = read_field::<D, _>(cursor, FieldSpec::MONTH, &mut months)?;
  calendar.months = months.bits() as u16;
  calendar.months_restricted = month.restricted;

  let mut weekdays = Mask::default();
  let dow = read_field::<D, _>(cursor, FieldSpec::day_of_week::<D>(), &mut weekdays)?;
  calendar.days_of_week = weekdays.bits() as u8;
  calendar.day_of_week_restricted = dow.restricted;
  calendar.day_of_week_wildcard = dow.wildcard;
  if let Some(Modifier::DayOfWeek(modifier)) = dow.modifier {
    calendar.day_of_week_modifier = Some(modifier);
  }

  skip_space(cursor);
  if !cursor.at_end() {
    match FieldSpec::year::<D>() {
      Some(spec) => {
        let year = read_field::<D, _>(cursor, spec, &mut calendar.years)?;
        calendar.year_restricted = year.restricted;
      }
      None => {
        return Err(ParseError::new(
          ErrorKind::TrailingInput,
          cursor.next_span().into(),
        ));
      }
    }
    skip_space(cursor);
    if !cursor.at_end() {
      return Err(ParseError::new(
        ErrorKind::TrailingInput,
        cursor.next_span().into(),
      ));
    }
  }

  check_dom_dow::<D>(input, dom, dow)?;
  Ok(calendar)
}

/// Parses one field and the whitespace after it.
fn read_field<D: Dialect, S: crate::field::ValueSink>(
  cursor: &mut Cursor<'_>,
  spec: FieldSpec,
  sink: &mut S,
) -> Result<crate::field::FieldOutcome, ParseError> {
  let outcome = parse_field::<D, S>(cursor, spec, sink)?;
  skip_space(cursor);
  Ok(outcome)
}
