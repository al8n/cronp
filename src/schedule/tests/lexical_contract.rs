//! Where a lexical failure is reported, generated — and computed from templates.
//!
//! `schedule/tests.rs` pins a hand-written table of expressions and the errors they must
//! produce. This is the same contract taken to its cross product: every dialect, every
//! field position of every shape that dialect takes, every lexical failure the scanner
//! can raise, every place inside a field a bad token can sit, and every layout of
//! whitespace around and between the fields.
//!
//! # Why none of it asks a parser
//!
//! The differential in `schedule/reference/tests.rs` cannot see a fault that is present
//! in the shipped parser and in the reference at the same time. That is not a gap in its
//! corpus — 5,566 expressions, every prefix, four instantiations — it is what an oracle
//! is blind to by construction, and it was demonstrated rather than argued: with the two
//! attribution fixes reverted on *both* sides at once the whole differential stayed
//! green, and only the contract table was red.
//!
//! Generating cases does not help with that on its own. A generated suite that asks the
//! reference what to expect reproduces the same blindness over a larger corpus and looks
//! like progress while doing it. So every expectation here — the [`ErrorKind`], the byte
//! span and the [`FieldKind`] — is arithmetic over the template that wrote the
//! expression: the layout's leading whitespace, the fields before this one, the
//! separators between them, and the offset of the bad token inside its own field. The
//! only thing a parser is asked is what it actually did.
//!
//! # Both sides of the field count
//!
//! Every expression the matrix writes is in shape for the dialect it is parsed under, so
//! that the field-count preflight never fires and every case reaches a field read. That
//! is the matrix's subject: where a bad byte is reported *within* a well-shaped
//! expression.
//!
//! The other side is its own contract and gets its own generated test.
//! [`the_field_count_preflight_runs_before_any_field_is_read`] takes the same cross
//! product and parses it under the dialects each shape is *wrong* for, where every case
//! answers with the count instead. Together the two cover the whole product on both
//! sides of the preflight, and
//! [`a_bad_byte_does_not_outrank_a_wrong_field_count`] says why the precedence is that
//! way round.
//!
//! # The seam
//!
//! [`the_reference_parser_cannot_change_without_this_contract_changing_with_it`] pins a
//! digest of each of the reference parser's source files, so a simultaneous edit stops
//! the test suite by name instead of relying on a reviewer noticing one.

#![allow(
  clippy::indexing_slicing,
  clippy::unwrap_used,
  clippy::expect_used,
  clippy::panic
)]

use std::string::String;

use crate::{
  dialect::{Quartz, Robfig, Vixie},
  error::{ErrorKind, FieldKind, ParseError},
  schedule::Schedule,
};

// ---------------------------------------------------------------------------
// The axes.
// ---------------------------------------------------------------------------

/// Which instantiation an expression is parsed under.
///
/// Quartz appears at both widths because the year sink is the one place `N` changes what
/// a legal expression does. A lexical failure must be unaffected by it, and the way to
/// know that is to ask both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Under {
  Vixie,
  Quartz1,
  Quartz2,
  Robfig,
}

impl Under {
  /// The instantiation, spelled as a caller would write it.
  const fn name(self) -> &'static str {
    match self {
      Self::Vixie => "Schedule<Vixie, 1>",
      Self::Quartz1 => "Schedule<Quartz, 1>",
      Self::Quartz2 => "Schedule<Quartz, 2>",
      Self::Robfig => "Schedule<Robfig, 1>",
    }
  }

  /// The dialect's name, as an error carries it.
  const fn dialect(self) -> &'static str {
    match self {
      Self::Vixie => "Vixie",
      Self::Quartz1 | Self::Quartz2 => "Quartz",
      Self::Robfig => "Robfig",
    }
  }

  /// The fewest and the most whitespace-separated fields the dialect takes.
  ///
  /// Written out rather than read from `Dialect::MIN_FIELDS`, for the same reason every
  /// span here is: an expectation taken from the code under test is not an expectation.
  const fn bounds(self) -> (usize, usize) {
    match self {
      Self::Vixie => (5, 5),
      Self::Quartz1 | Self::Quartz2 => (6, 7),
      Self::Robfig => (6, 6),
    }
  }

  /// Parses `input`, keeping only whether it was accepted and, if not, why.
  ///
  /// The schedule itself is dropped: what is under test here is the error, and the four
  /// instantiations are four different types that could not otherwise be compared.
  fn parse(self, input: &str) -> Result<(), ParseError> {
    match self {
      Self::Vixie => Schedule::<Vixie, 1>::parse(input).map(|_| ()),
      Self::Quartz1 => Schedule::<Quartz, 1>::parse(input).map(|_| ()),
      Self::Quartz2 => Schedule::<Quartz, 2>::parse(input).map(|_| ()),
      Self::Robfig => Schedule::<Robfig, 1>::parse(input).map(|_| ()),
    }
  }
}

