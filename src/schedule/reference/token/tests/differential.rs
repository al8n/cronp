//! The hand scanner against the generated automaton it replaced.
//!
//! The crate's scanner used to be a `#[derive(Logos)]` enum. Replacing a generated
//! automaton by hand is where a subtle divergence gets shipped, so logos is kept as a
//! `[dev-dependencies]` oracle and the two are held against each other here: the same
//! patterns, and an assertion that they produce the same tokens with the same payloads
//! over the same byte ranges.
//!
//! The scanner under test is the reference's, because that is where a token stream still
//! exists. This is the first link of a chain: logos checks this scanner, this scanner
//! feeds the reference grammar, and `reference/tests.rs` checks the shipped parser
//! against that grammar's whole result.
//!
//! The corpus is deliberately not a list of expressions somebody thought of. It is the
//! crate's own parser and lexer corpora, plus exhaustive one-, two- and three-character
//! strings over an alphabet chosen to hit the places the two implementations *could*
//! disagree, plus **every prefix of every one of those** — truncation is where a scanner
//! and an automaton diverge, because an automaton that runs off the end backtracks to its
//! last accepting state and a hand scanner has to be written to do the same.

#![allow(
  clippy::indexing_slicing,
  clippy::unwrap_used,
  clippy::expect_used,
  clippy::panic
)]

use core::ops::Range;

use logos::{Lexer, Logos};
use std::{string::String, vec::Vec};

use super::{SPELLED_OUT, TABLE};
use crate::schedule::reference::token::{LexError, Scanner, Token};

// ---------------------------------------------------------------------------
// The oracle: `src/token.rs` as it was before the hand scanner, verbatim.
// ---------------------------------------------------------------------------

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
enum OracleError {
  #[default]
  UnexpectedCharacter,
  NumberTooLarge,
}

#[derive(Logos, Debug, Clone, Copy, PartialEq, Eq)]
#[logos(error = OracleError)]
enum Oracle<'a> {
  #[token("*")]
  Star,
  #[token("?")]
  Question,
  #[token("/")]
  Slash,
  #[token("-")]
  Hyphen,
  #[token(",")]
  Comma,
  #[token("#")]
  Hash,
  #[regex("[Ll]")]
  Last,
  #[regex("[Ww]")]
  Weekday,
  #[regex("[Hh]")]
  Hashed,
  #[regex("[0-9]+", oracle_number)]
  Number(u32),
  #[regex(
    "[Jj][Aa][Nn]|[Ff][Ee][Bb]|[Mm][Aa][Rr]|[Aa][Pp][Rr]|[Mm][Aa][Yy]|[Jj][Uu][Nn]\
     |[Jj][Uu][Ll]|[Aa][Uu][Gg]|[Ss][Ee][Pp]|[Oo][Cc][Tt]|[Nn][Oo][Vv]|[Dd][Ee][Cc]\
     |[Ss][Uu][Nn]|[Mm][Oo][Nn]|[Tt][Uu][Ee]|[Ww][Ee][Dd]|[Tt][Hh][Uu]|[Ff][Rr][Ii]\
     |[Ss][Aa][Tt]",
    oracle_slice
  )]
  Name(&'a str),
  #[regex("@[a-zA-Z]+", oracle_slice)]
  Macro(&'a str),
  #[regex(r"[ \t\r\n\x0C]+")]
  Space,
}

fn oracle_number<'a>(lex: &Lexer<'a, Oracle<'a>>) -> Result<u32, OracleError> {
  lex
    .slice()
    .parse::<u32>()
    .map_err(|_| OracleError::NumberTooLarge)
}

fn oracle_slice<'a>(lex: &Lexer<'a, Oracle<'a>>) -> &'a str {
  lex.slice()
}

// ---------------------------------------------------------------------------
// A shape both sides can be compared in.
// ---------------------------------------------------------------------------

