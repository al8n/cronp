#![allow(
  clippy::indexing_slicing,
  clippy::unwrap_used,
  clippy::expect_used,
  clippy::panic
)]

use std::{vec, vec::Vec};

use super::{parse_field, FieldSpec, Mask};
use crate::{
  dialect::{Dialect, Quartz, Robfig, Vixie},
  error::{ErrorKind, FieldKind, ParseError},
  token::Cursor,
};

/// Parses one field and returns its canonical bitmask plus whether it is restricted.
fn parse<D: Dialect>(spec: FieldSpec, input: &str) -> Result<(u64, bool), ParseError> {
  let mut cursor = Cursor::new(input);
  let mut mask = Mask::default();
  let outcome = parse_field::<D, _>(&mut cursor, spec, &mut mask)?;
  assert!(
    cursor.at_end(),
    "{input:?} left tokens behind: {:?}",
    cursor.peek_token()
  );
  Ok((mask.bits(), outcome.restricted))
}

fn mask<D: Dialect>(spec: FieldSpec, input: &str) -> u64 {
  match parse::<D>(spec, input) {
    Ok((bits, _)) => bits,
    Err(e) => panic!("{input:?} in {:?} should parse: {e}", spec.kind),
  }
}

fn err<D: Dialect>(spec: FieldSpec, input: &str) -> ErrorKind {
  match parse::<D>(spec, input) {
    Ok((bits, _)) => panic!(
      "{input:?} in {:?} should not parse; got {bits:#x}",
      spec.kind
    ),
    Err(e) => *e.kind(),
  }
}

fn bits(values: &[u32]) -> u64 {
  values.iter().fold(0u64, |acc, v| acc | (1u64 << v))
}

fn range(lo: u32, hi: u32) -> u64 {
  (lo..=hi).fold(0u64, |acc, v| acc | (1u64 << v))
}

// ---------------------------------------------------------------------------
// Every field: both boundaries, one past each boundary, and every form of item.
// ---------------------------------------------------------------------------

/// The five fields whose values are plain numbers, with their inclusive bounds.
fn numeric_fields() -> Vec<(FieldSpec, u32, u32)> {
  vec![
    (FieldSpec::SECOND, 0, 59),
    (FieldSpec::MINUTE, 0, 59),
    (FieldSpec::HOUR, 0, 23),
    (FieldSpec::DAY_OF_MONTH, 1, 31),
    (FieldSpec::MONTH, 1, 12),
  ]
}

#[test]
fn both_boundaries_of_every_field_are_accepted() {
  for (spec, lo, hi) in numeric_fields() {
    let mut low = std::string::String::new();
    core::fmt::Write::write_fmt(&mut low, format_args!("{lo}")).unwrap();
    let mut high = std::string::String::new();
    core::fmt::Write::write_fmt(&mut high, format_args!("{hi}")).unwrap();

    assert_eq!(
      mask::<Vixie>(spec, &low),
      bits(&[lo]),
      "{:?} low",
      spec.kind
    );
    assert_eq!(
      mask::<Vixie>(spec, &high),
      bits(&[hi]),
      "{:?} high",
      spec.kind
    );
  }
}

#[test]
fn one_past_either_boundary_names_the_range() {
  for (spec, lo, hi) in numeric_fields() {
    if lo > 0 {
      let mut below = std::string::String::new();
      core::fmt::Write::write_fmt(&mut below, format_args!("{}", lo - 1)).unwrap();
      assert_eq!(
        err::<Vixie>(spec, &below),
        ErrorKind::ValueOutOfRange {
          value: lo - 1,
          min: lo,
          max: hi
        },
        "{:?} below its floor",
        spec.kind
      );
    }
    let mut above = std::string::String::new();
    core::fmt::Write::write_fmt(&mut above, format_args!("{}", hi + 1)).unwrap();
    assert_eq!(
      err::<Vixie>(spec, &above),
      ErrorKind::ValueOutOfRange {
        value: hi + 1,
        min: lo,
        max: hi
      },
      "{:?} above its ceiling",
      spec.kind
    );
  }
}

#[test]
fn a_star_covers_the_whole_range_and_is_unrestricted() {
  for (spec, lo, hi) in numeric_fields() {
    let (got, restricted) = parse::<Vixie>(spec, "*").unwrap();
    assert_eq!(got, range(lo, hi), "{:?}", spec.kind);
    assert!(
      !restricted,
      "{:?}: a bare star restricts nothing",
      spec.kind
    );
  }
}

#[test]
fn lists_ranges_and_steps() {
  let s = FieldSpec::MINUTE;

  assert_eq!(mask::<Vixie>(s, "1,2,3"), bits(&[1, 2, 3]), "a list");
  assert_eq!(mask::<Vixie>(s, "5-10"), range(5, 10), "a range");
  assert_eq!(
    mask::<Vixie>(s, "*/15"),
    bits(&[0, 15, 30, 45]),
    "a step over the whole range"
  );
  assert_eq!(
    mask::<Vixie>(s, "10-30/10"),
    bits(&[10, 20, 30]),
    "a range with a step"
  );
  assert_eq!(
    mask::<Vixie>(s, "0-10/3,40,50-52"),
    bits(&[0, 3, 6, 9, 40, 50, 51, 52]),
    "the three forms in one field"
  );

  // A list, a range and a step are all restrictions even when they cover everything.
  assert!(parse::<Vixie>(s, "0-59").unwrap().1);
  assert!(parse::<Vixie>(s, "*/1").unwrap().1);
}

