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
  modifier::{DayOfMonthModifier, DayOfWeekModifier},
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
fn a_star_writes_no_bits_because_it_constrains_nothing() {
  for (spec, lo, hi) in numeric_fields() {
    // `*` means "no constraint", so there is nothing to write down. An empty set
    // paired with an unrestricted field is how that is stored, and it is what leaves
    // a storage ceiling nothing to truncate.
    let (got, restricted) = parse::<Vixie>(spec, "*").unwrap();
    assert_eq!(got, 0, "{:?}: a bare star writes no bits", spec.kind);
    assert!(
      !restricted,
      "{:?}: a bare star restricts nothing",
      spec.kind
    );

    // `*/1` is the same field written differently and must store the same thing.
    assert_eq!(
      parse::<Vixie>(spec, "*/1").unwrap(),
      (0, false),
      "{:?}",
      spec.kind
    );

    // Beside another item the field *is* a restriction, so now the whole range is
    // written out — against the field's own bounds.
    let (got, restricted) = parse::<Vixie>(spec, "*,*").unwrap();
    assert_eq!(got, range(lo, hi), "{:?}", spec.kind);
    assert!(restricted, "{:?}", spec.kind);
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

  // A field that was *written out* is a restriction even when it happens to cover
  // everything, because the stored set is what answers for it.
  assert!(parse::<Vixie>(s, "0-59").unwrap().1);
  assert!(parse::<Vixie>(s, "0,1").unwrap().1);
  assert!(parse::<Vixie>(s, "*/2").unwrap().1);

  // `*/1` is not one of them. It narrows nothing, so it denotes exactly what `*`
  // denotes and is unrestricted for the same reason `*` is. This assertion used to
  // read the other way; that was the defect behind the year wildcard truncating at
  // the storage ceiling, because a "restricted" field is materialised in full.
  assert!(!parse::<Vixie>(s, "*/1").unwrap().1);
  assert_eq!(mask::<Vixie>(s, "*/1"), mask::<Vixie>(s, "*"));
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
fn a_written_out_dow_star_covers_seven_days_in_both_numberings() {
  // A bare `*` writes nothing in every dialect, so it says nothing about numbering.
  for got in [
    mask::<Vixie>(FieldSpec::day_of_week::<Vixie>(), "*"),
    mask::<Quartz>(FieldSpec::day_of_week::<Quartz>(), "*"),
    mask::<Robfig>(FieldSpec::day_of_week::<Robfig>(), "*"),
  ] {
    assert_eq!(got, 0);
  }

  // Written out, it does — and it has to reach the same seven canonical days from
  // Vixie's 0..=7 and Quartz's 1..=7 alike. Vixie's raw range holds eight digits for
  // seven days, so a fold that miscounted would show here.
  assert_eq!(
    mask::<Vixie>(FieldSpec::day_of_week::<Vixie>(), "*,SUN"),
    range(SUNDAY, SATURDAY)
  );
  assert_eq!(
    mask::<Quartz>(FieldSpec::day_of_week::<Quartz>(), "*,SUN"),
    range(SUNDAY, SATURDAY)
  );
  assert_eq!(
    mask::<Robfig>(FieldSpec::day_of_week::<Robfig>(), "*,SUN"),
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

  // `?` admits everything, exactly as `*` does, and stores nothing for the same
  // reason: a field that constrains nothing has nothing to write down.
  let (got, restricted) = parse::<Quartz>(FieldSpec::DAY_OF_MONTH, "?").unwrap();
  assert_eq!(got, 0, "`?` writes no bits");
  assert!(!restricted, "`?` restricts nothing");

  assert_eq!(
    parse::<Robfig>(FieldSpec::DAY_OF_MONTH, "?").unwrap(),
    (0, false)
  );

  // Written beside another item in a dialect that allows that, it is materialised.
  assert_eq!(
    parse::<Robfig>(FieldSpec::DAY_OF_MONTH, "?,1").unwrap(),
    (range(1, 31), true)
  );

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

// ---------------------------------------------------------------------------
// Quartz's date predicates, parsed out of the field they appear in.
// ---------------------------------------------------------------------------

fn outcome<D: Dialect>(spec: FieldSpec, input: &str) -> super::FieldOutcome {
  let mut cursor = Cursor::new(input);
  let mut mask = Mask::default();
  match parse_field::<D, _>(&mut cursor, spec, &mut mask) {
    Ok(outcome) => outcome,
    Err(e) => panic!("{input:?} in {:?} should parse: {e}", spec.kind),
  }
}

fn dom_modifier(input: &str) -> DayOfMonthModifier {
  match outcome::<Quartz>(FieldSpec::DAY_OF_MONTH, input).modifier {
    Some(super::Modifier::DayOfMonth(m)) => m,
    other => panic!("{input:?} produced {other:?}, not a day-of-month predicate"),
  }
}

fn dow_modifier(input: &str) -> DayOfWeekModifier {
  match outcome::<Quartz>(FieldSpec::day_of_week::<Quartz>(), input).modifier {
    Some(super::Modifier::DayOfWeek(m)) => m,
    other => panic!("{input:?} produced {other:?}, not a day-of-week predicate"),
  }
}

#[test]
fn day_of_month_predicates_parse() {
  assert_eq!(dom_modifier("L"), DayOfMonthModifier::Last);
  assert_eq!(dom_modifier("l"), DayOfMonthModifier::Last);
  assert_eq!(dom_modifier("LW"), DayOfMonthModifier::LastWeekday);
  assert_eq!(dom_modifier("lw"), DayOfMonthModifier::LastWeekday);
  assert_eq!(
    dom_modifier("L-3"),
    DayOfMonthModifier::LastOffset { days: 3 }
  );
  assert_eq!(
    dom_modifier("15W"),
    DayOfMonthModifier::NearestWeekday { day: 15 }
  );
  assert_eq!(
    dom_modifier("1W"),
    DayOfMonthModifier::NearestWeekday { day: 1 }
  );
}

#[test]
fn day_of_week_predicates_parse_through_the_dialects_numbering() {
  // Quartz numbers Sunday as 1, so 6 is Friday. The predicate must store the day, not
  // the digit, or `6L` would mean Saturday here and Friday under a Vixie-numbered
  // dialect that later gained predicates.
  assert_eq!(
    dow_modifier("6L"),
    DayOfWeekModifier::Last {
      weekday: crate::date::Weekday::Friday
    }
  );
  assert_eq!(
    dow_modifier("FRI#3"),
    DayOfWeekModifier::Nth {
      weekday: crate::date::Weekday::Friday,
      nth: 3
    }
  );
  assert_eq!(
    dow_modifier("6#3"),
    DayOfWeekModifier::Nth {
      weekday: crate::date::Weekday::Friday,
      nth: 3
    },
    "the digit and the name must reach the same day"
  );
}

#[test]
fn a_lone_l_in_the_day_of_week_field_is_saturday_not_a_predicate() {
  // Quartz defines it as another spelling of 7, so it is an ordinary set member.
  let spec = FieldSpec::day_of_week::<Quartz>();
  let got = outcome::<Quartz>(spec, "L");
  assert_eq!(got.modifier, None);
  let (bits, restricted) = parse::<Quartz>(spec, "L").unwrap();
  assert_eq!(bits, self::bits(&[SATURDAY]));
  assert!(restricted);
}

#[test]
fn a_predicate_in_the_wrong_field_is_rejected() {
  assert_eq!(
    err::<Quartz>(FieldSpec::day_of_week::<Quartz>(), "15W"),
    ErrorKind::ModifierNotValidHere,
    "`W` is a day-of-month predicate"
  );
  assert_eq!(
    err::<Quartz>(FieldSpec::DAY_OF_MONTH, "6#3"),
    ErrorKind::ModifierNotValidHere,
    "`#` is a day-of-week predicate"
  );
  assert_eq!(
    err::<Quartz>(FieldSpec::MINUTE, "L"),
    ErrorKind::ModifierNotValidHere
  );
}

#[test]
fn a_predicate_cannot_be_one_item_of_a_list() {
  assert_eq!(
    err::<Quartz>(FieldSpec::DAY_OF_MONTH, "L,15"),
    ErrorKind::ModifierMustBeAlone
  );
  assert_eq!(
    err::<Quartz>(FieldSpec::DAY_OF_MONTH, "15,L"),
    ErrorKind::ModifierMustBeAlone
  );
  assert_eq!(
    err::<Quartz>(FieldSpec::day_of_week::<Quartz>(), "6#3,2"),
    ErrorKind::ModifierMustBeAlone
  );
}

#[test]
fn predicate_operands_are_range_checked() {
  assert_eq!(
    err::<Quartz>(FieldSpec::DAY_OF_MONTH, "L-31"),
    ErrorKind::ValueOutOfRange {
      value: 31,
      min: 1,
      max: 30
    },
    "there is no month with a day thirty-one before its last"
  );
  assert_eq!(
    err::<Quartz>(FieldSpec::DAY_OF_MONTH, "L-0"),
    ErrorKind::ValueOutOfRange {
      value: 0,
      min: 1,
      max: 30
    }
  );
  assert_eq!(
    err::<Quartz>(FieldSpec::DAY_OF_MONTH, "32W"),
    ErrorKind::ValueOutOfRange {
      value: 32,
      min: 1,
      max: 31
    }
  );
  assert_eq!(
    err::<Quartz>(FieldSpec::day_of_week::<Quartz>(), "6#6"),
    ErrorKind::ValueOutOfRange {
      value: 6,
      min: 1,
      max: 5
    },
    "no weekday occurs six times in a month"
  );
  assert_eq!(
    err::<Quartz>(FieldSpec::day_of_week::<Quartz>(), "6#0"),
    ErrorKind::ValueOutOfRange {
      value: 0,
      min: 1,
      max: 5
    }
  );
}

// ---------------------------------------------------------------------------
// A range that runs backwards: a dialect difference, not a mistake.
// ---------------------------------------------------------------------------

/// Which field a wrapping case applies to, resolved per dialect because day-of-week's
/// bounds depend on the dialect's numbering.
#[derive(Clone, Copy, Debug)]
enum Which {
  Second,
  Minute,
  Hour,
  DayOfMonth,
  Month,
}

fn spec_for(which: Which) -> FieldSpec {
  match which {
    Which::Second => FieldSpec::SECOND,
    Which::Minute => FieldSpec::MINUTE,
    Which::Hour => FieldSpec::HOUR,
    Which::DayOfMonth => FieldSpec::DAY_OF_MONTH,
    Which::Month => FieldSpec::MONTH,
  }
}

/// A backwards range, the values Quartz makes of it, and the endpoints the dialects
/// that refuse it report. One table so that acceptance and rejection cannot drift apart.
const BACKWARDS: &[(Which, &str, &[u32], u32, u32)] = &[
  (Which::Month, "NOV-FEB", &[11, 12, 1, 2], 11, 2),
  (Which::Month, "11-2", &[11, 12, 1, 2], 11, 2),
  (Which::Hour, "22-2", &[22, 23, 0, 1, 2], 22, 2),
  (Which::DayOfMonth, "30-2", &[30, 31, 1, 2], 30, 2),
  (Which::Second, "58-2", &[58, 59, 0, 1, 2], 58, 2),
  (Which::Minute, "58-2", &[58, 59, 0, 1, 2], 58, 2),
];

#[test]
fn quartz_wraps_a_backwards_range_and_the_others_refuse_it() {
  for &(which, text, expected, start, end) in BACKWARDS {
    let spec = spec_for(which);

    // Quartz documents overflowing ranges: the range runs on through the field's
    // ceiling and back round to its floor.
    assert_eq!(
      mask::<Quartz>(spec, text),
      bits(expected),
      "Quartz {which:?} {text}"
    );

    // `cron` 0.17 guards its range expansion with `start <= end` and reports an
    // invalid range otherwise. Vixie and the Go dialect follow it.
    for_each_ascending(spec, text, start, end);
  }
}

fn for_each_ascending(spec: FieldSpec, text: &str, start: u32, end: u32) {
  assert_eq!(
    err::<Vixie>(spec, text),
    ErrorKind::ReversedRange { start, end },
    "Vixie {text}"
  );
  assert_eq!(
    err::<Robfig>(spec, text),
    ErrorKind::ReversedRange { start, end },
    "Robfig {text}"
  );
}

#[test]
fn a_backwards_weekday_range_wraps_through_the_dialects_own_numbering() {
  // FRI-MON is Friday, Saturday, Sunday, Monday. Quartz numbers Friday 6 and Monday 2,
  // so the wrap runs 6, 7, 1, 2 in its numbering and lands on 5, 6, 0, 1 canonically —
  // the conversion has to happen after the wrap, not before it.
  assert_eq!(
    mask::<Quartz>(FieldSpec::day_of_week::<Quartz>(), "FRI-MON"),
    bits(&[FRIDAY, SATURDAY, SUNDAY, MONDAY])
  );
  assert_eq!(
    mask::<Quartz>(FieldSpec::day_of_week::<Quartz>(), "6-2"),
    bits(&[FRIDAY, SATURDAY, SUNDAY, MONDAY]),
    "the digits and the names must reach the same days"
  );

  // The dialects that refuse it quote their own numbering back: Vixie's Friday is 5.
  assert_eq!(
    err::<Vixie>(FieldSpec::day_of_week::<Vixie>(), "FRI-MON"),
    ErrorKind::ReversedRange { start: 5, end: 1 }
  );
  assert_eq!(
    err::<Robfig>(FieldSpec::day_of_week::<Robfig>(), "FRI-MON"),
    ErrorKind::ReversedRange { start: 5, end: 1 }
  );
}

#[test]
fn a_backwards_range_with_a_step_strides_across_the_wrap() {
  // NOV-FEB/2 walks 11 and 13, and 13 comes back round to January.
  assert_eq!(
    mask::<Quartz>(FieldSpec::MONTH, "NOV-FEB/2"),
    bits(&[11, 1])
  );
  assert_eq!(mask::<Quartz>(FieldSpec::HOUR, "22-2/2"), bits(&[22, 0, 2]));
}

#[test]
fn years_do_not_wrap_even_where_the_dialect_wraps_everything_else() {
  // Quartz throws "Start year must be less than stop year" for this, and there is no
  // modulus a year could wrap through in any case.
  let spec = FieldSpec::year::<Quartz>().expect("Quartz has a year field");
  assert_eq!(
    err::<Quartz>(spec, "2030-2020"),
    ErrorKind::ReversedRange {
      start: 2030,
      end: 2020
    }
  );
}

#[test]
fn an_ascending_range_is_unaffected_by_the_wrapping_policy() {
  // The wrap must not disturb the ordinary case in the dialect that has it.
  assert_eq!(mask::<Quartz>(FieldSpec::MONTH, "FEB-NOV"), range(2, 11));
  assert_eq!(mask::<Quartz>(FieldSpec::HOUR, "2-22"), range(2, 22));
  assert_eq!(mask::<Quartz>(FieldSpec::MONTH, "3-3"), bits(&[3]));
  assert_eq!(
    mask::<Quartz>(FieldSpec::day_of_week::<Quartz>(), "MON-FRI"),
    range(MONDAY, FRIDAY)
  );
}
