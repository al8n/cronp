#![allow(
  clippy::indexing_slicing,
  clippy::unwrap_used,
  clippy::expect_used,
  clippy::panic
)]

use core::time::Duration;
use std::{string::String, vec::Vec};

use super::{Calendar, Schedule};
use crate::{
  date::Weekday,
  dialect::{Dialect, Quartz, Robfig, Vixie},
  error::ErrorKind,
};

/// What a dialect must do with an expression.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Expect {
  Accept,
  Reject(ErrorKind),
}

const fn fields(found: usize, min: usize, max: usize, dialect: &'static str) -> Expect {
  Expect::Reject(ErrorKind::WrongFieldCount {
    found,
    min,
    max,
    dialect,
  })
}

/// One expression and what each of the three dialects must make of it.
struct Row {
  expression: &'static str,
  vixie: Expect,
  quartz: Expect,
  robfig: Expect,
  why: &'static str,
}

const TABLE: &[Row] = &[
  // ----- field count -----
  Row {
    expression: "0 0 * * *",
    vixie: Expect::Accept,
    quartz: fields(5, 6, 7, "Quartz"),
    robfig: fields(5, 6, 6, "Robfig"),
    why: "five fields: Vixie's shape and nobody else's",
  },
  Row {
    expression: "0 0 0 * * *",
    vixie: fields(6, 5, 5, "Vixie"),
    quartz: Expect::Reject(ErrorKind::QuestionMarkRequired { dialect: "Quartz" }),
    robfig: Expect::Accept,
    why: "six fields: the Go dialect takes it; Quartz takes the width but not two \
          unrestricted day fields",
  },
  Row {
    expression: "0 0 0 ? * *",
    vixie: fields(6, 5, 5, "Vixie"),
    quartz: Expect::Accept,
    robfig: Expect::Accept,
    why: "the same six fields with `?` in day-of-month: now Quartz takes it too",
  },
  Row {
    expression: "0 0 0 ? * * 2025",
    vixie: fields(7, 5, 5, "Vixie"),
    quartz: Expect::Accept,
    robfig: fields(7, 6, 6, "Robfig"),
    why: "seven fields: only Quartz has a year field, and it is the plan's \
          seven-fields-under-a-five-field-dialect case",
  },
  Row {
    expression: "0 0 0 ? * * *",
    vixie: fields(7, 5, 5, "Vixie"),
    quartz: Expect::Accept,
    robfig: fields(7, 6, 6, "Robfig"),
    why: "a star year field, which must not overflow Years<1>",
  },
  Row {
    expression: "0 0 * * * * * *",
    vixie: fields(8, 5, 5, "Vixie"),
    quartz: fields(8, 6, 7, "Quartz"),
    robfig: fields(8, 6, 6, "Robfig"),
    why: "eight fields: too many for every dialect",
  },
  Row {
    expression: "0 0 * *",
    vixie: fields(4, 5, 5, "Vixie"),
    quartz: fields(4, 6, 7, "Quartz"),
    robfig: fields(4, 6, 6, "Robfig"),
    why: "four fields: too few for every dialect",
  },
  // ----- `?` -----
  Row {
    expression: "0 0 ? * *",
    vixie: Expect::Reject(ErrorKind::QuestionMarkNotSupported { dialect: "Vixie" }),
    quartz: fields(5, 6, 7, "Quartz"),
    robfig: fields(5, 6, 6, "Robfig"),
    why: "the plan's `?`-under-Vixie case: Vixie has no such token at all",
  },
  Row {
    expression: "0 0 0 ? * ?",
    vixie: fields(6, 5, 5, "Vixie"),
    quartz: Expect::Reject(ErrorKind::QuestionMarkInBothDayFields { dialect: "Quartz" }),
    robfig: Expect::Accept,
    why: "`?` in both day fields leaves Quartz nothing to fire on; the Go dialect \
          reads both as stars",
  },
  // ----- day-of-month against day-of-week -----
  Row {
    expression: "0 0 1 * MON",
    vixie: Expect::Accept,
    quartz: fields(5, 6, 7, "Quartz"),
    robfig: fields(5, 6, 6, "Robfig"),
    why: "the plan's dom-and-dow-both-restricted case. Vixie takes the union; the \
          other two never see it because the width is wrong",
  },
  Row {
    expression: "0 0 0 1 * MON",
    vixie: fields(6, 5, 5, "Vixie"),
    quartz: Expect::Reject(ErrorKind::QuestionMarkRequired { dialect: "Quartz" }),
    robfig: Expect::Accept,
    why: "both day fields restricted at six fields: Quartz refuses the question, the \
          Go dialect answers it with the union",
  },
  // ----- Quartz's date predicates -----
  Row {
    expression: "0 0 12 L * ?",
    vixie: fields(6, 5, 5, "Vixie"),
    quartz: Expect::Accept,
    robfig: Expect::Reject(ErrorKind::ModifierNotSupported { dialect: "Robfig" }),
    why: "`L` at a width the Go dialect accepts, so its rejection is about the \
          predicate rather than the shape",
  },
  Row {
    expression: "0 0 12 ? * 6#3",
    vixie: fields(6, 5, 5, "Vixie"),
    quartz: Expect::Accept,
    robfig: Expect::Reject(ErrorKind::ModifierNotSupported { dialect: "Robfig" }),
    why: "`#` likewise",
  },
  Row {
    expression: "0 0 12 LW * ?",
    vixie: fields(6, 5, 5, "Vixie"),
    quartz: Expect::Accept,
    robfig: Expect::Reject(ErrorKind::ModifierNotSupported { dialect: "Robfig" }),
    why: "`LW`",
  },
  // ----- backwards ranges -----
  Row {
    expression: "0 0 0 ? NOV-FEB MON",
    vixie: fields(6, 5, 5, "Vixie"),
    quartz: Expect::Accept,
    robfig: Expect::Reject(ErrorKind::ReversedRange { start: 11, end: 2 }),
    why: "Quartz documents an overflowing range; cron 0.17 and the Go dialect guard \
          their expansion with start <= end. Both at a width the Go dialect accepts, \
          so its rejection is about the range and not the shape.",
  },
  // ----- steps -----
  Row {
    expression: "5/15 * * * *",
    vixie: Expect::Reject(ErrorKind::OpenEndedStepNotSupported { dialect: "Vixie" }),
    quartz: fields(5, 6, 7, "Quartz"),
    robfig: fields(5, 6, 6, "Robfig"),
    why: "a bare step start, which Vixie alone refuses",
  },
  Row {
    expression: "0 5/15 * * * *",
    vixie: fields(6, 5, 5, "Vixie"),
    quartz: Expect::Reject(ErrorKind::QuestionMarkRequired { dialect: "Quartz" }),
    robfig: Expect::Accept,
    why: "the same bare step at six fields: the Go dialect takes it",
  },
  // ----- nicknames -----
  Row {
    expression: "@daily",
    vixie: Expect::Accept,
    quartz: Expect::Reject(ErrorKind::MacroNotSupported { dialect: "Quartz" }),
    robfig: Expect::Accept,
    why: "the nickname macros, which Quartz does not have",
  },
  Row {
    expression: "@hourly",
    vixie: Expect::Accept,
    quartz: Expect::Reject(ErrorKind::MacroNotSupported { dialect: "Quartz" }),
    robfig: Expect::Accept,
    why: "the shortest nickname",
  },
  Row {
    expression: "@ANNUALLY",
    vixie: Expect::Accept,
    quartz: Expect::Reject(ErrorKind::MacroNotSupported { dialect: "Quartz" }),
    robfig: Expect::Accept,
    why: "nicknames are case-insensitive, and `annually` is `yearly`",
  },
  Row {
    expression: "@reboot",
    vixie: Expect::Accept,
    quartz: Expect::Reject(ErrorKind::RebootNotSupported { dialect: "Quartz" }),
    robfig: Expect::Reject(ErrorKind::RebootNotSupported { dialect: "Robfig" }),
    why: "legal Vixie, so it is parsed and represented rather than rejected",
  },
  Row {
    expression: "@every 1h30m",
    vixie: Expect::Reject(ErrorKind::EveryNotSupported { dialect: "Vixie" }),
    quartz: Expect::Reject(ErrorKind::EveryNotSupported { dialect: "Quartz" }),
    robfig: Expect::Accept,
    why: "a length of time rather than a set of instants: the Go dialect's alone",
  },
  Row {
    expression: "@every",
    vixie: Expect::Reject(ErrorKind::EveryNotSupported { dialect: "Vixie" }),
    quartz: Expect::Reject(ErrorKind::EveryNotSupported { dialect: "Quartz" }),
    robfig: Expect::Reject(ErrorKind::EmptyDuration),
    why: "`@every` with nothing after it",
  },
  Row {
    expression: "@wibble",
    vixie: Expect::Reject(ErrorKind::UnknownMacro),
    quartz: Expect::Reject(ErrorKind::UnknownMacro),
    robfig: Expect::Reject(ErrorKind::UnknownMacro),
    why: "an unknown nickname is unknown everywhere, and says so rather than \
          blaming the dialect",
  },
  // ----- degenerate input -----
  Row {
    expression: "",
    vixie: Expect::Reject(ErrorKind::EmptyExpression),
    quartz: Expect::Reject(ErrorKind::EmptyExpression),
    robfig: Expect::Reject(ErrorKind::EmptyExpression),
    why: "nothing at all",
  },
  Row {
    expression: "     ",
    vixie: Expect::Reject(ErrorKind::EmptyExpression),
    quartz: Expect::Reject(ErrorKind::EmptyExpression),
    robfig: Expect::Reject(ErrorKind::EmptyExpression),
    why: "whitespace only, which must not be read as five empty fields",
  },
  Row {
    expression: "0 0 * * %",
    vixie: Expect::Reject(ErrorKind::UnexpectedCharacter),
    quartz: fields(5, 6, 7, "Quartz"),
    robfig: fields(5, 6, 6, "Robfig"),
    why: "a character no dialect lexes",
  },
];

