//! The shipped parser against the token-stream parser it replaced.
//!
//! The corpus is the one the scanner's own differential runs on, taken from it rather
//! than copied so the two cannot drift: both parser corpora, every name in every case
//! permutation, exhaustive one- and two-character strings over an alphabet chosen for
//! the places two implementations can disagree, exhaustive three-character strings over
//! the name window, the predicate, macro, whitespace and non-ASCII shapes, and **every
//! prefix of every one of those**. Truncation is where a rewrite diverges, because a
//! parser that runs off the end has to fail in the same place and say the same thing.

#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::panic)]

use crate::{
  dialect::{Cronexpr, Quartz, Robfig, Vixie},
  schedule::Schedule,
};

use super::token::tests::differential::corpus;

/// Asserts the two parsers agree on `input`, in every dialect, result for result.
///
/// `Result<Schedule<D, N>, ParseError>` is compared whole rather than field by field,
/// and that is the point of comparing it this way. `ParseError` carries the kind, the
/// byte span *and* the field attribution; `Schedule` carries every bitset, every
/// restriction flag, both date predicates and the year set. So there is no part of the
/// observable outcome the fusion could have moved without this failing — in particular
/// not the error spans, which are a contract and not a diagnostic nicety.
///
/// Five instantiations rather than four: `Quartz` is checked at `N = 1` and `N = 2`
/// because the year sink is the one place the storage width changes what a legal
/// expression does, and `Cronexpr` is checked because it is the one dialect that admits
/// `H` — the reference is never given a seed, so what this holds is the two refusals,
/// which are the whole of what a seedless parse of an `H` can do.
fn agree(input: &str) {
  assert_eq!(
    Schedule::<Vixie>::parse(input),
    super::parse::<Vixie, 1>(input),
    "Vixie disagrees on {input:?} (left = fused, right = token stream)"
  );
  assert_eq!(
    Schedule::<Quartz>::parse(input),
    super::parse::<Quartz, 1>(input),
    "Quartz disagrees on {input:?} (left = fused, right = token stream)"
  );
  assert_eq!(
    Schedule::<Robfig>::parse(input),
    super::parse::<Robfig, 1>(input),
    "Robfig disagrees on {input:?} (left = fused, right = token stream)"
  );
  assert_eq!(
    Schedule::<Quartz, 2>::parse(input),
    super::parse::<Quartz, 2>(input),
    "Quartz<2> disagrees on {input:?} (left = fused, right = token stream)"
  );
  assert_eq!(
    Schedule::<Cronexpr>::parse(input),
    super::parse::<Cronexpr, 1>(input),
    "Cronexpr disagrees on {input:?} (left = fused, right = token stream)"
  );
}

/// Five thousand expressions across four instantiations: what this looks for is a
/// divergence between the fused parser and the token-stream one, which only a corpus this
/// wide reaches.
#[test]
fn the_fused_parser_matches_the_token_stream_parser() {
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
    corpus.len() > 5_000 && checked > 20_000,
    "the corpus shrank: {} expressions, {checked} parses per instantiation",
    corpus.len()
  );
}

/// The shapes a whole-expression differential has to carry that a lexical one does not.
///
/// The corpus above is built for the *scanner*, so its expressions are mostly one field
/// long. These are whole expressions, and each is here because it reaches a decision the
/// fused parser had to reproduce rather than re-derive: a list that ends on a predicate,
/// a wildcard that turns out not to be one, a lexical failure in the middle of a field,
/// and the trailing year field.
#[test]
fn whole_expressions_agree_too() {
  const EXPRESSIONS: &[&str] = &[
    "30 2 * * 1-5",
    "0,15,30,45 0-23/2 1-15 JAN-JUN MON-FRI",
    "0 30 2 * * 1-5",
    "0 15 10 ? * MON-FRI 2020-2030",
    "0 15 10 LW * ?",
    "0 15 10 ? * 6L",
    "0 15 10 ? * 6#3",
    "0 15 10 L-3 * ?",
    "@every 1h30m45s",
    "@daily",
    "@REBOOT",
    "@nonsense",
    "0 0 * * 99",
    // A `*` beside another item: the wildcard is expanded at field end, not at the item.
    "0 0 *,5 * *",
    "0 0 */1,5 * *",
    // A predicate as one item of a list, caught at the comma and at the field's end.
    "0 15 10 1W,2 * ?",
    "0 15 10 1,2W * ?",
    "0 15 10 ?,1 * *",
    // A lexical failure mid-field, which the token stream reported in the *next* field.
    "1% 2 3 4 5",
    "1 2 3 4 5%",
    "4294967296 * * * *",
    "* 4294967296 * * *",
    // Whitespace of every kind, leading, trailing and doubled.
    "\t30\t2\r\n*\x0C*  1-5 ",
    // Backwards ranges, which wrap in Quartz and fail everywhere else.
    "0 0 0 ? NOV-FEB *",
    "0 0 0 ? * FRI-MON",
    "0 0 0 ? * * 2030-2020",
    // The year field at both ends of what `N` can hold.
    "0 0 0 ? * * 2097",
    "0 0 0 ? * * 2098",
    "0 0 0 ? * * 1969",
    // Steps, including the ones a dialect refuses.
    "5/15 * * * *",
    "*/0 * * * *",
    "0 0 0 ? * * 2020-2030/0",
    // Too few and too many fields.
    "0 0 * *",
    "0 0 0 0 0 0 0 0",
  ];

  for &expression in EXPRESSIONS {
    agree(expression);
    for (boundary, _) in expression.char_indices() {
      agree(&expression[..boundary]);
    }
  }
}