/// One well-formed expression, field by field, and what each position is called.
///
/// Both halves are written out and neither is read back from a parser. `fields` is the
/// template a case is a one-field mutation of, and `kinds` is the attribution the parser
/// owes — which is precisely the surface a simultaneous edit moves without any
/// differential noticing.
struct Shape {
  /// The shape, for a failure message.
  what: &'static str,
  /// A well-formed expression in this shape.
  fields: &'static [&'static str],
  /// What each field position is called.
  kinds: &'static [FieldKind],
  /// The instantiations this shape is legal under.
  under: &'static [Under],
}

const SHAPES: &[Shape] = &[
  Shape {
    what: "Vixie, five fields",
    fields: &["0", "0", "1", "1", "0"],
    kinds: &[
      FieldKind::Minute,
      FieldKind::Hour,
      FieldKind::DayOfMonth,
      FieldKind::Month,
      FieldKind::DayOfWeek,
    ],
    under: &[Under::Vixie],
  },
  Shape {
    what: "Quartz, six fields",
    fields: &["0", "0", "0", "?", "1", "1"],
    kinds: &[
      FieldKind::Second,
      FieldKind::Minute,
      FieldKind::Hour,
      FieldKind::DayOfMonth,
      FieldKind::Month,
      FieldKind::DayOfWeek,
    ],
    under: &[Under::Quartz1, Under::Quartz2],
  },
  Shape {
    what: "Quartz, seven fields with the trailing year",
    fields: &["0", "0", "0", "?", "1", "1", "2020"],
    kinds: &[
      FieldKind::Second,
      FieldKind::Minute,
      FieldKind::Hour,
      FieldKind::DayOfMonth,
      FieldKind::Month,
      FieldKind::DayOfWeek,
      FieldKind::Year,
    ],
    under: &[Under::Quartz1, Under::Quartz2],
  },
  Shape {
    what: "Robfig, six fields",
    fields: &["0", "0", "0", "1", "1", "0"],
    kinds: &[
      FieldKind::Second,
      FieldKind::Minute,
      FieldKind::Hour,
      FieldKind::DayOfMonth,
      FieldKind::Month,
      FieldKind::DayOfWeek,
    ],
    under: &[Under::Robfig],
  },
];

/// A lexical failure: the text that raises it, and how much of that text is reported.
///
/// `span_len` is a claim about the scanner rather than a measurement of it, and the two
/// are not always the length of `text`: `MAX` is three bytes written and two bytes
/// reported, because `MA` could still have become `MAR` while `X` could not continue it.
/// A non-ASCII character is reported whole so that every span stays sliceable.
struct Failure {
  /// What is written into the field.
  text: &'static str,
  /// How many bytes the report must cover.
  span_len: usize,
  /// The kind the report must carry.
  kind: ErrorKind,
  /// Why this case is here.
  why: &'static str,
}

