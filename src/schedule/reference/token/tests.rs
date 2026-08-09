#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::panic)]

use core::ops::Range;
use std::{string::String, vec::Vec};

use super::{Cursor, LexError, Scanner, Token};
use crate::token::{key, name_index, NAMES};

pub(crate) mod differential;

/// The name of a token variant.
///
/// The match is exhaustive on purpose: adding a variant to [`Token`] without deciding
/// what it is called here is a compile error, which is the first half of keeping
/// [`ALL_TOKEN_NAMES`] honest.
fn name_of(token: &Token<'_>) -> &'static str {
  match token {
    Token::Star => "Star",
    Token::Question => "Question",
    Token::Slash => "Slash",
    Token::Hyphen => "Hyphen",
    Token::Comma => "Comma",
    Token::Hash => "Hash",
    Token::Last => "Last",
    Token::Weekday => "Weekday",
    Token::Number(_) => "Number",
    Token::Name(_) => "Name",
    Token::Macro(_) => "Macro",
    Token::Space => "Space",
  }
}

/// Every variant the lexer can produce.
///
/// The second half of the coupling with [`name_of`]: the coverage test compares this
/// list against what the table below actually produced, in both directions, so a variant
/// that is listed but never lexed and a variant that is lexed but never listed both fail.
const ALL_TOKEN_NAMES: &[&str] = &[
  "Star", "Question", "Slash", "Hyphen", "Comma", "Hash", "Last", "Weekday", "Number", "Name",
  "Macro", "Space",
];

fn lex(input: &str) -> Vec<Result<Token<'_>, LexError>> {
  Scanner::new(input).map(|(token, _)| token).collect()
}

fn spanned(input: &str) -> Vec<(Result<Token<'_>, LexError>, Range<usize>)> {
  Scanner::new(input).collect()
}

fn ok(input: &str) -> Vec<Token<'_>> {
  Scanner::new(input)
    .map(|(t, _)| match t {
      Ok(token) => token,
      Err(e) => panic!("{input:?} failed to lex: {e:?}"),
    })
    .collect()
}

/// Input, the tokens it must produce, and which dialect's surface needs it.
///
/// The `why` column is not decoration: it is what makes the coverage assertion below
/// able to say that every dialect's lexical surface is represented rather than merely
/// that twelve variants were touched.
struct Case {
  input: &'static str,
  /// `None` when the input's tail is deliberately not cron syntax, in which case the
  /// case is asserted by its own test rather than by the table walk.
  expected: Option<&'static [Token<'static>]>,
  why: &'static str,
}