#[test]
fn a_bare_step_start_is_a_dialect_difference() {
  let s = FieldSpec::MINUTE;

  assert_eq!(
    err::<Vixie>(s, "5/15"),
    ErrorKind::OpenEndedStepNotSupported { dialect: "Vixie" },
    "Vixie requires a range or a star before the slash"
  );
  assert_eq!(
    mask::<Quartz>(s, "5/15"),
    bits(&[5, 20, 35, 50]),
    "Quartz reads `5/15` as `5-59/15`"
  );
  assert_eq!(mask::<Robfig>(s, "5/15"), bits(&[5, 20, 35, 50]));
}

#[test]
fn malformed_items_are_rejected_by_cause() {
  let s = FieldSpec::MINUTE;

  assert_eq!(
    err::<Vixie>(s, "10-5"),
    ErrorKind::ReversedRange { start: 10, end: 5 }
  );
  assert_eq!(err::<Vixie>(s, "*/0"), ErrorKind::ZeroStep);
  assert_eq!(err::<Vixie>(s, "0-10/0"), ErrorKind::ZeroStep);
  assert_eq!(err::<Vixie>(s, ""), ErrorKind::UnexpectedEnd);
  assert_eq!(err::<Vixie>(s, "1,"), ErrorKind::UnexpectedEnd);
  assert_eq!(err::<Vixie>(s, "1-"), ErrorKind::UnexpectedEnd);
  assert_eq!(err::<Vixie>(s, "*/"), ErrorKind::UnexpectedEnd);
  assert_eq!(err::<Vixie>(s, ","), ErrorKind::UnexpectedToken);
  assert_eq!(err::<Vixie>(s, "1--2"), ErrorKind::UnexpectedToken);
  assert_eq!(err::<Vixie>(s, "%"), ErrorKind::UnexpectedCharacter);
}

// ---------------------------------------------------------------------------
// Names.
// ---------------------------------------------------------------------------

#[test]
fn month_names_are_case_insensitive_and_one_based() {
  let s = FieldSpec::MONTH;
  assert_eq!(mask::<Vixie>(s, "JAN"), bits(&[1]));
  assert_eq!(mask::<Vixie>(s, "dec"), bits(&[12]));
  assert_eq!(mask::<Vixie>(s, "Mar"), bits(&[3]));
  assert_eq!(mask::<Vixie>(s, "JAN-MAR"), bits(&[1, 2, 3]));
  assert_eq!(mask::<Vixie>(s, "JAN,JUL"), bits(&[1, 7]));
  assert_eq!(
    err::<Vixie>(s, "MON"),
    ErrorKind::UnknownName,
    "a weekday name is not a month"
  );
}

#[test]
fn a_field_without_names_rejects_them() {
  assert_eq!(
    err::<Vixie>(FieldSpec::MINUTE, "JAN"),
    ErrorKind::UnknownName
  );
  assert_eq!(
    err::<Vixie>(FieldSpec::DAY_OF_MONTH, "MON"),
    ErrorKind::UnknownName
  );
}

// ---------------------------------------------------------------------------
// Day of week: the numbering incompatibility, at the values that discriminate.
// ---------------------------------------------------------------------------

const SUNDAY: u32 = 0;
const MONDAY: u32 = 1;
const FRIDAY: u32 = 5;
const SATURDAY: u32 = 6;

#[test]
fn the_same_dow_digit_lands_on_different_days() {
  let vixie = FieldSpec::day_of_week::<Vixie>();
  let quartz = FieldSpec::day_of_week::<Quartz>();

  assert_eq!(
    mask::<Vixie>(vixie, "7"),
    bits(&[SUNDAY]),
    "7 is Sunday in Vixie"
  );
  assert_eq!(
    mask::<Quartz>(quartz, "7"),
    bits(&[SATURDAY]),
    "7 is Saturday in Quartz — the same digit, six days apart"
  );

  assert_eq!(mask::<Vixie>(vixie, "1"), bits(&[MONDAY]));
  assert_eq!(
    mask::<Quartz>(quartz, "1"),
    bits(&[SUNDAY]),
    "1 is Monday in Vixie and Sunday in Quartz"
  );

  assert_eq!(mask::<Vixie>(vixie, "0"), bits(&[SUNDAY]));
  assert_eq!(
    err::<Quartz>(quartz, "0"),
    ErrorKind::ValueOutOfRange {
      value: 0,
      min: 1,
      max: 7
    },
    "Quartz numbers 1..=7, so 0 is not a day at all"
  );

  assert_eq!(
    err::<Vixie>(vixie, "8"),
    ErrorKind::ValueOutOfRange {
      value: 8,
      min: 0,
      max: 7
    }
  );
}