/// One lexeme, with the failures spelled out as variants.
///
/// Errors are values here rather than a bare "it failed", so that a scanner reporting
/// `UnexpectedCharacter` where the automaton reports `NumberTooLarge` is a divergence
/// rather than a match.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lexeme<'a> {
  Star,
  Question,
  Slash,
  Hyphen,
  Comma,
  Hash,
  Last,
  Weekday,
  Hashed,
  Number(u32),
  Name(&'a str),
  Macro(&'a str),
  Space,
  UnexpectedCharacter,
  NumberTooLarge,
}

fn from_hand<'a>(result: Result<Token<'a>, LexError>) -> Lexeme<'a> {
  match result {
    Ok(Token::Star) => Lexeme::Star,
    Ok(Token::Question) => Lexeme::Question,
    Ok(Token::Slash) => Lexeme::Slash,
    Ok(Token::Hyphen) => Lexeme::Hyphen,
    Ok(Token::Comma) => Lexeme::Comma,
    Ok(Token::Hash) => Lexeme::Hash,
    Ok(Token::Last) => Lexeme::Last,
    Ok(Token::Weekday) => Lexeme::Weekday,
    Ok(Token::Hashed) => Lexeme::Hashed,
    Ok(Token::Number(value)) => Lexeme::Number(value),
    Ok(Token::Name(name)) => Lexeme::Name(name),
    Ok(Token::Macro(name)) => Lexeme::Macro(name),
    Ok(Token::Space) => Lexeme::Space,
    Err(LexError::UnexpectedCharacter) => Lexeme::UnexpectedCharacter,
    Err(LexError::NumberTooLarge) => Lexeme::NumberTooLarge,
  }
}

fn from_oracle<'a>(result: Result<Oracle<'a>, OracleError>) -> Lexeme<'a> {
  match result {
    Ok(Oracle::Star) => Lexeme::Star,
    Ok(Oracle::Question) => Lexeme::Question,
    Ok(Oracle::Slash) => Lexeme::Slash,
    Ok(Oracle::Hyphen) => Lexeme::Hyphen,
    Ok(Oracle::Comma) => Lexeme::Comma,
    Ok(Oracle::Hash) => Lexeme::Hash,
    Ok(Oracle::Last) => Lexeme::Last,
    Ok(Oracle::Weekday) => Lexeme::Weekday,
    Ok(Oracle::Hashed) => Lexeme::Hashed,
    Ok(Oracle::Number(value)) => Lexeme::Number(value),
    Ok(Oracle::Name(name)) => Lexeme::Name(name),
    Ok(Oracle::Macro(name)) => Lexeme::Macro(name),
    Ok(Oracle::Space) => Lexeme::Space,
    Err(OracleError::UnexpectedCharacter) => Lexeme::UnexpectedCharacter,
    Err(OracleError::NumberTooLarge) => Lexeme::NumberTooLarge,
  }
}

type Stream<'a> = Vec<(Lexeme<'a>, Range<usize>)>;

fn by_hand(input: &str) -> Stream<'_> {
  Scanner::new(input)
    .map(|(result, span)| (from_hand(result), span))
    .collect()
}

fn by_oracle(input: &str) -> Stream<'_> {
  Oracle::lexer(input)
    .spanned()
    .map(|(result, span)| (from_oracle(result), span))
    .collect()
}

/// Asserts the two agree token for token and span for span, and returns the input's
/// every prefix having been checked too.
///
/// Comparing whole vectors rather than zipping them is what makes this bidirectional: a
/// stream with an extra token, a missing one, or the right tokens over the wrong ranges
/// all fail, where a zip would silently stop at the shorter one.
fn agree(input: &str) {
  assert_eq!(
    by_hand(input),
    by_oracle(input),
    "the hand scanner and the automaton disagree on {input:?}"
  );
}

// ---------------------------------------------------------------------------
// The corpus.
// ---------------------------------------------------------------------------

/// Characters chosen for where the two implementations could disagree.
///
/// Every punctuation mark that is a token, the `@` that only prefixes one, bytes that
/// begin no token at all, both ends of the digit range, every member of the whitespace
/// class, the two single-letter modifiers in both cases, letters that begin a name and
/// letters that cannot, and characters of two, three and four UTF-8 bytes.
const ALPHABET: &[&str] = &[
  "*", "?", "/", "-", ",", "#", "@", "%", ";", "0", "9", " ", "\t", "\r", "\n", "\x0C", "L", "l",
  "W", "w", "H", "h", "S", "A", "T", "M", "O", "N", "J", "U", "E", "D", "X", "é", "日", "𝄞",
];

/// The subset the exhaustive three-character sweep runs over.
///
/// Three characters is the width of the name window, so this is where a name, a name
/// prefix that fails on its last letter, and a single-letter modifier followed by two
/// more letters all have to be told apart. Held to sixteen so the sweep stays a few
/// thousand cases rather than tens of thousands.
const NAME_WINDOW: &[&str] = &[
  "L", "W", "H", "S", "A", "T", "M", "O", "N", "J", "U", "E", "D", "X", "@", "1", "é",
];