const TABLE: &[Case] = &[
  Case {
    input: "*",
    expected: Some(&[Token::Star]),
    why: "every dialect",
  },
  Case {
    input: "?",
    expected: Some(&[Token::Question]),
    why: "Quartz requires it in one of dom/dow; robfig accepts it; Vixie has no ?",
  },
  Case {
    input: "*/15",
    expected: Some(&[Token::Star, Token::Slash, Token::Number(15)]),
    why: "every dialect: step over the whole range",
  },
  Case {
    input: "1-5",
    expected: Some(&[Token::Number(1), Token::Hyphen, Token::Number(5)]),
    why: "every dialect: a range",
  },
  Case {
    input: "1,15,30",
    expected: Some(&[
      Token::Number(1),
      Token::Comma,
      Token::Number(15),
      Token::Comma,
      Token::Number(30),
    ]),
    why: "every dialect: a list",
  },
  Case {
    input: "7",
    expected: Some(&[Token::Number(7)]),
    why: "legal day-of-week in Vixie (Sunday) and in Quartz (Saturday) — the same \
           digit, different days. The lexer must not take a side.",
  },
  Case {
    input: "L",
    expected: Some(&[Token::Last]),
    why: "Quartz only: last day of month",
  },
  Case {
    input: "l",
    expected: Some(&[Token::Last]),
    why: "Quartz modifiers are case-insensitive",
  },
  Case {
    input: "L-3",
    expected: Some(&[Token::Last, Token::Hyphen, Token::Number(3)]),
    why: "Quartz only: three days before the last day of the month",
  },
  Case {
    input: "LW",
    expected: Some(&[Token::Last, Token::Weekday]),
    why: "Quartz only: last weekday of the month. Two tokens, not one name.",
  },
  Case {
    input: "15W",
    expected: Some(&[Token::Number(15), Token::Weekday]),
    why: "Quartz only: the weekday nearest the 15th",
  },
  Case {
    input: "6#3",
    expected: Some(&[Token::Number(6), Token::Hash, Token::Number(3)]),
    why: "Quartz only: the third Friday of the month",
  },
  Case {
    input: "JAN",
    expected: Some(&[Token::Name("JAN")]),
    why: "every dialect: month names",
  },
  Case {
    input: "mon-fri",
    expected: Some(&[Token::Name("mon"), Token::Hyphen, Token::Name("fri")]),
    why: "every dialect: weekday names, lower case, in a range",
  },
  Case {
    input: "SAT",
    expected: Some(&[Token::Name("SAT")]),
    why: "a three-letter name starting with S, not to be confused with a modifier",
  },
  Case {
    input: "WED",
    expected: Some(&[Token::Name("WED")]),
    why: "starts with W: the longest match must win over the W modifier",
  },
  Case {
    input: "@daily",
    expected: Some(&[Token::Macro("@daily")]),
    why: "Vixie and robfig: the nickname macros",
  },
  Case {
    input: "@reboot",
    expected: Some(&[Token::Macro("@reboot")]),
    why: "Vixie only, and legal Vixie: the lexer must not reject it",
  },
  Case {
    input: "@every 1h30m",
    expected: None,
    why: "robfig only: a duration rather than a set of instants. Its tail is not cron \
           syntax, so it is asserted by its own test.",
  },
  Case {
    input: "0 0 * * *",
    expected: Some(&[
      Token::Number(0),
      Token::Space,
      Token::Number(0),
      Token::Space,
      Token::Star,
      Token::Space,
      Token::Star,
      Token::Space,
      Token::Star,
    ]),
    why: "Vixie: five fields, whitespace is the separator and therefore a token",
  },
];

#[test]
fn lexes_the_table() {
  for case in TABLE {
    let Some(expected) = case.expected else {
      continue;
    };
    let got = ok(case.input);
    assert_eq!(got.as_slice(), expected, "{:?} ({})", case.input, case.why);
  }
}

/// `@every` is a token; the duration after it is not cron syntax and is not lexed here.
#[test]
fn every_lexes_as_a_macro_and_leaves_its_duration_alone() {
  let tokens = spanned("@every 1h30m");
  assert_eq!(tokens[0], (Ok(Token::Macro("@every")), 0..6));
  assert_eq!(tokens[1], (Ok(Token::Space), 6..7));

  // The tail is read off the cursor rather than lexed, so it must survive intact.
  let mut cursor = Cursor::new("@every 1h30m");
  cursor.bump();
  cursor.bump();
  assert_eq!(
    cursor.rest(),
    ("1h30m", 7),
    "the duration tail must survive intact for the duration parser"
  );
}

#[test]
fn the_table_covers_every_listed_token_variant() {
  let mut seen: Vec<&'static str> = TABLE
    .iter()
    .filter_map(|c| c.expected)
    .flat_map(|expected| expected.iter().map(name_of))
    .collect();
  seen.sort_unstable();
  seen.dedup();

  let mut listed: Vec<&'static str> = ALL_TOKEN_NAMES.to_vec();
  listed.sort_unstable();

  assert_eq!(
    seen, listed,
    "left = produced by the table, right = ALL_TOKEN_NAMES"
  );
}

#[test]
fn the_table_covers_every_dialect() {
  // Three dialects, so three distinct dialect attributions must appear. Counting the
  // cases would prove nothing about coverage; counting the dialects named is the claim
  // the test's name makes.
  let mentions_vixie = TABLE.iter().any(|c| c.why.contains("Vixie"));
  let mentions_quartz = TABLE.iter().any(|c| c.why.contains("Quartz"));
  let mentions_robfig = TABLE.iter().any(|c| c.why.contains("robfig"));
  assert!(mentions_vixie && mentions_quartz && mentions_robfig);

  let universal = TABLE
    .iter()
    .filter(|c| c.why.contains("every dialect"))
    .count();
  assert!(
    universal >= 4,
    "the shared surface (*, ranges, lists, steps) must be in the table too"
  );
}

// ---------------------------------------------------------------------------
// The name set, and what it costs to get it wrong.
// ---------------------------------------------------------------------------