fn check<D: Dialect>(row: &Row, expect: Expect) {
  let got = Schedule::<D, 1>::parse(row.expression);
  match (expect, got) {
    (Expect::Accept, Ok(_)) => {}
    (Expect::Accept, Err(e)) => panic!(
      "{} rejected {:?} with `{e}` but must accept it ({})",
      D::NAME,
      row.expression,
      row.why
    ),
    (Expect::Reject(kind), Err(e)) => assert_eq!(
      *e.kind(),
      kind,
      "{} rejected {:?} for the wrong reason ({})",
      D::NAME,
      row.expression,
      row.why
    ),
    (Expect::Reject(_), Ok(_)) => panic!(
      "{} accepted {:?} but must not ({})",
      D::NAME,
      row.expression,
      row.why
    ),
  }
}

#[test]
fn the_table_holds_for_every_dialect() {
  for row in TABLE {
    check::<Vixie>(row, row.vixie);
    check::<Quartz>(row, row.quartz);
    check::<Robfig>(row, row.robfig);
  }
}

#[test]
fn every_pair_of_dialects_disagrees_somewhere_in_the_table() {
  // The table's job is the *differences*. Three dialects make three pairs, and a pair
  // with no disagreeing row would mean the table never exercised what separates them.
  /// One column of the table, read out of a row.
  type Column = fn(&Row) -> Expect;

  let pairs: [(&str, Column, Column); 3] = [
    ("Vixie/Quartz", |r| r.vixie, |r| r.quartz),
    ("Vixie/Robfig", |r| r.vixie, |r| r.robfig),
    ("Quartz/Robfig", |r| r.quartz, |r| r.robfig),
  ];
  for (name, left, right) in pairs {
    let disagreements = TABLE
      .iter()
      .filter(|row| {
        matches!(
          (left(row), right(row)),
          (Expect::Accept, Expect::Reject(_)) | (Expect::Reject(_), Expect::Accept)
        )
      })
      .count();
    assert!(
      disagreements > 0,
      "{name} never disagree anywhere in the table"
    );
  }
}