const FAILURES: &[Failure] = &[
  Failure {
    text: "%",
    span_len: 1,
    kind: ErrorKind::UnexpectedCharacter,
    why: "an ASCII byte that begins no token in any dialect",
  },
  Failure {
    text: "$",
    span_len: 1,
    kind: ErrorKind::UnexpectedCharacter,
    why: "a second such byte, so the case is the class and not the character",
  },
  Failure {
    text: "é",
    span_len: 2,
    kind: ErrorKind::UnexpectedCharacter,
    why: "two bytes of one character, spanned whole",
  },
  Failure {
    text: "日",
    span_len: 3,
    kind: ErrorKind::UnexpectedCharacter,
    why: "three bytes of one character",
  },
  Failure {
    text: "X",
    span_len: 1,
    kind: ErrorKind::UnexpectedCharacter,
    why: "a letter that begins no name",
  },
  Failure {
    text: "SX",
    span_len: 1,
    kind: ErrorKind::UnexpectedCharacter,
    why: "two letters that could not have become a name: one byte each",
  },
  Failure {
    text: "MAX",
    span_len: 2,
    kind: ErrorKind::UnexpectedCharacter,
    why: "`MA` could still have become `MAR`, so the two are reported together",
  },
  Failure {
    text: "@",
    span_len: 1,
    kind: ErrorKind::UnexpectedCharacter,
    why: "a lone `@` is not a nickname; it is an ordinary bad byte in an ordinary field",
  },
  Failure {
    text: "4294967296",
    span_len: 10,
    kind: ErrorKind::NumberTooLarge,
    why: "one past `u32`, reported over the whole run",
  },
  Failure {
    text: "99999999999999999999",
    span_len: 20,
    kind: ErrorKind::NumberTooLarge,
    why: "a run long enough to overflow and wrap back into range",
  },
];

/// The token a field carries beside the bad one.
///
/// `*` is the only item legal in every field of every dialect, including the trailing
/// year, and it neither swallows what follows it nor is swallowed by it.
const CARRIER: &str = "*";

/// Where inside its field the bad token sits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Place {
  /// The bad token is the whole field.
  Alone,
  /// The bad token is the field's first byte, and what follows is never reached.
  First,
  /// A good item is read first, and the bad token is what the field ends on.
  Last,
}

/// Every [`Place`], in the order the generator visits them.
const PLACES: [Place; 3] = [Place::Alone, Place::First, Place::Last];

impl Place {
  /// The field's text, with the bad token written into it.
  fn field(self, bad: &str) -> String {
    let mut text = String::new();
    if matches!(self, Self::Last) {
      text.push_str(CARRIER);
    }
    text.push_str(bad);
    if matches!(self, Self::First) {
      text.push_str(CARRIER);
    }
    text
  }

  /// How many bytes into the field the report has to start.
  const fn offset(self) -> usize {
    match self {
      Self::Alone | Self::First => 0,
      Self::Last => CARRIER.len(),
    }
  }

  /// Why this placement is here.
  const fn why(self) -> &'static str {
    match self {
      Self::Alone => "the bad token is the whole field",
      Self::First => "the bad token is the field's first byte",
      Self::Last => "the field ends on the bad token, after reading a good item",
    }
  }
}

/// Whitespace around and between the fields.
///
/// The separator is a field boundary and the lead shifts every span in the expression,
/// so both are arithmetic the report has to get right and neither is exercised by an
/// expression written with one space between each pair of fields.
struct Layout {
  lead: &'static str,
  sep: &'static str,
  trail: &'static str,
  why: &'static str,
}

const LAYOUTS: &[Layout] = &[
  Layout {
    lead: "",
    sep: " ",
    trail: "",
    why: "one space between fields and nothing around them",
  },
  Layout {
    lead: "  ",
    sep: " ",
    trail: "",
    why: "leading whitespace, which every span has to be shifted past",
  },
  Layout {
    lead: "",
    sep: " ",
    trail: "  ",
    why: "trailing whitespace, which the last field must not read as part of itself",
  },
  Layout {
    lead: "\t",
    sep: " \t",
    trail: "\r\n",
    why: "the whitespace class in full, and separators wider than one byte",
  },
];

// ---------------------------------------------------------------------------
// Writing a case, and where the bad token landed.
// ---------------------------------------------------------------------------