#[test]
fn dow_names_agree_where_digits_do_not() {
  let vixie = FieldSpec::day_of_week::<Vixie>();
  let quartz = FieldSpec::day_of_week::<Quartz>();
  let robfig = FieldSpec::day_of_week::<Robfig>();

  for (name, expected) in [("SUN", SUNDAY), ("MON", MONDAY), ("SAT", SATURDAY)] {
    assert_eq!(
      mask::<Vixie>(vixie, name),
      bits(&[expected]),
      "Vixie {name}"
    );
    assert_eq!(
      mask::<Quartz>(quartz, name),
      bits(&[expected]),
      "Quartz {name}"
    );
    assert_eq!(
      mask::<Robfig>(robfig, name),
      bits(&[expected]),
      "Robfig {name}"
    );
  }

  assert_eq!(mask::<Vixie>(vixie, "mon-fri"), range(MONDAY, FRIDAY));
  assert_eq!(mask::<Quartz>(quartz, "MON-FRI"), range(MONDAY, FRIDAY));
}

#[test]
fn a_dow_range_that_ends_on_sunday_folds_rather_than_wrapping() {
  // Vixie parses the day-of-week field over 0..=7 and only then folds 7 onto 0, which
  // is why `5-7` is Friday, Saturday and Sunday rather than an error or a wrap.
  let vixie = FieldSpec::day_of_week::<Vixie>();
  assert_eq!(
    mask::<Vixie>(vixie, "5-7"),
    bits(&[FRIDAY, SATURDAY, SUNDAY])
  );
  assert_eq!(mask::<Vixie>(vixie, "0-7"), range(SUNDAY, SATURDAY));

  let quartz = FieldSpec::day_of_week::<Quartz>();
  assert_eq!(
    mask::<Quartz>(quartz, "5-7"),
    bits(&[4, 5, 6]),
    "Quartz shifts by one instead of folding: 5-7 is Thursday to Saturday"
  );
}

#[test]
fn a_dow_star_covers_seven_days_in_both_numberings() {
  assert_eq!(
    mask::<Vixie>(FieldSpec::day_of_week::<Vixie>(), "*"),
    range(SUNDAY, SATURDAY)
  );
  assert_eq!(
    mask::<Quartz>(FieldSpec::day_of_week::<Quartz>(), "*"),
    range(SUNDAY, SATURDAY)
  );
  assert_eq!(
    mask::<Robfig>(FieldSpec::day_of_week::<Robfig>(), "*"),
    range(SUNDAY, SATURDAY)
  );
}

// ---------------------------------------------------------------------------
// `?` and the modifier tokens are gated on the dialect, not on the lexer.
// ---------------------------------------------------------------------------

#[test]
fn question_mark_is_gated_on_the_dialect() {
  assert_eq!(
    err::<Vixie>(FieldSpec::DAY_OF_MONTH, "?"),
    ErrorKind::QuestionMarkNotSupported { dialect: "Vixie" }
  );

  let (got, restricted) = parse::<Quartz>(FieldSpec::DAY_OF_MONTH, "?").unwrap();
  assert_eq!(got, range(1, 31), "`?` admits everything, like `*`");
  assert!(!restricted, "`?` restricts nothing");

  assert!(parse::<Robfig>(FieldSpec::DAY_OF_MONTH, "?").is_ok());

  assert_eq!(
    err::<Quartz>(FieldSpec::MINUTE, "?"),
    ErrorKind::QuestionMarkNotValidHere,
    "`?` is only ever a day-of-month or day-of-week token"
  );
}

#[test]
fn modifier_tokens_are_rejected_where_the_dialect_has_none() {
  for input in ["L", "LW", "15W", "6#3"] {
    assert_eq!(
      err::<Vixie>(FieldSpec::DAY_OF_MONTH, input),
      ErrorKind::ModifierNotSupported { dialect: "Vixie" },
      "{input}"
    );
    assert_eq!(
      err::<Robfig>(FieldSpec::DAY_OF_MONTH, input),
      ErrorKind::ModifierNotSupported { dialect: "Robfig" },
      "{input}"
    );
  }
}

// ---------------------------------------------------------------------------
// Errors carry a span into the original input.
// ---------------------------------------------------------------------------

#[test]
fn errors_point_at_the_offending_token() {
  let mut cursor = Cursor::new("1,2,99");
  let mut sink = Mask::default();
  let error = parse_field::<Vixie, _>(&mut cursor, FieldSpec::MINUTE, &mut sink)
    .expect_err("99 is out of range for a minute");
  assert_eq!(error.span().start(), 4);
  assert_eq!(error.span().end(), 6);
  assert_eq!(error.field(), Some(FieldKind::Minute));
}

#[test]
fn an_error_at_the_end_of_input_points_past_the_last_token() {
  let mut cursor = Cursor::new("1-");
  let mut sink = Mask::default();
  let error = parse_field::<Vixie, _>(&mut cursor, FieldSpec::MINUTE, &mut sink)
    .expect_err("a range needs an end");
  assert_eq!(*error.kind(), ErrorKind::UnexpectedEnd);
  assert_eq!(error.span().start(), 2);
  assert_eq!(error.span().end(), 2);
}