// ---------------------------------------------------------------------------
// What an accepted expression actually holds.
// ---------------------------------------------------------------------------

#[test]
fn the_same_dow_digit_reaches_different_days_through_a_whole_expression() {
  // The incompatibility, end to end rather than at the field parser.
  let vixie = Schedule::<Vixie>::parse("0 0 * * 7").unwrap();
  let calendar = vixie.calendar().expect("a calendar");
  assert!(calendar.admits_weekday(Weekday::Sunday));
  assert!(!calendar.admits_weekday(Weekday::Saturday));

  let quartz = Schedule::<Quartz>::parse("0 0 0 ? * 7").unwrap();
  let calendar = quartz.calendar().expect("a calendar");
  assert!(calendar.admits_weekday(Weekday::Saturday));
  assert!(!calendar.admits_weekday(Weekday::Sunday));
}

#[test]
fn a_five_field_expression_fires_at_second_zero() {
  let schedule = Schedule::<Vixie>::parse("30 2 * * *").unwrap();
  let calendar = schedule.calendar().unwrap();
  assert!(calendar.admits_second(0));
  assert!(!calendar.admits_second(1));
  assert!(calendar.admits_minute(30));
  assert!(!calendar.admits_minute(31));
  assert!(calendar.admits_hour(2));
  assert!(!calendar.admits_hour(3));
  for day in 1..=31 {
    assert!(calendar.admits_day_of_month(day));
  }
  assert!(!calendar.day_of_month_restricted());
  assert!(!calendar.day_of_week_restricted());
}

