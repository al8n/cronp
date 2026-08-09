#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::panic)]

use std::{string::String, vec::Vec};

use super::{Cursor, Lexeme, MONTHS, NAMES, Word, is_space_byte, key, name_index};
use crate::schedule::reference::token::{LexError, Scanner, Token};

/// The nineteen names, spelled out in the order the table has to hold them.
///
/// A second spelling of the same set, so that a typo in the packed table is a test
/// failure rather than a month that stops parsing.
const SPELLED_OUT: [&str; 19] = [
  "JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC", "SUN", "MON",
  "TUE", "WED", "THU", "FRI", "SAT",
];

/// The table's *order* is load-bearing, and nothing else says so.
///
/// [`name_value`](crate::field) resolves a name by arithmetic on its index: a month is
/// `index + 1` and a weekday is `index - MONTHS`. That is only correct while the table
/// is the twelve months in calendar order followed by the seven weekdays from Sunday.
/// Reordering it, or inserting a name, silently renumbers every month and every weekday,
/// so the order is pinned here as a fact rather than left as a convention.
#[test]
fn the_table_is_the_months_then_the_weekdays_in_order() {
  assert_eq!(NAMES.len(), SPELLED_OUT.len());
  assert_eq!(
    usize::from(MONTHS),
    12,
    "twelve months, and then the weekdays"
  );

  for (index, name) in SPELLED_OUT.iter().enumerate() {
    let bytes = name.as_bytes();
    assert_eq!(
      name_index(key(bytes[0], bytes[1], bytes[2])),
      Some(index as u8),
      "{name} is not at index {index}"
    );
  }

  let mut sorted: Vec<u32> = NAMES.to_vec();
  sorted.sort_unstable();
  sorted.dedup();
  assert_eq!(
    sorted.len(),
    NAMES.len(),
    "the packed table has a duplicate"
  );
}

/// `LW` is two lexemes, and it stays that way only because the name set is closed.
///
/// The names are a spelled-out set rather than "any three letters" precisely so that
/// Quartz's `LW` — last weekday of the month — reads as `L` then `W` instead of being
/// swallowed as one three-letter name. The other half of the same rule is that a name
/// still wins over the single letter that starts it, which is what makes `W` and `WED`
/// different lexemes.
#[test]
fn lw_is_two_modifiers_and_a_name_still_wins() {
  for input in ["LW", "lw", "Lw", "lW"] {
    let mut cursor = Cursor::new(input);
    assert_eq!(cursor.take_word(), Word::Last, "{input:?}");
    assert_eq!(cursor.take_word(), Word::Weekday, "{input:?}");
    assert!(cursor.at_end(), "{input:?}");
  }

  assert!(
    name_index(key(b'L', b'W', b'X')).is_none()
      && !NAMES.iter().any(|name| name >> 8 == key(0, b'L', b'W')),
    "no name may begin with LW, or `LW` stops being two lexemes"
  );

  assert_eq!(Cursor::new("W").take_word(), Word::Weekday);
  assert_eq!(Cursor::new("WED").take_word(), Word::Name(15));
  assert_eq!(Cursor::new("wed").take_word(), Word::Name(15));

  let mut cursor = Cursor::new("LWED");
  assert_eq!(cursor.take_word(), Word::Last);
  assert_eq!(
    cursor.take_word(),
    Word::Name(15),
    "the name must still win once L has been taken"
  );
}

/// A digit run's value comes back, or the fact that no field could hold it.
#[test]
fn a_digit_run_yields_its_value_until_it_cannot() {
  assert_eq!(Cursor::new("0").take_number(), Some(0));
  assert_eq!(Cursor::new("4294967295").take_number(), Some(u32::MAX));
  assert_eq!(Cursor::new("4294967296").take_number(), None);
  // A run that overflows and then wraps back into range is still too long.
  assert_eq!(Cursor::new("4294967297").take_number(), None);
  // Leading zeros are not overflow, however many there are.
  assert_eq!(Cursor::new("00000000000000000005").take_number(), Some(5));

  // The run stops at the first byte that is not a digit, and the cursor stops with it.
  let mut cursor = Cursor::new("15W");
  assert_eq!(cursor.take_number(), Some(15));
  assert_eq!(cursor.pos(), 2);
  assert_eq!(cursor.take_word(), Word::Weekday);
}

/// Looking ahead must not move.
///
/// The fused parser peeks in several places to decide which grammar rule applies, and a
/// peek that advanced would drop a lexeme with no other symptom than a wrong answer far
/// away from here.
#[test]
fn peeking_never_moves_the_cursor() {
  for input in ["30 2 * * 1-5", "WED#3", "@every 1h", "%", "4294967296", ""] {
    let cursor = Cursor::new(input);
    let before = cursor.pos();
    let _ = cursor.peek();
    let _ = cursor.peek_lexeme();
    let _ = cursor.peek_word();
    let _ = cursor.next_span();
    assert_eq!(cursor.pos(), before, "a peek moved the cursor on {input:?}");
  }
}

/// A peek says exactly what the take that follows it does.
#[test]
fn a_peek_and_the_take_after_it_agree() {
  for input in [
    "30 2 * * 1-5",
    "0 15 10 LW * ?",
    "é%SA JAN@ \t4294967296",
    "@every 1h30m",
  ] {
    let mut cursor = Cursor::new(input);
    while let Some((peeked, span)) = cursor.peek_lexeme() {
      let taken = cursor.take_lexeme();
      assert_eq!(Some(peeked), taken, "{input:?} at {span:?}");
      assert_eq!(cursor.pos(), span.end, "{input:?} at {span:?}");
    }
    assert!(cursor.at_end(), "{input:?} was not scanned to the end");
  }
}