/// Writes one case and returns it with the byte offset the report must start at.
///
/// The offset is a sum over the template's own pieces and nothing else: the layout's
/// lead, the text of every field before this one, one separator per boundary crossed,
/// and the bad token's offset within its own field. **No parser is consulted.** That is
/// the whole reason this file exists, so it is stated here rather than left to be
/// noticed.
fn write_case(
  shape: &Shape,
  layout: &Layout,
  index: usize,
  place: Place,
  bad: &str,
) -> (String, usize) {
  let mut written = String::from(layout.lead);
  let mut start = None;

  for (position, field) in shape.fields.iter().enumerate() {
    if position > 0 {
      written.push_str(layout.sep);
    }
    if position == index {
      start = Some(written.len() + place.offset());
      written.push_str(&place.field(bad));
    } else {
      written.push_str(field);
    }
  }

  written.push_str(layout.trail);
  (written, start.expect("the index is a field of this shape"))
}

/// The shape written out untouched, which every instantiation it declares must accept.
///
/// Rewriting field zero as itself is the same string as the base, and reusing the
/// generator to produce it is deliberate: a base built by some other route could differ
/// from what the cases are mutations of, and then a case would be measuring two changes.
fn write_base(shape: &Shape, layout: &Layout) -> String {
  write_case(shape, layout, 0, Place::Alone, shape.fields[0]).0
}

// ---------------------------------------------------------------------------
// The matrix.
// ---------------------------------------------------------------------------

/// How many cases the tables generate.
///
/// Pinned as a literal as well as recomputed from the tables, because the failure this
/// guards against is a row going *missing*: a suite that quietly shrinks still passes
/// every assertion it has left.
const GENERATED_CASES: usize = 4_440;

/// Nine thousand short parses is the right size for a compiled test job and the wrong
/// size for an interpreter, so Miri skips this sweep for the same reason it skips the
/// differential: what it looks for is which kind, which byte and which field an error
/// carries, and none of those is a property of the machine the logic runs on. The crate
/// is `#![forbid(unsafe_code)]` besides, and every named case in this file still runs
/// under Miri. It costs the Miri job 149 seconds and every other job a hundredth of a
/// second, which is where it runs.
#[test]
#[cfg_attr(
  miri,
  ignore = "4,440 parses is far too slow under an interpreter; runs on every other job"
)]
fn every_field_of_every_dialect_reports_its_own_lexical_failure() {
  let mut cases = 0usize;

  for shape in SHAPES {
    assert_eq!(
      shape.fields.len(),
      shape.kinds.len(),
      "{}: {} fields but {} names for them",
      shape.what,
      shape.fields.len(),
      shape.kinds.len()
    );

    for &under in shape.under {
      for layout in LAYOUTS {
        // The base has to parse. A base that did not would let a case pass against a
        // failure the template never planted, somewhere else in the expression.
        let base = write_base(shape, layout);
        assert!(
          under.parse(&base).is_ok(),
          "{} rejected its own base {base:?} ({}, {})",
          under.name(),
          shape.what,
          layout.why
        );

        for (index, &field) in shape.kinds.iter().enumerate() {
          for failure in FAILURES {
            for place in PLACES {
              let (written, start) = write_case(shape, layout, index, place, failure.text);
              let error = match under.parse(&written) {
                Err(error) => error,
                Ok(()) => panic!(
                  "{} accepted {written:?}, which carries {} at field {index} ({})",
                  under.name(),
                  failure.why,
                  place.why()
                ),
              };

              assert_eq!(
                (
                  *error.kind(),
                  error.span().start(),
                  error.span().end(),
                  error.field()
                ),
                (failure.kind, start, start + failure.span_len, Some(field)),
                "{} on {written:?}\n  shape:   {}\n  layout:  {}\n  field:   {index} \
                 ({field})\n  failure: {}\n  place:   {}",
                under.name(),
                shape.what,
                layout.why,
                failure.why,
                place.why()
              );

              assert!(
                written
                  .get(error.span().start()..error.span().end())
                  .is_some(),
                "{} reported a span that is not a slice of {written:?}",
                under.name()
              );

              cases += 1;
            }
          }
        }
      }
    }
  }

  let from_the_tables: usize = SHAPES
    .iter()
    .map(|shape| shape.under.len() * shape.kinds.len())
    .sum::<usize>()
    * LAYOUTS.len()
    * FAILURES.len()
    * PLACES.len();

  assert_eq!(
    cases, from_the_tables,
    "the generator visited {cases} cases but the tables describe {from_the_tables}"
  );
  assert_eq!(
    cases, GENERATED_CASES,
    "the matrix changed size; if a row was removed, say why, and if one was added, \
     update GENERATED_CASES"
  );
}