#[test]
fn the_nicknames_expand_to_what_they_say() {
  let yearly = Schedule::<Vixie>::parse("@yearly").unwrap();
  let calendar = yearly.calendar().unwrap();
  assert!(calendar.admits_minute(0) && !calendar.admits_minute(1));
  assert!(calendar.admits_hour(0) && !calendar.admits_hour(1));
  assert!(calendar.admits_day_of_month(1) && !calendar.admits_day_of_month(2));
  assert!(calendar.admits_month(1) && !calendar.admits_month(2));
  assert!(calendar.day_of_month_restricted());

  assert_eq!(
    Schedule::<Vixie>::parse("@annually").unwrap(),
    Schedule::<Vixie>::parse("@yearly").unwrap()
  );
  assert_eq!(
    Schedule::<Vixie>::parse("@midnight").unwrap(),
    Schedule::<Vixie>::parse("@daily").unwrap()
  );

  let monthly = Schedule::<Vixie>::parse("@monthly").unwrap();
  let calendar = monthly.calendar().unwrap();
  assert!(calendar.admits_day_of_month(1) && !calendar.admits_day_of_month(2));
  for month in 1..=12 {
    assert!(calendar.admits_month(month));
  }

  // `@weekly` is Sunday, and it must be Sunday under a dialect that numbers Sunday as
  // something other than zero too. Building the nickname from a substitute expression
  // rather than from the day itself is exactly how that goes wrong.
  let weekly = Schedule::<Vixie>::parse("@weekly").unwrap();
  let calendar = weekly.calendar().unwrap();
  assert!(calendar.admits_weekday(Weekday::Sunday));
  assert!(!calendar.admits_weekday(Weekday::Monday));
  assert!(calendar.day_of_week_restricted());
  assert!(!calendar.day_of_month_restricted());

  let robfig_weekly = Schedule::<Robfig>::parse("@weekly").unwrap();
  let calendar = robfig_weekly.calendar().unwrap();
  assert!(calendar.admits_weekday(Weekday::Sunday));
  assert!(!calendar.admits_weekday(Weekday::Monday));

  let hourly = Schedule::<Vixie>::parse("@hourly").unwrap();
  let calendar = hourly.calendar().unwrap();
  assert!(calendar.admits_minute(0) && !calendar.admits_minute(1));
  for hour in 0..24 {
    assert!(calendar.admits_hour(hour));
  }
}

#[test]
fn reboot_is_its_own_variant_and_says_what_it_cannot_do() {
  let schedule = Schedule::<Vixie>::parse("@reboot").unwrap();
  assert_eq!(schedule, Schedule::Reboot);
  assert_eq!(
    schedule.calendar(),
    None,
    "there is no set of instants behind @reboot"
  );
  assert_eq!(schedule.every(), None);
}

#[test]
fn every_carries_a_core_duration() {
  let schedule = Schedule::<Robfig>::parse("@every 1h30m").unwrap();
  assert_eq!(schedule.every(), Some(Duration::from_secs(5400)));
  assert_eq!(schedule.calendar(), None);

  assert_eq!(
    Schedule::<Robfig>::parse("@every 90m").unwrap().every(),
    Some(Duration::from_secs(5400))
  );
  assert_eq!(
    *Schedule::<Robfig>::parse("@every 1x").unwrap_err().kind(),
    ErrorKind::UnknownDurationUnit
  );
  assert_eq!(
    *Schedule::<Robfig>::parse("@every 0s").unwrap_err().kind(),
    ErrorKind::ZeroDuration
  );
}

#[test]
fn the_year_field_reaches_the_year_set() {
  let schedule = Schedule::<Quartz>::parse("0 0 0 ? * * 2025-2027").unwrap();
  let calendar = schedule.calendar().unwrap();
  assert!(calendar.year_restricted());
  for year in 2025..=2027 {
    assert!(calendar.admits_year(year), "{year}");
  }
  assert!(!calendar.admits_year(2024));
  assert!(!calendar.admits_year(2028));

  // A star year places no restriction, so it admits years past what N enumerates.
  let star = Schedule::<Quartz>::parse("0 0 0 ? * * *").unwrap();
  let calendar = star.calendar().unwrap();
  assert!(!calendar.year_restricted());
  assert!(calendar.admits_year(1970));
  assert!(
    calendar.admits_year(2098),
    "`*` restricts nothing, not even 2098"
  );

  // A written 2098 is a different matter, and is refused by name.
  assert_eq!(
    *Schedule::<Quartz, 1>::parse("0 0 0 ? * * 2098")
      .unwrap_err()
      .kind(),
    ErrorKind::YearNotRepresentable {
      year: 2098,
      max_representable: 2097,
      required_n: 2,
    }
  );
  assert!(Schedule::<Quartz, 2>::parse("0 0 0 ? * * 2098").is_ok());
}

