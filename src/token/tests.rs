#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::panic)]

use logos::Logos as _;
use std::{string::String, vec::Vec};

use super::{LexError, Token};

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
  Token::lexer(input).collect()
}

fn ok(input: &str) -> Vec<Token<'_>> {
  Token::lexer(input)
    .map(|t| match t {
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
  let mut lexer = Token::lexer("@every 1h30m");
  assert_eq!(lexer.next(), Some(Ok(Token::Macro("@every"))));
  assert_eq!(lexer.span(), 0..6);
  assert_eq!(lexer.next(), Some(Ok(Token::Space)));
  assert_eq!(lexer.span(), 6..7);
  assert_eq!(
    lexer.remainder(),
    "1h30m",
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
    let mut lexer = Token::lexer(input);
    let found = loop {
      match lexer.next() {
        Some(Err(_)) => break Some(lexer.span()),
        Some(Ok(_)) => continue,
        None => break None,
      }
    };
    let span = found.unwrap_or_else(|| panic!("{input:?} lexed cleanly but must not"));
    assert_eq!(span.start, offset, "{input:?} reported the wrong offset");
    assert!(span.end > span.start, "{input:?} reported an empty span");
    assert!(
      input.is_char_boundary(span.start) && input.is_char_boundary(span.end),
      "{input:?} reported a span that splits a character"
    );
  }
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
  let spans: Vec<_> = Token::lexer(input).spanned().collect();
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