/// Values at and just past every field's bounds, every shape of item, and the atoms
/// that are legal in one field and not in another.
///
/// Shared rather than private because two sweeps need the same ground and a copied list
/// is a list that drifts: this one and `schedule/tests/precedence.rs`, whose span rules
/// have to reach the *field* grammar. Sweeping whole corpus strings does not get them
/// there — the scanner's corpus is mostly one-field text, and one field is the wrong
/// number of fields in every dialect, so the count preflight answers before any field is
/// read. Embedding an atom in a well-shaped expression is what reaches the grammar.
pub(crate) const ATOMS: &[&str] = &[
  "0",
  "1",
  "5",
  "6",
  "7",
  "8",
  "11",
  "12",
  "13",
  "22",
  "23",
  "24",
  "30",
  "31",
  "32",
  "58",
  "59",
  "60",
  "99",
  "1969",
  "1970",
  "1971",
  "2096",
  "2097",
  "2098",
  "2099",
  "2100",
  "*",
  "?",
  "L",
  "W",
  "LW",
  "1W",
  "15W",
  "31W",
  "32W",
  "L-1",
  "L-30",
  "L-31",
  "5L",
  "0L",
  "8L",
  "6#1",
  "6#5",
  "6#6",
  "6#0",
  "JAN",
  "DEC",
  "SUN",
  "SAT",
  "MON",
  "WED",
  "FEB-NOV",
  "MON-FRI",
  "FRI-MON",
  "NOV-FEB",
  "1-5",
  "5-1",
  "0-7",
  "1-7",
  "*/1",
  "*/2",
  "*/0",
  "*/60",
  "1/2",
  "5/15",
  "0-59/5",
  "1,2,3",
  "*,5",
  "?,1",
  "1W,2",
  "1,2W",
  "1-",
  "-1",
  "/",
  "#",
  "1#",
  "",
  // Every predicate spelling as one item of a list, in both positions. These are the
  // shapes whose *span* was wrong in both parsers at once, and the four that begin with a
  // value were not here before: a differential cannot see a span both sides agree on, but
  // the contract sweep can, and it needs the shape to exist.
  "L,2",
  "2,L",
  "LW,2",
  "2,LW",
  "L-3,2",
  "2,L-3",
  "15W,2",
  "2,15W",
  "6L,2",
  "2,6L",
  "6#3,2",
  "2,6#3",
  // Runs that generate values they do not write down. The year field is where a generated
  // value can exceed what the storage holds, and `1970-2099` names 2098 in none of its
  // bytes.
  "1970-2099",
  "1970-2099/1",
  "2090-2099",
  "*/3",
];

/// One well-formed expression per shape a dialect takes: five fields, six, and seven
/// with the year — and both ways round for Quartz's day fields, because which of the
/// two carries the `?` decides what the other one may say.
pub(crate) const BASES: &[&str] = &[
  "0 0 1 1 0",
  "0 0 0 1 1 0",
  "0 0 0 1 1 ? 2020",
  "0 0 0 ? 1 1 2020",
];

/// One atom at a time, in every position of an otherwise well-formed expression.
///
/// The corpus the scanner's differential built is a *lexical* one, and a lexical corpus
/// does not systematically visit a field's bounds: an off-by-one planted in the range
/// check — `value > max` written as `value > max + 1` — survived it, because no
/// expression in it put `max + 1` in a field that had that maximum. This sweep is the
/// half that does. Each case differs from a valid expression in exactly one atom, so a
/// disagreement names the atom and the field it landed in.
#[test]
fn every_atom_in_every_field_position_agrees() {
  for base in BASES {
    let fields: std::vec::Vec<&str> = base.split(' ').collect();
    for index in 0..fields.len() {
      for atom in ATOMS {
        let mut written = fields.clone();
        written[index] = atom;
        agree(&written.join(" "));
      }
    }
  }
}