#[test]
fn surrounding_whitespace_and_a_trailing_newline_are_tolerated() {
  let plain = Schedule::<Vixie>::parse("0 0 * * *").unwrap();
  for spelling in [
    "  0 0 * * *",
    "0 0 * * *  ",
    "\t0 0 * * *\n",
    "0\t0\t*\t*\t*",
    "0   0   *   *   *",
  ] {
    assert_eq!(
      Schedule::<Vixie>::parse(spelling).unwrap(),
      plain,
      "{spelling:?}"
    );
  }
}

#[test]
fn errors_carry_a_span_into_the_expression() {
  let error = Schedule::<Vixie>::parse("0 0 * * 9").unwrap_err();
  assert_eq!(
    *error.kind(),
    ErrorKind::ValueOutOfRange {
      value: 9,
      min: 0,
      max: 7
    }
  );
  assert_eq!(error.span().start(), 8);
  assert_eq!(error.span().end(), 9);
  assert_eq!(error.field(), Some(crate::error::FieldKind::DayOfWeek));
}

#[test]
fn no_input_makes_the_parser_panic() {
  // Junk, truncations and oversized input, through every dialect. Nothing here may
  // panic; every one of them is either a schedule or an error.
  let corpus = [
    "0 0 * * *",
    "0 0 0 ? * * 2025",
    "@every 1h",
    "@reboot",
    "*/0 * * * *",
    "1--2 * * * *",
    "L L L L L",
    "###",
    ",,,,,",
    "0 0 0 0 0 0 0 0 0",
    "\u{65e5}\u{672c}\u{8a9e}",
    "@",
    "@@@",
    "0 0 * * SUNDAY",
    "4294967296 * * * *",
  ];
  let mut inputs: Vec<String> = Vec::new();
  for text in corpus {
    for end in 0..=text.len() {
      if text.is_char_boundary(end) {
        inputs.push(text.get(..end).expect("a boundary").into());
      }
    }
  }
  inputs.push("59,".repeat(2000));
  inputs.push("*".repeat(5000));

  for input in &inputs {
    let _ = Schedule::<Vixie>::parse(input);
    let _ = Schedule::<Quartz>::parse(input);
    let _ = Schedule::<Robfig>::parse(input);
    let _ = Schedule::<Quartz, 2>::parse(input);
  }
}

// ---------------------------------------------------------------------------
// The ergonomics gate: the shapes a caller actually writes must compile as written.
// ---------------------------------------------------------------------------

#[test]
fn the_common_call_needs_no_turbofish() {
  // Shape one: an annotated binding. `Schedule<Vixie>` is `Schedule<Vixie, 1>` because
  // N has a default, so the const parameter never appears.
  let annotated: Schedule<Vixie> = Schedule::parse("0 0 * * *").unwrap();
  assert!(annotated.calendar().is_some());

  // Shape two: a turbofish on the type, still without naming N.
  let turbofished = Schedule::<Quartz>::parse("0 0 0 ? * *").unwrap();
  assert!(turbofished.calendar().is_some());

  // Shape three: inference from a function's parameter, with nothing written at all.
  fn takes(schedule: Schedule<Robfig>) -> bool {
    schedule.calendar().is_some()
  }
  assert!(takes(Schedule::parse("0 0 0 * * *").unwrap()));

  // And the wide case, where N is written precisely because the caller needs it.
  let wide: Schedule<Quartz, 2> = Schedule::parse("0 0 0 ? * * 2098").unwrap();
  assert!(wide.calendar().is_some());
}

#[test]
fn a_dialect_read_from_configuration_dispatches_into_one_generic_function() {
  // The cost the design document names, written out so that it is known to compile
  // rather than assumed to. A caller whose dialect is not known until runtime pays a
  // match; the function it dispatches into is generic and monomorphised per arm.
  fn run<D: Dialect>(schedule: Schedule<D>) -> bool {
    schedule.calendar().is_some() || schedule.every().is_some()
  }

  enum Configured {
    Vixie,
    Quartz,
    Robfig,
  }

  for (configured, expression) in [
    (Configured::Vixie, "0 0 * * *"),
    (Configured::Quartz, "0 0 0 ? * *"),
    (Configured::Robfig, "@every 5m"),
  ] {
    let ok = match configured {
      Configured::Vixie => run(Schedule::<Vixie>::parse(expression).unwrap()),
      Configured::Quartz => run(Schedule::<Quartz>::parse(expression).unwrap()),
      Configured::Robfig => run(Schedule::<Robfig>::parse(expression).unwrap()),
    };
    assert!(ok, "{expression}");
  }
}