/// How many cases the same tables generate on the *other* side of the field count.
///
/// Larger than [`GENERATED_CASES`] because a shape is out of range for more dialects
/// than it is in range for.
const PREFLIGHT_CASES: usize = 4_920;

/// Every instantiation, whatever shape it is being offered.
const ALL_UNDER: [Under; 4] = [Under::Vixie, Under::Quartz1, Under::Quartz2, Under::Robfig];

/// The same cross product, parsed under the dialects the shape is *wrong* for.
///
/// Every case here carries a bad token in a field, exactly as the matrix above does, and
/// every one of them answers with the field count instead. That is the rule, and it is
/// deliberate: the count is checked before any field is read, for every expression, so
/// what a caller is told first is the difference they have to fix first.
///
/// It is generated rather than sampled because the interesting part is that there is *no
/// exception*. A rule with one is a rule a reader has to keep in their head; this pins
/// the whole cross product at once, so the precedence cannot quietly acquire one.
#[test]
#[cfg_attr(
  miri,
  ignore = "4,920 parses is far too slow under an interpreter; runs on every other job"
)]
pub(super) fn the_field_count_preflight_runs_before_any_field_is_read() {
  let mut cases = 0usize;

  for shape in SHAPES {
    let found = shape.fields.len();
    for under in ALL_UNDER {
      let (min, max) = under.bounds();
      // In range is the matrix above: the field gets read and reports its own failure.
      if found >= min && found <= max {
        continue;
      }

      for layout in LAYOUTS {
        for index in 0..shape.kinds.len() {
          for failure in FAILURES {
            for place in PLACES {
              let (written, _) = write_case(shape, layout, index, place, failure.text);
              let error = match under.parse(&written) {
                Err(error) => error,
                Ok(()) => panic!("{} accepted {written:?}", under.name()),
              };

              assert_eq!(
                (
                  *error.kind(),
                  error.span().start(),
                  error.span().end(),
                  error.field()
                ),
                (
                  ErrorKind::WrongFieldCount {
                    found,
                    min,
                    max,
                    dialect: under.dialect(),
                  },
                  0,
                  written.len(),
                  None
                ),
                "{} on {written:?}: {found} runs is not {min}..={max} whatever those \
                 runs contain, and the count is checked before any of them is read \
                 ({}, field {index}, {}, {})",
                under.name(),
                shape.what,
                failure.why,
                place.why()
              );

              cases += 1;
            }
          }
        }
      }
    }
  }

  assert_eq!(
    cases, PREFLIGHT_CASES,
    "the out-of-shape half of the matrix changed size"
  );
}