/// Expression shapes the tables do not carry, named for what each one is for.
const SHAPES: &[&str] = &[
  // Predicate forms, which are where `L` and `W` and `#` meet values.
  "LW",
  "lw",
  "1W",
  "15W",
  "L-3",
  "L-30",
  "5#3",
  "5L",
  "SATL",
  "SAT#2",
  "LWLW",
  "L#W",
  // `@`-macros, including ones no dialect knows and ones that are not macros at all.
  "@daily",
  "@REBOOT",
  "@every 1h30m45s",
  "@nonsense",
  "@",
  "@1",
  "@ ",
  "@@daily",
  "@a",
  // Digit runs at and past the edge of `u32`.
  "4294967295",
  "4294967296",
  "4294967297",
  "99999999999999999999",
  "00000000000000000005",
  "0",
  // Whitespace runs of each member of the class, and mixtures.
  " ",
  "\t",
  "\r",
  "\n",
  "\x0C",
  " \t\r\n\x0C",
  "\x0C\n\r\t ",
  "1 \t 2",
  "  *  ",
  // `\x0B` is a whitespace character that this class deliberately excludes.
  "\x0B",
  " \x0B ",
  // Non-ASCII, alone and wedged between tokens, in two, three and four byte forms.
  "é",
  "日",
  "𝄞",
  "*é*",
  "JAé",
  "Jé",
  "@é",
  "Lé",
  "1é2",
  "é\u{0301}",
  "МОН", // Cyrillic homoglyphs of a weekday name
  // Name prefixes that run out, which is where the automaton's backtracking shows.
  "SA",
  "JA",
  "JU",
  "MO",
  "TH",
  "FR",
  "WE",
  "SAX",
  "MAX",
  "SX",
  "A9",
  "AP9",
  "WEX",
  "WEDX",
  "LWED",
  "MONDAY",
  "SATURDAY",
  "*/5 1,2 ? * lw#",
];

pub(crate) fn corpus() -> Vec<String> {
  let mut corpus: Vec<String> = Vec::new();

  // The parser's own table of expressions, and the lexer's.
  corpus.extend(crate::schedule::tests::corpus().map(String::from));
  corpus.extend(TABLE.iter().map(|case| String::from(case.input)));
  corpus.extend(
    super::LEXICALLY_INVALID
      .iter()
      .map(|&(e, _)| String::from(e)),
  );
  corpus.extend(
    super::LEXICALLY_VALID_BUT_UNGRAMMATICAL
      .iter()
      .map(|&e| String::from(e)),
  );
  corpus.extend(SHAPES.iter().map(|&e| String::from(e)));

  // Every name in every case permutation the matcher can distinguish. It folds one bit
  // per letter, so three letters is eight spellings and there is no ninth.
  for name in SPELLED_OUT {
    for mask in 0u8..8 {
      let mut written = String::new();
      for (index, letter) in name.chars().enumerate() {
        if mask >> index & 1 == 1 {
          written.push(letter.to_ascii_lowercase());
        } else {
          written.push(letter);
        }
      }
      corpus.push(written);
    }
  }

  // Every single-byte token alone, and every ordered pair over the alphabet: adjacency
  // is what tells a scanner that consumes one byte too many from one that does not.
  for &first in ALPHABET {
    corpus.push(String::from(first));
    for &second in ALPHABET {
      let mut pair = String::from(first);
      pair.push_str(second);
      corpus.push(pair);
    }
  }

  // And every triple over the name window.
  for &first in NAME_WINDOW {
    for &second in NAME_WINDOW {
      for &third in NAME_WINDOW {
        let mut triple = String::from(first);
        triple.push_str(second);
        triple.push_str(third);
        corpus.push(triple);
      }
    }
  }

  corpus
}

/// Twenty-one thousand scans is the right size for a compiled test job and the wrong size
/// for an interpreter: under Miri this one test takes about 165 s, and CI runs Miri over
/// fourteen target-and-model combinations. Skipping it there costs nothing, because the
/// crate is `#![forbid(unsafe_code)]` and every other test in this module still puts the
/// scanner through Miri — what this test looks for is a lexical divergence, which is a
/// property of the logic and not of the machine it is interpreted on.
#[test]
#[cfg_attr(
  miri,
  ignore = "21k scans is ~165s under an interpreter; runs on every other job"
)]
fn the_hand_scanner_matches_the_generated_automaton() {
  let corpus = corpus();
  let mut checked = 0usize;

  for expression in &corpus {
    agree(expression);
    checked += 1;

    // Every prefix. `char_indices` yields every boundary except the input's length, and
    // the whole expression above is that one; a prefix at any other offset is not a
    // `&str` at all, so this is every prefix that exists.
    for (boundary, _) in expression.char_indices() {
      agree(&expression[..boundary]);
      checked += 1;
    }
  }

  assert!(
    corpus.len() > 2_000 && checked > 8_000,
    "the corpus shrank: {} expressions, {checked} scans",
    corpus.len()
  );
}