#[test]
fn a_schedule_is_a_fixed_size_value_that_never_allocates() {
  use core::mem::size_of;

  // The design document's table says forty bytes at N = 1. That figure is the sum of
  // the *bits* — 60 + 60 + 24 + 31 + 12 + 7 + 128 = 322, which is 40.25 bytes — and it
  // is not a layout any struct can have. The declared field widths alone come to 27
  // bytes, the predicates and the three restriction flags add eight more, the year word
  // is 16, and `u128` alignment rounds the total to 64.
  //
  // 48 would be reachable by folding the two predicate slots into one, since Quartz
  // never produces both; that is deliberately not done, because "at most one predicate"
  // is a fact about Quartz rather than about the type. The number pinned here is the
  // measured one, so that any change to the representation has to be deliberate.
  assert_eq!(size_of::<Schedule<Vixie, 1>>(), 64);
  assert_eq!(size_of::<Schedule<Quartz, 1>>(), 64);
  assert_eq!(
    size_of::<Schedule<Vixie, 2>>(),
    80,
    "one more year word costs sixteen bytes and nothing else"
  );
  assert_eq!(
    size_of::<Schedule<Vixie, 1>>(),
    size_of::<crate::Calendar<Vixie, 1>>(),
    "the enum discriminant fits in the calendar's padding, so the two other \
     variants are free"
  );

  // The dialect is a type, not a tag, so it costs a schedule nothing.
  assert_eq!(
    size_of::<Schedule<Vixie, 1>>(),
    size_of::<Schedule<Robfig, 1>>()
  );
}

#[test]
fn an_unrestricted_year_still_respects_the_dialects_bounds() {
  // Quartz declares 1970..=2099. A six-field expression has no year field at all, and
  // a `*` year field places no restriction, but neither makes the dialect's own bounds
  // go away. "Unrestricted" means the caller narrowed nothing, not that the set is
  // unbounded: an explicit 1969 or 2100 is refused at parse time, so the same years
  // have to be refused here.
  for expression in ["0 0 0 ? * *", "0 0 0 ? * * *"] {
    let schedule = Schedule::<Quartz, 2>::parse(expression).unwrap();
    let calendar = schedule.calendar().unwrap();
    assert!(!calendar.year_restricted(), "{expression}");
    assert!(
      !calendar.admits_year(1969),
      "{expression}: before Quartz's floor"
    );
    assert!(
      !calendar.admits_year(2100),
      "{expression}: past Quartz's ceiling"
    );
    assert!(calendar.admits_year(1970), "{expression}");
    assert!(calendar.admits_year(2099), "{expression}");
  }

  // A dialect with no year field is the one case that is genuinely unbounded, and it
  // has to stay that way.
  let vixie = Schedule::<Vixie>::parse("0 0 * * *").unwrap();
  let calendar = vixie.calendar().unwrap();
  assert!(calendar.admits_year(1));
  assert!(calendar.admits_year(1969));
  assert!(calendar.admits_year(9999));
}

#[test]
fn an_explicit_year_and_an_implicit_one_agree_about_every_year() {
  // The tell that the wildcard ceiling and `admits_year` were one bug rather than two
  // is that they gave different answers about the same year. N = 2 reaches past
  // Quartz's ceiling, so the only way writing a year out can fail here is the dialect
  // refusing it — and that must be exactly when an unrestricted schedule refuses it.
  let unrestricted = Schedule::<Quartz, 2>::parse("0 0 0 ? * * *").unwrap();
  let calendar = unrestricted.calendar().unwrap();

  for year in (1960u16..=2110).chain([0, 1, 9999]) {
    let mut written = String::new();
    core::fmt::Write::write_fmt(&mut written, format_args!("0 0 0 ? * * {year}")).unwrap();
    let explicit = Schedule::<Quartz, 2>::parse(&written).is_ok();
    let implicit = calendar.admits_year(year);
    assert_eq!(
      explicit,
      implicit,
      "year {year}: written out it {}, but an unrestricted schedule says {}",
      if explicit { "parses" } else { "is refused" },
      if implicit { "yes" } else { "no" },
    );
  }
}