/// A bad byte does not outrank a wrong field count, and the alternative is worse.
///
/// `"0 0 * * *%"` is five runs with a bad byte in the last of them. Under Vixie five runs
/// is the right number, so the byte is reported where it is — in the **day of week**.
/// Under Quartz and the Go dialect five is the wrong number, and what comes back is the
/// count, spanning the whole expression with no field attributed.
///
/// Reporting the byte instead would mean naming the field it is in, and under those two
/// dialects there is no such field to name. Run four of a six- or seven-field expression
/// is the *month*, so a Quartz reading of this string would attribute the byte to the
/// month — which is not the field it is in by any reading a caller would recognise,
/// since the string is verbatim Vixie and the run they wrote there is the day of week.
/// A wrong-count error is true and is the difference that has to be fixed first; a field
/// name guessed from a layout the expression does not have is neither.
///
/// Three more cases say why the rule cannot be narrowed to lexical failures only:
///
///   - `"0 0 * * 99"` puts a *non*-lexical failure in the same run under the same
///     dialects. If "the parser could have looked at the runs" outranked the count, this
///     would have to become `ValueOutOfRange` in the month, and it plainly should not.
///   - `"0 0 * * * * * *%"` has eight runs. There is no seventh field position in any
///     dialect, so a pass over the runs has nowhere to attribute the byte at all.
///   - `"%"` and `"*"` are one run, and run zero is exactly as attributable as run four.
///     A rule that reported the byte in the five-run case would have to report it here
///     too, which is the decision [`a_lone_bad_byte_is_a_field_count_error_in_every_dialect`]
///     records as settled.
#[test]
pub(super) fn a_bad_byte_does_not_outrank_a_wrong_field_count() {
  /// A five-run expression, and the lexical failure Vixie reports in its last run.
  const FIVE_RUNS: &[(&str, ErrorKind, usize, usize)] = &[
    ("0 0 * * *%", ErrorKind::UnexpectedCharacter, 9, 10),
    ("0 0 * * *4294967296", ErrorKind::NumberTooLarge, 9, 19),
  ];

  for &(expression, kind, start, end) in FIVE_RUNS {
    let error = Schedule::<Vixie>::parse(expression).expect_err(expression);
    assert_eq!(
      (
        *error.kind(),
        error.span().start(),
        error.span().end(),
        error.field()
      ),
      (kind, start, end, Some(FieldKind::DayOfWeek)),
      "{expression:?}: five runs is Vixie's shape, so the byte is reported where it is"
    );

    for under in [Under::Quartz1, Under::Quartz2, Under::Robfig] {
      let (min, max) = under.bounds();
      let error = under.parse(expression).expect_err(expression);
      assert_eq!(
        (
          *error.kind(),
          error.span().start(),
          error.span().end(),
          error.field()
        ),
        (
          ErrorKind::WrongFieldCount {
            found: 5,
            min,
            max,
            dialect: under.dialect(),
          },
          0,
          expression.len(),
          None
        ),
        "{} on {expression:?}: run four of this dialect's layout is the month, and the \
         byte is not in the month — the shape is what is wrong, and the shape is what \
         is reported",
        under.name()
      );
    }
  }

  // A non-lexical failure in the same run, under the same dialects: the count still
  // wins, so the precedence is about the count and not about lexing.
  for under in [Under::Quartz1, Under::Robfig] {
    let error = under.parse("0 0 * * 99").expect_err("five runs");
    assert!(
      matches!(error.kind(), ErrorKind::WrongFieldCount { .. }),
      "{} on \"0 0 * * 99\" answered {:?}",
      under.name(),
      error.kind()
    );
  }

  // Too many runs: there is no field position for the offending one in any dialect, so
  // a pass over the runs could not attribute it even if the precedence were reversed.
  for under in ALL_UNDER {
    let error = under.parse("0 0 * * * * * *%").expect_err("eight runs");
    assert!(
      matches!(error.kind(), ErrorKind::WrongFieldCount { found: 8, .. }),
      "{} on eight runs answered {:?}",
      under.name(),
      error.kind()
    );
    assert_eq!(error.field(), None, "{}", under.name());
  }
}

/// Every `FieldKind`, so that a field with no contract case cannot exist quietly.
const ALL_FIELD_KINDS: [FieldKind; 7] = [
  FieldKind::Second,
  FieldKind::Minute,
  FieldKind::Hour,
  FieldKind::DayOfMonth,
  FieldKind::Month,
  FieldKind::DayOfWeek,
  FieldKind::Year,
];

/// A field kind's place in [`ALL_FIELD_KINDS`].
///
/// Exhaustive on purpose. A new `FieldKind` stops compiling here, which is the point: a
/// field nobody ever plants a bad byte in is a field whose attribution nothing checks,
/// and that is exactly the surface the differential is blind to.
const fn ordinal(kind: FieldKind) -> usize {
  match kind {
    FieldKind::Second => 0,
    FieldKind::Minute => 1,
    FieldKind::Hour => 2,
    FieldKind::DayOfMonth => 3,
    FieldKind::Month => 4,
    FieldKind::DayOfWeek => 5,
    FieldKind::Year => 6,
  }
}