/// The recogniser against the token stream it replaced, lexeme for lexeme.
///
/// The parser-level differential covers the whole grammar, but it exercises
/// [`Cursor::take_lexeme`] only where the parser is on its way to an error — the fast
/// paths test their bytes directly and never call it. This holds the classifier itself
/// against the reference scanner over the scanner's own corpus, so the cold path is
/// checked as closely as the hot one.
#[test]
#[cfg_attr(
  miri,
  ignore = "21k scans is ~165s under an interpreter; runs on every other job"
)]
fn take_lexeme_matches_the_reference_scanner() {
  fn kind_of(token: &Result<Token<'_>, LexError>) -> Lexeme {
    match token {
      Ok(Token::Star) => Lexeme::Star,
      Ok(Token::Question) => Lexeme::Question,
      Ok(Token::Slash) => Lexeme::Slash,
      Ok(Token::Hyphen) => Lexeme::Hyphen,
      Ok(Token::Comma) => Lexeme::Comma,
      Ok(Token::Hash) => Lexeme::Hash,
      Ok(Token::Last) => Lexeme::Last,
      Ok(Token::Weekday) => Lexeme::Weekday,
      Ok(Token::Number(_)) => Lexeme::Number,
      Ok(Token::Name(_)) => Lexeme::Name,
      Ok(Token::Macro(_)) => Lexeme::Macro,
      Ok(Token::Space) => Lexeme::Space,
      Err(LexError::UnexpectedCharacter) => Lexeme::UnexpectedCharacter,
      Err(LexError::NumberTooLarge) => Lexeme::NumberTooLarge,
    }
  }

  let corpus = crate::schedule::reference::token::tests::differential::corpus();
  assert!(corpus.len() > 5_000, "the corpus shrank: {}", corpus.len());

  for expression in &corpus {
    let expected: Vec<(Lexeme, core::ops::Range<usize>)> = Scanner::new(expression)
      .map(|(token, span)| (kind_of(&token), span))
      .collect();

    let mut cursor = Cursor::new(expression);
    let mut found: Vec<(Lexeme, core::ops::Range<usize>)> = Vec::new();
    while let Some((lexeme, span)) = cursor.peek_lexeme() {
      cursor.take_lexeme();
      found.push((lexeme, span));
    }

    assert_eq!(found, expected, "the two disagree on {expression:?}");
  }
}

/// Every lexeme's span is a slice of the input, and the spans tile it.
///
/// A hand scanner fails by forgetting to advance or by advancing twice, and both show up
/// here on any input at all rather than only on the one that was thought of. A scan that
/// forgot to advance would also not terminate, which this catches by construction.
#[test]
fn the_spans_tile_the_whole_input() {
  for input in [
    "30 2 * * 1-5",
    "0 15 10 LW * ?",
    "é%SA JAN@ \t4294967296",
    "\u{1d11e}",
  ] {
    let mut cursor = Cursor::new(input);
    let mut at = 0usize;
    while let Some((_, span)) = cursor.peek_lexeme() {
      cursor.take_lexeme();
      assert_eq!(span.start, at, "{input:?} left a gap or overlapped");
      assert!(span.end > span.start, "{input:?} produced an empty span");
      assert!(
        input.get(span.clone()).is_some(),
        "span {span:?} is not a slice of {input:?}"
      );
      at = span.end;
    }
    assert_eq!(at, input.len(), "{input:?} was not scanned to the end");
  }
}

/// Whitespace is consumed as a run, because the field separator is the whole run.
#[test]
fn whitespace_is_skipped_a_run_at_a_time() {
  const CLASS: &str = " \t\r\n\x0C";
  for byte in CLASS.bytes() {
    assert!(is_space_byte(byte));
  }
  // `\x0B` is a whitespace character this class deliberately excludes.
  assert!(!is_space_byte(b'\x0B'));

  let mut cursor = Cursor::new(" \t\r\n\x0C*");
  cursor.skip_space();
  assert_eq!(cursor.pos(), 5);
  assert_eq!(cursor.peek(), Some(b'*'));

  // Skipping when there is nothing to skip is not an advance.
  let mut cursor = Cursor::new("*");
  cursor.skip_space();
  assert_eq!(cursor.pos(), 0);
}

/// `@every`'s duration is not cron syntax, so the tail has to survive the scan intact.
#[test]
fn a_nickname_leaves_its_duration_alone() {
  let mut cursor = Cursor::new("@every 1h30m");
  assert_eq!(cursor.take_macro(), Some("@every"));
  cursor.skip_space();
  assert_eq!(cursor.rest(), ("1h30m", 7));

  // A lone `@` is not a nickname, and neither is `@` followed by a digit.
  assert_eq!(Cursor::new("@").take_macro(), None);
  assert_eq!(Cursor::new("@1").take_macro(), None);
  assert_eq!(Cursor::new("@@daily").take_macro(), None);
}

/// A 4 KiB expression. Long, well-formed, and nothing here may recurse or allocate; the
/// scanner is a loop over bytes and must stay one.
#[test]
fn a_long_expression_is_scanned_without_recursion() {
  const ITEMS: usize = 1400;
  let mut long = String::new();
  for index in 0..ITEMS {
    if index > 0 {
      long.push(',');
    }
    long.push_str("59");
  }
  assert!(long.len() >= 4096, "corpus input is {} bytes", long.len());

  let mut cursor = Cursor::new(&long);
  let mut lexemes = 0usize;
  while cursor.take_lexeme().is_some() {
    lexemes += 1;
  }
  assert_eq!(lexemes, ITEMS + (ITEMS - 1));
}