/// The nineteen names, spelled out independently of [`NAMES`].
///
/// Two spellings of the same set, so that a typo in the packed table is a test failure
/// rather than a month that stops parsing.
const SPELLED_OUT: [&str; 19] = [
  "JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC", "SUN", "MON",
  "TUE", "WED", "THU", "FRI", "SAT",
];

#[test]
fn the_packed_table_holds_exactly_the_nineteen_names() {
  let mut packed: Vec<u32> = NAMES.to_vec();
  packed.sort_unstable();
  packed.dedup();
  assert_eq!(
    packed.len(),
    NAMES.len(),
    "the packed table has a duplicate"
  );

  let mut spelled: Vec<u32> = SPELLED_OUT
    .iter()
    .map(|name| {
      let b = name.as_bytes();
      key(b[0], b[1], b[2])
    })
    .collect();
  spelled.sort_unstable();
  assert_eq!(
    packed, spelled,
    "the packed table is not the nineteen names"
  );

  for name in SPELLED_OUT {
    assert_eq!(
      ok(name).as_slice(),
      &[Token::Name(name)],
      "{name:?} must lex as one name"
    );
  }
}

/// `LW` is two modifiers, and it stays that way only because the name set is closed.
///
/// The names are a spelled-out set rather than "any three letters" precisely so that
/// Quartz's `LW` — last weekday of the month — lexes as [`Token::Last`] then
/// [`Token::Weekday`] instead of being swallowed as one three-letter name. Matching
/// three letters generically, or adding a name beginning `LW`, breaks the day-of-month
/// predicate; this fails if either happens.
#[test]
fn lw_is_two_modifiers_not_a_name() {
  for input in ["LW", "lw", "Lw", "lW"] {
    assert_eq!(
      ok(input).as_slice(),
      &[Token::Last, Token::Weekday],
      "{input:?} must be two modifiers"
    );
  }
  assert!(
    name_index(key(b'L', b'W', b'X')).is_none()
      && !NAMES.iter().any(|name| name >> 8 == key(0, b'L', b'W')),
    "no name may begin with LW, or `LW` stops being two tokens"
  );

  // The other half of the same rule: three letters that *are* a name still win over the
  // single-letter modifier that starts them, so `W` alone and `WED` are different
  // tokens and `L` followed by a name is two tokens rather than a mangled one.
  assert_eq!(ok("W").as_slice(), &[Token::Weekday]);
  assert_eq!(ok("WED").as_slice(), &[Token::Name("WED")]);
  assert_eq!(
    ok("LWED").as_slice(),
    &[Token::Last, Token::Name("WED")],
    "the name must still win once L has been taken"
  );
  assert_eq!(
    lex("LWX").as_slice(),
    &[
      Ok(Token::Last),
      Ok(Token::Weekday),
      Err(LexError::UnexpectedCharacter)
    ],
    "a letter that begins no name does not extend the modifiers before it"
  );
}

#[test]
fn names_are_case_insensitive_in_every_permutation() {
  for name in SPELLED_OUT {
    for mask in 0u8..8 {
      let mut written = String::new();
      for (index, ch) in name.chars().enumerate() {
        if mask >> index & 1 == 1 {
          written.push(ch.to_ascii_lowercase());
        } else {
          written.push(ch);
        }
      }
      assert_eq!(
        ok(&written).as_slice(),
        &[Token::Name(written.as_str())],
        "{written:?} must lex as one name, in the case it was written"
      );
    }
  }
}

// ---------------------------------------------------------------------------
// Malformed corpus.
//
// The plan asks that every malformed input "produce an error token rather than a
// panic". Only some of these are malformed *lexically*: `1-`, a lone `/` and `#` with
// no digit are all perfectly good token sequences that no dialect's grammar accepts.
// Splitting the corpus in two says which layer rejects what, instead of asserting a
// lexer error that a total lexer is never going to produce.
// ---------------------------------------------------------------------------

/// Inputs the lexer itself must reject, each with the byte offset it must report.
const LEXICALLY_INVALID: &[(&str, usize)] = &[
  ("%", 0),
  ("0 0 % * *", 4),
  ("0 0 * * é", 8),
  ("日", 0),
  ("0 0 * * *;", 9),
  ("4294967296", 0), // one past u32::MAX: a digit run that cannot be a Number
];