#[test]
fn a_quartz_question_mark_has_to_be_the_whole_field() {
  // Quartz's `?` marks a field as unspecified; it is not a value that can be one
  // alternative among several, and Quartz's own parser rejects anything but
  // whitespace after it. Letting it hide inside a list means `check_dom_dow` only
  // ever sees the pair it expects, so `?,1` reads as though the field were `?`.
  for expression in [
    "0 0 0 ?,1 * MON",
    "0 0 0 1,? * MON",
    "0 0 0 1 * ?,MON",
    "0 0 0 1 * MON,?",
    "0 0 0 ?,1 * MON 2025",
    "0 0 0 1,? * MON 2025",
  ] {
    assert_eq!(
      *Schedule::<Quartz, 1>::parse(expression).unwrap_err().kind(),
      ErrorKind::QuestionMarkMustBeAlone { dialect: "Quartz" },
      "{expression}"
    );
  }

  // A lone `?` is of course still fine, at both widths.
  assert!(Schedule::<Quartz>::parse("0 0 0 ? * MON").is_ok());
  assert!(Schedule::<Quartz>::parse("0 0 0 ? * MON 2025").is_ok());
}

#[test]
fn the_go_dialects_question_mark_is_an_ordinary_list_item() {
  // `cron` 0.17 — the reference implementation — maps `?` to the same specifier as
  // `*` and accepts it inside a comma list. Applying Quartz's must-be-alone rule to
  // every dialect would be a dialect-blind check losing a dialect difference, which is
  // the defect class this round is fixing elsewhere. This is the guard against it.
  let listed = Schedule::<Robfig>::parse("0 0 0 ?,1 * MON").expect("legal Go");
  let starred = Schedule::<Robfig>::parse("0 0 0 * * MON").expect("legal Go");
  let listed = listed.calendar().expect("a calendar");
  let starred = starred.calendar().expect("a calendar");
  for day in 1..=31 {
    assert!(listed.admits_day_of_month(day), "day {day}");
    assert_eq!(
      listed.admits_day_of_month(day),
      starred.admits_day_of_month(day),
      "day {day}"
    );
  }
}

#[test]
fn a_wrapping_month_range_reaches_the_right_months_end_to_end() {
  let schedule = Schedule::<Quartz>::parse("0 0 0 ? NOV-FEB MON").expect("legal Quartz");
  let calendar = schedule.calendar().expect("a calendar");
  for month in [11, 12, 1, 2] {
    assert!(calendar.admits_month(month), "month {month}");
  }
  for month in 3..=10 {
    assert!(!calendar.admits_month(month), "month {month}");
  }
}

#[test]
fn a_wildcard_in_a_list_is_not_narrowed_to_the_storage_ceiling() {
  // A field is restricted the moment a second item appears, so a `*` beside another
  // item has to be materialised against the *dialect's* ceiling rather than the
  // storage one. Quartz reaches 2099 and `Years<1>` reaches 2097, so this is the
  // YearNotRepresentable case and it must say so rather than quietly dropping 2098
  // and 2099 — which is R1's defect reached one level up.
  for expression in [
    "0 0 0 ? * * *,2025",
    "0 0 0 ? * * */1,2025",
    "0 0 0 ? * * 2025,*",
    "0 0 0 ? * * 2025,*/1",
    "0 0 0 ? * * *,*",
  ] {
    assert_eq!(
      *Schedule::<Quartz, 1>::parse(expression).unwrap_err().kind(),
      ErrorKind::YearNotRepresentable {
        year: 2098,
        max_representable: 2097,
        required_n: 2,
      },
      "{expression}"
    );
  }

  // At N = 2 the whole dialect range fits, so the same expressions parse and the
  // wildcard really does reach 2099.
  for expression in ["0 0 0 ? * * *,2025", "0 0 0 ? * * */1,2025"] {
    let schedule = Schedule::<Quartz, 2>::parse(expression).expect(expression);
    let calendar = schedule.calendar().expect("a calendar");
    assert!(calendar.year_restricted(), "{expression}");
    assert!(calendar.admits_year(1970), "{expression}");
    assert!(calendar.admits_year(2099), "{expression}");
    assert!(!calendar.admits_year(2100), "{expression}");
  }
}