#[test]
fn the_matrix_reaches_every_axis_it_claims_to() {
  for (index, kind) in ALL_FIELD_KINDS.into_iter().enumerate() {
    assert_eq!(
      ordinal(kind),
      index,
      "ALL_FIELD_KINDS is out of order at {index}"
    );
    assert!(
      SHAPES.iter().any(|shape| shape.kinds.contains(&kind)),
      "no shape ever puts a bad token in the {kind} field, so nothing checks what a \
       failure there is attributed to"
    );
  }

  for under in ALL_UNDER {
    assert!(
      SHAPES.iter().any(|shape| shape.under.contains(&under)),
      "{} never parses a generated case",
      under.name()
    );
  }

  for kind in [ErrorKind::UnexpectedCharacter, ErrorKind::NumberTooLarge] {
    assert!(
      FAILURES.iter().any(|failure| failure.kind == kind),
      "{kind:?} is a lexical failure with no generated case"
    );
  }

  // A non-ASCII failure and a multi-byte separator are the two places the span
  // arithmetic can be right about bytes and wrong about characters.
  assert!(
    FAILURES.iter().any(|failure| failure.span_len > 1
      && !failure.text.is_ascii()
      && failure.text.len() == failure.span_len),
    "no case reports a whole non-ASCII character"
  );
  assert!(
    FAILURES
      .iter()
      .any(|failure| failure.span_len < failure.text.len()),
    "no case reports less text than it wrote, so nothing pins where a partial name run \
     stops"
  );
  assert!(
    LAYOUTS.iter().any(|layout| layout.sep.len() > 1),
    "every layout separates its fields with a single byte"
  );
  assert!(
    LAYOUTS.iter().any(|layout| !layout.lead.is_empty()),
    "no layout puts whitespace in front of the expression"
  );
  assert!(
    LAYOUTS.iter().any(|layout| !layout.trail.is_empty()),
    "no layout puts whitespace after the expression"
  );
}

/// The two expressions a simultaneous regression was shown to sail straight through.
///
/// Named separately from the matrix, in the exact form they were reported in, because a
/// case that only exists as one cell of a cross product is a case a future edit to the
/// tables can delete without ever meaning to.
#[test]
fn the_two_cases_the_blind_oracle_would_have_missed() {
  let error = Schedule::<Robfig>::parse("% 0 0 * * *").expect_err("a bad byte");
  assert_eq!(
    (
      *error.kind(),
      error.span().start(),
      error.span().end(),
      error.field()
    ),
    (
      ErrorKind::UnexpectedCharacter,
      0,
      1,
      Some(FieldKind::Second)
    ),
    "the first byte of a six-field expression is the seconds field, not an empty \
     expression and not the minute"
  );

  let error = Schedule::<Quartz>::parse("0 0 0 ? * * %").expect_err("a bad byte");
  assert_eq!(
    (
      *error.kind(),
      error.span().start(),
      error.span().end(),
      error.field()
    ),
    (
      ErrorKind::UnexpectedCharacter,
      12,
      13,
      Some(FieldKind::Year)
    ),
    "the optional year is read on a branch of its own, and its failures used to escape \
     as TrailingInput with no field at all"
  );
}