#[test]
fn lexically_invalid_input_errors_with_a_byte_offset() {
  for &(input, offset) in LEXICALLY_INVALID {
    let found = spanned(input)
      .into_iter()
      .find_map(|(token, span)| token.is_err().then_some(span));
    let span = found.unwrap_or_else(|| panic!("{input:?} lexed cleanly but must not"));
    assert_eq!(span.start, offset, "{input:?} reported the wrong offset");
    assert!(span.end > span.start, "{input:?} reported an empty span");
    assert!(
      input.is_char_boundary(span.start) && input.is_char_boundary(span.end),
      "{input:?} reported a span that splits a character"
    );
  }
}

/// A digit run too long for `u32` is its own failure, not an unexpected character.
#[test]
fn an_overlong_digit_run_says_so() {
  assert_eq!(
    spanned("4294967296").as_slice(),
    &[(Err(LexError::NumberTooLarge), 0..10)]
  );
  // A run that overflows and then wraps back into range is still too long.
  assert_eq!(
    spanned("4294967297").as_slice(),
    &[(Err(LexError::NumberTooLarge), 0..10)]
  );
  // Leading zeros are not overflow, however many there are.
  assert_eq!(
    spanned("00000000000000000005").as_slice(),
    &[(Ok(Token::Number(5)), 0..20)]
  );
}

/// Inputs that lex cleanly and are the grammar's problem, not the lexer's.
///
/// Listed so that the split is a recorded decision rather than a gap in the corpus.
const LEXICALLY_VALID_BUT_UNGRAMMATICAL: &[&str] = &[
  "1-",      // unterminated range
  "/",       // a lone step marker
  "#",       // a hash with no digit
  "1#",      // nth-weekday with no n
  "*/",      // step with no value
  ",",       // an empty list
  "1--2",    // two hyphens
  "0 0 * *", // four fields: too few for every dialect
];

#[test]
fn ungrammatical_input_still_lexes_without_error() {
  for &input in LEXICALLY_VALID_BUT_UNGRAMMATICAL {
    let results = lex(input);
    assert!(
      results.iter().all(Result::is_ok),
      "{input:?} must reach the parser, which is what rejects it"
    );
  }
}

#[test]
fn degenerate_input_does_not_panic() {
  assert!(lex("").is_empty(), "the empty string yields no tokens");
  assert_eq!(lex("   ").as_slice(), &[Ok(Token::Space)]);
  assert_eq!(lex("\t\r\n").as_slice(), &[Ok(Token::Space)]);

  // A 4 KiB expression. Long, well-formed, and nothing here may allocate unboundedly
  // or recurse; the lexer is a state machine and must stay one.
  // 1400 two-digit values separated by commas: 1400*2 + 1399 = 4199 bytes.
  const ITEMS: usize = 1400;
  let mut long = String::new();
  for i in 0..ITEMS {
    if i > 0 {
      long.push(',');
    }
    long.push_str("59");
  }
  assert!(long.len() >= 4096, "corpus input is {} bytes", long.len());
  let results = lex(&long);
  assert_eq!(results.len(), ITEMS + (ITEMS - 1));
  assert!(results.iter().all(Result::is_ok));
}

#[test]
fn spans_are_byte_offsets_into_the_original_input() {
  let input = "0 15 10 ? * MON-FRI";
  let spans = spanned(input);
  for (token, span) in &spans {
    assert!(token.is_ok(), "{input:?} must lex cleanly");
    assert!(
      input.get(span.clone()).is_some(),
      "span {span:?} is not a slice of the input"
    );
  }
  let (last, last_span) = spans.last().unwrap();
  assert_eq!(last.as_ref().unwrap(), &Token::Name("FRI"));
  assert_eq!(*last_span, 16..19);
}

/// The spans partition the input: no gap, no overlap, and nothing left over.
///
/// A hand scanner fails by forgetting to advance or by advancing twice, and both show up
/// here on any input at all rather than only on the one that was thought of.
#[test]
fn the_spans_tile_the_whole_input() {
  for input in ["30 2 * * 1-5", "0 15 10 LW * ?", "é%SA JAN@ \t4294967296"] {
    let mut at = 0usize;
    for (_, span) in spanned(input) {
      assert_eq!(span.start, at, "{input:?} left a gap or overlapped");
      assert!(span.end > span.start, "{input:?} produced an empty span");
      at = span.end;
    }
    assert_eq!(at, input.len(), "{input:?} was not scanned to the end");
  }
}