#[test]
fn every_field_admits_the_same_values_however_its_wildcard_is_written() {
  // The reviewer's question, answered in the tests rather than in prose: can any field
  // other than the year observe the difference between `*`, `*/1` and a `*` written
  // beside another item? It must not.
  //
  // Only the year has a sink narrower than its dialect, so only the year could ever
  // have observed a storage ceiling. The sweep covers every field anyway, because an
  // unrestricted field now stores no bits and a missed short-circuit would make a
  // whole field answer "no" to everything it is asked.
  fn admits(calendar: &Calendar<Robfig, 1>, index: usize, value: u8) -> bool {
    match index {
      0 => calendar.admits_second(value),
      1 => calendar.admits_minute(value),
      2 => calendar.admits_hour(value),
      3 => calendar.admits_day_of_month(value),
      4 => calendar.admits_month(value),
      _ => Weekday::from_canonical(value).is_some_and(|day| calendar.admits_weekday(day)),
    }
  }

  // Field position in a six-field expression, and the values it is defined over.
  let fields: [(usize, u8, u8); 6] = [
    (0, 0, 59),
    (1, 0, 59),
    (2, 0, 23),
    (3, 1, 31),
    (4, 1, 12),
    (5, 0, 6),
  ];

  for (index, lo, hi) in fields {
    for shape in ["*", "*/1", "*,*"] {
      let mut parts = ["*"; 6];
      parts[index] = shape;
      let expression = parts.join(" ");
      let schedule = Schedule::<Robfig>::parse(&expression)
        .unwrap_or_else(|e| panic!("{expression:?} should parse: {e}"));
      let calendar = schedule.calendar().expect("a calendar");

      for value in lo..=hi {
        assert!(
          admits(calendar, index, value),
          "{expression:?} field {index} value {value}"
        );
      }
      if lo > 0 {
        assert!(
          !admits(calendar, index, lo - 1),
          "{expression:?} below {lo}"
        );
      }
      assert!(
        !admits(calendar, index, hi + 1),
        "{expression:?} above {hi}"
      );
    }
  }

  // And the year, where the difference could actually have been observed. N = 2 so
  // that every shape parses and the comparison is about the wildcard rather than the
  // width.
  for shape in ["*", "*/1", "*,2025"] {
    let mut expression = String::new();
    core::fmt::Write::write_fmt(&mut expression, format_args!("0 0 0 ? * * {shape}")).unwrap();
    let schedule = Schedule::<Quartz, 2>::parse(&expression)
      .unwrap_or_else(|e| panic!("{expression:?} should parse: {e}"));
    let calendar = schedule.calendar().expect("a calendar");
    for year in [1970u16, 2025, 2097, 2098, 2099] {
      assert!(calendar.admits_year(year), "{expression:?} year {year}");
    }
    assert!(!calendar.admits_year(1969), "{expression:?} year 1969");
    assert!(!calendar.admits_year(2100), "{expression:?} year 2100");
  }
}

/// The byte-run field counter against the token walk it replaced.
///
/// [`count_fields`](super::count_fields) used to run a whole lexer pass whose tokens were
/// thrown away, which made every expression get tokenised twice. The byte scan is only a
/// legitimate replacement if it counts the same fields, so the token walk is kept here as
/// the oracle and the two are held against each other — including on the inputs where the
/// argument is least obvious: multi-byte UTF-8, a byte that begins no token, a digit run
/// too long to be a value, and each member of the whitespace class.
#[test]
fn equivalent_to_the_token_walk() {
  use crate::token::{Cursor, Token};

  /// The counter as it was written when it walked tokens.
  fn by_tokens(input: &str) -> usize {
    let mut cursor = Cursor::new(input);
    let mut fields = 0usize;
    let mut inside = false;
    while let Some((token, _)) = cursor.bump() {
      if matches!(token, Ok(Token::Space)) {
        inside = false;
      } else if !inside {
        inside = true;
        fields = fields.saturating_add(1);
      }
    }
    fields
  }

  const CORPUS: &[&str] = &[
    "30 2 * * 1-5",
    "0,15,30,45 0-23/2 1-15 JAN-JUN MON-FRI",
    "0 30 2 * * 1-5",
    "0 15 10 ? * MON-FRI 2020-2030",
    "0 0 * * 99",
    "@daily",
    "",
    "   ",
    " a  b ",
    // A byte that begins no token: the lexer still advances past it, so it is inside a
    // field for both counters.
    "1%2 3",
    "@every 5s",
    // Multi-byte UTF-8: the lexer's error advances one character, the byte scan advances
    // one byte, and neither is whitespace, so the field count is the same either way.
    "é * * * *",
    // A digit run too long for `u32`: one error token spanning the whole run.
    "99999999999999999999 *",
    // Every member of the whitespace class, mixed, leading and trailing.
    "\t1 2\r\n3\x0C4 5\t",
  ];

  assert_eq!(CORPUS.len(), 14, "the corpus the equivalence was proven on");

  for &input in CORPUS {
    assert_eq!(
      super::count_fields(input),
      by_tokens(input),
      "field count disagrees on {input:?}"
    );
  }
}

/// Every expression the dialect table exercises.
///
/// Exposed to the crate so that the lexer's differential oracle can scan the whole
/// parser corpus rather than a copy of it, which would drift the moment a row is added
/// here and not there.
pub(crate) fn corpus() -> impl Iterator<Item = &'static str> {
  TABLE.iter().map(|row| row.expression)
}