/// A lone bad byte is a field-count error, in every dialect. Settled.
///
/// One field is not five, or six, or six-or-seven, whatever that field contains, and
/// `count_fields` runs before any field is read — so this is mechanically the same
/// answer in all three dialects rather than three decisions that happen to agree. `*`
/// alone is here beside `%` because it is the control: if lexical validity decided which
/// message a caller got, these two would differ, and they must not.
///
/// This is a decision, not an accident, and it was validated as one. A generated matrix
/// that disagrees with it is wrong; the policy is not.
#[test]
fn a_lone_bad_byte_is_a_field_count_error_in_every_dialect() {
  let expected: &[(Under, ErrorKind)] = &[
    (
      Under::Vixie,
      ErrorKind::WrongFieldCount {
        found: 1,
        min: 5,
        max: 5,
        dialect: "Vixie",
      },
    ),
    (
      Under::Quartz1,
      ErrorKind::WrongFieldCount {
        found: 1,
        min: 6,
        max: 7,
        dialect: "Quartz",
      },
    ),
    (
      Under::Robfig,
      ErrorKind::WrongFieldCount {
        found: 1,
        min: 6,
        max: 6,
        dialect: "Robfig",
      },
    ),
  ];

  for &(under, kind) in expected {
    for expression in ["%", "*"] {
      let error = under.parse(expression).expect_err(expression);
      assert_eq!(
        (
          *error.kind(),
          error.span().start(),
          error.span().end(),
          error.field()
        ),
        (kind, 0, expression.len(), None),
        "{} on {expression:?}: one field is not the right number of fields whatever \
         that field contains",
        under.name()
      );
    }
  }
}

// ---------------------------------------------------------------------------
// The seam: the reference parser cannot move without this file moving with it.
// ---------------------------------------------------------------------------

/// The reference parser's sources, and a digest of each.
///
/// Only the parser, not its tests: what a simultaneous edit hides is a *behaviour*
/// change, and `reference/tests.rs` holds no behaviour of its own.
const REFERENCE_SOURCES: &[(&str, &str, u64)] = &[
  (
    "src/schedule/reference/mod.rs",
    include_str!("../reference/mod.rs"),
    0xbd08_3891_6362_4e72,
  ),
  (
    "src/schedule/reference/field.rs",
    include_str!("../reference/field.rs"),
    0x0c3d_6c36_09bb_bf5c,
  ),
  (
    "src/schedule/reference/token/mod.rs",
    include_str!("../reference/token/mod.rs"),
    0x6849_a8c8_3d7d_4d61,
  ),
];

/// FNV-1a over the bytes, ignoring carriage returns.
///
/// Not cryptographic and does not need to be: this is a tripwire on a file in this
/// repository, not a defence against a forgery. Carriage returns are skipped because the
/// Windows test job checks the repository out with them and a digest that noticed would
/// be a platform failure rather than a contract one.
fn digest(text: &str) -> u64 {
  let mut hash = 0xcbf2_9ce4_8422_2325_u64;
  for &byte in text.as_bytes() {
    if byte == b'\r' {
      continue;
    }
    hash ^= u64::from(byte);
    hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
  }
  hash
}

#[test]
fn the_reference_parser_cannot_change_without_this_contract_changing_with_it() {
  for &(path, source, pinned) in REFERENCE_SOURCES {
    let found = digest(source);
    assert_eq!(
      found, pinned,
      "\n{path} has changed.\n\n\
       That file is the differential oracle. A change to it and a change to the shipped \
       parser at the same time is the one fault `schedule/reference/tests.rs` cannot \
       see: 5,566 expressions, every prefix and four instantiations stayed green \
       through exactly that pair, and the only thing that caught it was a contract case \
       that names the kind, the span and the field outright.\n\n\
       So what is owed is not a new digest. It is: say in the commit message what \
       behaviour changed and why both sides move together; add or adjust the \
       non-differential cases that pin the new behaviour, here and in \
       `schedule/tests.rs`; and only then write {found:#018x} into REFERENCE_SOURCES, \
       in the same commit. A digest bumped on its own re-arms nothing.\n"
    );
  }
}

#[test]
fn the_seam_would_notice_an_edit() {
  // The tripwire's own tripwire. A digest that could not tell two versions of a file
  // apart would pass for every edit ever made to the reference, so the property it
  // rests on is checked rather than assumed: one byte of difference, one different
  // answer.
  for &(path, source, _) in REFERENCE_SOURCES {
    let mut edited = String::from(source);
    edited.push_str("// a comment nobody wrote\n");
    assert_ne!(
      digest(source),
      digest(&edited),
      "{path}: the digest cannot tell an edit from the original"
    );
  }

  // And the normalisation does what it says, so a Windows checkout is not a failure.
  assert_eq!(digest("a\r\nb"), digest("a\nb"));
}
