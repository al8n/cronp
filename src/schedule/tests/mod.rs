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
  date::{CivilDateTime, Weekday},
  dialect::{Cronexpr, Dialect, Quartz, Robfig, Vixie},
  error::{ErrorKind, FieldKind},
};

/// The same contract as [`REPORTED`], taken to its cross product and generated.
///
/// The table below is hand-written and therefore representative; that module is every
/// dialect against every field position against every lexical failure against every
/// place inside a field, with the expected kind, span and field computed from the
/// templates that write each expression rather than from any parser. It also carries the
/// seam that stops the reference parser being edited in silence.
mod lexical_contract;

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
  assert!(!calendar.day_of_month_restricted);
  assert!(!calendar.day_of_week_restricted);
}

#[test]
fn the_nicknames_expand_to_what_they_say() {
  let yearly = Schedule::<Vixie>::parse("@yearly").unwrap();
  let calendar = yearly.calendar().unwrap();
  assert!(calendar.admits_minute(0) && !calendar.admits_minute(1));
  assert!(calendar.admits_hour(0) && !calendar.admits_hour(1));
  assert!(calendar.admits_day_of_month(1) && !calendar.admits_day_of_month(2));
  assert!(calendar.admits_month(1) && !calendar.admits_month(2));
  assert!(calendar.day_of_month_restricted);

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
  assert!(calendar.day_of_week_restricted);
  assert!(!calendar.day_of_month_restricted);

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

/// A nickname's open day field carries the wildcard, exactly as a written `*` does.
///
/// The census's highest-severity row. `nickname_calendar` used to assign bitsets into a
/// zeroed `Calendar` and never assign the outcomes, so both day fields reported no
/// wildcard and `@weekly` took the union rule's "or" branch: every day of the week,
/// where the equivalent `0 0 * * 0` fired on Sundays. Upstream sets the flag on every
/// nickname it expands — vixie's `entry.c` sets `DOM_STAR` for `@weekly` and `DOW_STAR`
/// for the rest, and robfig's `all()` carries `starBit` into whichever fields it fills.
///
/// The repair is that a `Calendar` cannot be built without an outcome per field, so this
/// test is checking a value that can no longer be *absent*, only wrong.
#[test]
fn a_nickname_leaves_its_open_day_field_carrying_the_wildcard() {
  // Nickname, day-of-month wildcard, day-of-week wildcard. Whichever field the nickname
  // pins is not a wildcard, and whichever it leaves open is.
  let cases: &[(&str, bool, bool)] = &[
    ("@yearly", false, true),
    ("@annually", false, true),
    ("@monthly", false, true),
    ("@weekly", true, false),
    ("@daily", true, true),
    ("@midnight", true, true),
    ("@hourly", true, true),
  ];

  for &(nickname, dom, dow) in cases {
    let vixie = Schedule::<Vixie>::parse(nickname).unwrap();
    let calendar = vixie.calendar().unwrap();
    assert_eq!(
      (
        calendar.day_of_month_wildcard,
        calendar.day_of_week_wildcard
      ),
      (dom, dow),
      "Vixie {nickname}"
    );

    // The Go dialect reaches the same answer down the other fold: a bare `*` is an
    // unconstrained item wherever it sits, so it witnesses under `AnyUnconstrained` too.
    let robfig = Schedule::<Robfig>::parse(nickname).unwrap();
    let calendar = robfig.calendar().unwrap();
    assert_eq!(
      (
        calendar.day_of_month_wildcard,
        calendar.day_of_week_wildcard
      ),
      (dom, dow),
      "Robfig {nickname}"
    );
  }

  // And the nickname agrees with the expression it is short for, flag for flag. This is
  // the pair the census measured as diverging.
  let weekly = Schedule::<Vixie>::parse("@weekly").unwrap();
  let written = Schedule::<Vixie>::parse("0 0 * * 0").unwrap();
  assert_eq!(
    weekly.calendar().unwrap(),
    written.calendar().unwrap(),
    "`@weekly` and `0 0 * * 0` are the same schedule, so they are the same calendar"
  );
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
  assert!(calendar.year_restricted);
  for year in 2025..=2027 {
    assert!(calendar.admits_year(year), "{year}");
  }
  assert!(!calendar.admits_year(2024));
  assert!(!calendar.admits_year(2028));

  // A star year places no restriction, so it admits years past what N enumerates.
  let star = Schedule::<Quartz>::parse("0 0 0 ? * * *").unwrap();
  let calendar = star.calendar().unwrap();
  assert!(!calendar.year_restricted);
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
  use core::mem::{align_of, size_of};

  // The design document's table says forty bytes at N = 1. That figure is the sum of
  // the *bits* — 60 + 60 + 24 + 31 + 12 + 7 + 128 = 322, which is 40.25 bytes — and it
  // is not a layout any struct can have. The declared field widths alone come to 27
  // bytes, the two predicates and the seven restriction flags add eleven more, and one
  // year word is 16, for 54 bytes of content before any padding.
  //
  // That content is then rounded up to a whole number of `u128`'s own alignment, since
  // the year word is the widest-aligned field `Calendar` has — and `u128`'s alignment is
  // target-defined rather than fixed by the language: 16 bytes on x86_64, aarch64, and
  // most other targets, but only 8 on s390x. The identical field layout therefore
  // measures 64 bytes on the former and 56 on the latter. `expected_bytes` below derives
  // that rounding from `align_of::<u128>()` instead of asserting one target's answer, so
  // the number pinned here — 54 bytes of content at N = 1, 70 at N = 2 — is still the
  // measured one, and any change to the representation still has to be deliberate, on
  // every target.
  //
  // 48 would be reachable by folding the two predicate slots into one, since Quartz
  // never produces both; that is deliberately not done, because "at most one predicate"
  // is a fact about Quartz rather than about the type.

  // 27 declared bytes + 11 (two predicates, seven restriction flags) + 16 per year word,
  // rounded up to this target's `u128` alignment.
  fn expected_bytes(year_words: usize) -> usize {
    (27 + 11 + 16 * year_words).next_multiple_of(align_of::<u128>())
  }

  assert_eq!(size_of::<Schedule<Vixie, 1>>(), expected_bytes(1));
  assert_eq!(size_of::<Schedule<Quartz, 1>>(), expected_bytes(1));
  assert_eq!(
    size_of::<Schedule<Vixie, 2>>(),
    expected_bytes(2),
    "one more year word costs sixteen bytes and nothing else, on every target"
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
    assert!(!calendar.year_restricted, "{expression}");
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
fn a_wildcard_in_a_list_leaves_the_whole_field_unrestricted() {
  // A union that contains the whole domain *is* the whole domain, so a `*` beside
  // another item narrows nothing and the field stores nothing at all. The year field is
  // where the difference was observable: Quartz reaches 2099 and `Years<1>` reaches
  // 2097, so materialising the wildcard used to report `YearNotRepresentable` at 2098
  // for expressions that place no year restriction whatsoever.
  for expression in [
    "0 0 0 ? * * *,2025",
    "0 0 0 ? * * */1,2025",
    "0 0 0 ? * * 2025,*",
    "0 0 0 ? * * 2025,*/1",
    "0 0 0 ? * * *,*",
    "0 0 0 ? * * 2025,*,2026",
    "0 0 0 ? * * *,1970-2097",
  ] {
    let schedule = Schedule::<Quartz, 1>::parse(expression)
      .unwrap_or_else(|error| panic!("{expression:?} restricts no year: {error}"));
    let calendar = schedule.calendar().expect("a calendar");
    assert!(!calendar.year_restricted, "{expression}");
    assert!(
      calendar.years().is_empty(),
      "{expression}: an unrestricted field stores nothing"
    );
    for year in [1970u16, 2025, 2097, 2098, 2099] {
      assert!(calendar.admits_year(year), "{expression}: {year}");
    }
    assert!(!calendar.admits_year(1969), "{expression}");
    assert!(!calendar.admits_year(2100), "{expression}");
  }

  // The storage width was never the question, and this is what says so: the same
  // expressions answer identically at both widths, for every year either could hold.
  for expression in [
    "0 0 0 ? * * *,2025",
    "0 0 0 ? * * 2025,*",
    "0 0 0 ? * * *,*",
  ] {
    let narrow = Schedule::<Quartz, 1>::parse(expression).expect(expression);
    let wide = Schedule::<Quartz, 2>::parse(expression).expect(expression);
    let narrow = narrow.calendar().expect("a calendar");
    let wide = wide.calendar().expect("a calendar");
    for year in 1960u16..=2110 {
      assert_eq!(
        narrow.admits_year(year),
        wide.admits_year(year),
        "{expression}: {year}"
      );
    }
  }

  // `Calendar` is `PartialEq`, and an unrestricted field now stores nothing whatever is
  // listed beside its wildcard — so two spellings of one schedule compare equal where
  // they used to differ by a bitset one of them never consulted. The counter-case is
  // what keeps that from being a loss: `10,*` is the same set of days as `*` and a
  // different rule under Vixie, so the witness still separates them.
  let listed = Schedule::<Vixie>::parse("0 0 *,10 * MON").expect("legal Vixie");
  let starred = Schedule::<Vixie>::parse("0 0 * * MON").expect("legal Vixie");
  let star_last = Schedule::<Vixie>::parse("0 0 10,* * MON").expect("legal Vixie");
  assert_eq!(
    listed, starred,
    "`*,10` and `*` are one schedule: the same days, and the same side of the union rule"
  );
  assert_ne!(
    star_last, starred,
    "`10,*` is the same days and the other side of the union rule, so it is not the \
     same schedule"
  );
}

/// Day of week is where "unrestricted" and "materialised in full" could have parted.
///
/// Every other field stores the digit it was written with, so a field that admits every
/// value and a field that stores every value answer alike by inspection. Day of week is
/// the one that converts: Vixie writes it over `0..=7` and Quartz over `1..=7`, and both
/// name the same seven canonical days. Expanding a wildcard walked the *raw* range and
/// folded each digit; not expanding it answers over the *canonical* range instead. Those
/// are two different intervals reaching the same seven days, and the only thing that
/// makes them the same answer is that the fold is onto — which is a property worth
/// asserting rather than assuming, in every dialect that has the rule.
#[test]
fn an_unrestricted_day_of_week_admits_the_same_seven_days_the_expansion_did() {
  /// `written_out` spells the field's whole raw range, which is still materialised and
  /// still folded per digit. Every other spelling is one the wildcard makes
  /// unrestricted, and each has to admit exactly the days the written-out one does — so
  /// a fold that stopped being onto the seven canonical days fails here rather than
  /// leaving the two paths quietly disagreeing.
  fn check<D: Dialect>(written_out: &str, unrestricted: &[&str]) {
    let expansion = Schedule::<D>::parse(written_out)
      .unwrap_or_else(|error| panic!("{written_out:?} under {}: {error}", D::NAME));
    let expansion = expansion.calendar().expect("a calendar");

    let mut reached = 0usize;
    for canonical in 0..=6u8 {
      let weekday = Weekday::from_canonical(canonical).expect("a day of the week");
      if expansion.admits_weekday(weekday) {
        reached = reached.saturating_add(1);
      }
    }
    assert_eq!(
      reached,
      7,
      "{written_out:?} under {}: the field's own raw range must fold onto all seven \
       canonical days, or there is nothing for the wildcard to agree with",
      D::NAME
    );

    for expression in unrestricted {
      let schedule = Schedule::<D>::parse(expression)
        .unwrap_or_else(|error| panic!("{expression:?} under {}: {error}", D::NAME));
      let calendar = schedule.calendar().expect("a calendar");
      for canonical in 0..=6u8 {
        let weekday = Weekday::from_canonical(canonical).expect("a day of the week");
        assert_eq!(
          calendar.admits_weekday(weekday),
          expansion.admits_weekday(weekday),
          "{expression:?} under {}: {weekday:?} is admitted by {written_out:?} and not \
           by this, so storing nothing is not the same as storing everything",
          D::NAME
        );
      }
    }
  }

  // The three `ZeroSunday` dialects, whose eight digits name seven days.
  check::<Vixie>(
    "0 0 * * 0-7",
    &["0 0 * * *", "0 0 * * *,SUN", "0 0 * * SUN,*", "0 0 * * *,*"],
  );
  check::<Cronexpr>(
    "0 0 * * 0-7",
    &["0 0 * * *", "0 0 * * *,SUN", "0 0 * * SUN,*"],
  );
  check::<Robfig>(
    "0 0 0 * * 0-7",
    &[
      "0 0 0 * * *",
      "0 0 0 * * *,SUN",
      "0 0 0 * * SUN,*",
      "0 0 0 * * ?,SUN",
    ],
  );
  // And Quartz, which spells each day exactly once and starts at one rather than zero.
  check::<Quartz>(
    "0 0 0 ? * 1-7",
    &["0 0 0 ? * *", "0 0 0 ? * *,SUN", "0 0 0 ? * SUN,*"],
  );
}

/// The family the cause implies, decided member by member.
///
/// `restricted` was computed from **syntax** — "exactly one item, and that item was
/// bare" — for a property that is **semantic**: does the union this field denotes
/// constrain anything. Every spelling whose union is the field's whole domain belongs to
/// that family, so the fix is worth exactly as much as the enumeration behind it. The
/// list is here rather than in prose so that it can be read against the cause and so
/// that each decision is checked instead of described.
///
/// Two of the members are declined, and for reasons that are about the *rule* rather
/// than about effort:
///
///   - **A range or list whose union happens to be the whole domain** — `1970-2099`,
///     `0-29,30-59`. Detecting the first would be another syntactic test for a semantic
///     property, which is the same defect one level down: Vixie's day-of-week is written
///     over `0..=7` and names seven days, so `0-7` would normalise and `0-6` — the very
///     same seven days — would not. Detecting the second cannot be done without
///     materialising the union, and materialising it is the operation that fails. A
///     written-out set is what answers for its field, and `YearNotRepresentable` names
///     the `N` that holds what was written.
///   - **A written value the instantiation cannot hold, beside a wildcard** — `*,2098`,
///     and `*,1970-2099` for the same reason one step in. That year is the caller's, not
///     the parser's invention, and the rule that refuses it is not about storage at all:
///     **every item is checked on its own, and a wildcard beside it excuses nothing**.
///     `*,2100`, `*,2030-2020` and `*,ZZZ` are the same rule with a different refusal,
///     and they are in the list below as the controls that say so. What the repair
///     removed is categorically different — the parser refusing an expression because a
///     range *it* invented did not fit.
///
/// One member is not admitted by the grammar at all: there is no wildcard inside a
/// range, because `*` takes only a `/step` after it. `*-5` is a parse error, so the
/// implication is void there rather than decided.
#[test]
fn every_member_of_the_unrestricted_family_is_decided() {
  /// What the year field must do with one spelling at the default width.
  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  enum Verdict {
    /// It parses and places no restriction: nothing stored, every dialect year admitted.
    Unrestricted,
    /// It parses and the years it enumerated are what answer for the field.
    Restricted,
    /// It is refused because 2098 is past `Years<1>`, naming the `N` that would hold it.
    NeedsTwoWords,
    /// It is refused for a reason that is not about storage.
    Refused(ErrorKind),
  }
  use Verdict::{NeedsTwoWords, Refused, Restricted, Unrestricted};

  const FAMILY: &[(&str, Verdict, &str)] = &[
    // ----- normalised: some item is, by construction, the whole domain -----
    ("*", Unrestricted, "the member every other one generalises"),
    ("*/1", Unrestricted, "a stride of one narrows nothing"),
    ("*,2025", Unrestricted, "a bare wildcard first in a list"),
    (
      "2025,*",
      Unrestricted,
      "and last in one: the same union, so it owes the same answer — this is the pair \
       the syntactic rule got wrong in one direction only",
    ),
    ("*,*", Unrestricted, "two wildcards and nothing else"),
    (
      "*/1,2025",
      Unrestricted,
      "a stride-one star is bare wherever it sits",
    ),
    ("2025,*/1", Unrestricted, "the same, at the other end"),
    ("2025,*,2026", Unrestricted, "in the middle of a list"),
    (
      "*,1970-2097",
      Unrestricted,
      "beside a range this width does hold: the wildcard settles the field without the \
       range having to be recognised as covering anything",
    ),
    // ----- declined: the union is the domain and no single item says so -----
    (
      "1970-2099",
      NeedsTwoWords,
      "a written-out set is what answers for the field, and this one names 2098",
    ),
    (
      "1970-2099/1",
      NeedsTwoWords,
      "the same set, written with a stride of one",
    ),
    (
      "2025,1970-2024,2026-2099",
      NeedsTwoWords,
      "a list whose union is every year, which cannot be seen without building it",
    ),
    // ----- declined: a written year this width cannot hold -----
    (
      "*,2098",
      NeedsTwoWords,
      "2098 is the caller's, not the parser's invention",
    ),
    (
      "2098,*",
      NeedsTwoWords,
      "and the order does not change that",
    ),
    (
      "*,1970-2099",
      NeedsTwoWords,
      "the same, one step in: the range names 2098 whether or not the union needs it",
    ),
    // ----- the controls: every item is checked on its own, whatever the refusal -----
    (
      "*,2100",
      Refused(ErrorKind::ValueOutOfRange {
        value: 2100,
        min: 1970,
        max: 2099,
      }),
      "no wildcard excuses a year the dialect does not declare, at any N",
    ),
    (
      "*,2030-2020",
      Refused(ErrorKind::ReversedRange {
        start: 2030,
        end: 2020,
      }),
      "nor a range that runs backwards — the rule above is not about storage",
    ),
    (
      "*,ZZZ",
      Refused(ErrorKind::UnexpectedCharacter),
      "nor bytes that are not a token at all",
    ),
    // ----- unreachable: the grammar has no wildcard inside a range -----
    (
      "*-5",
      Refused(ErrorKind::UnexpectedToken),
      "`*` takes only a `/step`, so a wildcard cannot be one end of a range",
    ),
    (
      "2025-*",
      Refused(ErrorKind::UnexpectedToken),
      "nor the other end — written with a year in range, so it is the `*` that is \
       refused and not the number in front of it",
    ),
    // ----- restrictions that stay restrictions -----
    (
      "*/2",
      NeedsTwoWords,
      "a stride above one narrows, and the set it names reaches 2098",
    ),
    ("2025", Restricted, "one year is a year"),
    ("2025,2026", Restricted, "and so are two"),
    ("1970-2097", Restricted, "a range this width does hold"),
  ];

  for &(field, verdict, why) in FAMILY {
    let mut expression = String::new();
    core::fmt::Write::write_fmt(&mut expression, format_args!("0 0 0 ? * * {field}")).unwrap();
    let parsed = Schedule::<Quartz, 1>::parse(&expression);

    match verdict {
      Unrestricted => {
        let schedule =
          parsed.unwrap_or_else(|error| panic!("{field:?} ({why}) must parse: {error}"));
        let calendar = schedule.calendar().expect("a calendar");
        assert!(!calendar.year_restricted, "{field:?}: {why}");
        assert!(calendar.years().is_empty(), "{field:?}: {why}");
        assert!(calendar.admits_year(2098), "{field:?}: {why}");
      }
      Restricted => {
        let schedule =
          parsed.unwrap_or_else(|error| panic!("{field:?} ({why}) must parse: {error}"));
        let calendar = schedule.calendar().expect("a calendar");
        assert!(calendar.year_restricted, "{field:?}: {why}");
        assert!(!calendar.years().is_empty(), "{field:?}: {why}");
      }
      NeedsTwoWords => {
        let error = parsed
          .err()
          .unwrap_or_else(|| panic!("{field:?} ({why}) must be refused"));
        assert_eq!(
          *error.kind(),
          ErrorKind::YearNotRepresentable {
            year: 2098,
            max_representable: 2097,
            required_n: 2,
          },
          "{field:?}: {why}"
        );
        // Declined only because of the width: at N = 2 every one of them parses, which
        // is what separates a storage refusal from a grammar one.
        let mut wide = String::new();
        core::fmt::Write::write_fmt(&mut wide, format_args!("0 0 0 ? * * {field}")).unwrap();
        assert!(
          Schedule::<Quartz, 2>::parse(&wide).is_ok(),
          "{field:?} is refused at N = 2 as well, so the width is not what refuses it"
        );
      }
      Refused(kind) => {
        let error = parsed
          .err()
          .unwrap_or_else(|| panic!("{field:?} ({why}) must be refused"));
        assert_eq!(*error.kind(), kind, "{field:?}: {why}");
      }
    }
  }

  // Every member carries its reason, and the two declined classes are both present: a
  // family list with nothing declined in it is a list that was not derived.
  assert!(FAMILY.iter().all(|&(_, _, why)| !why.is_empty()));
  assert!(
    FAMILY
      .iter()
      .any(|&(_, verdict, _)| verdict == Unrestricted)
      && FAMILY
        .iter()
        .any(|&(_, verdict, _)| verdict == NeedsTwoWords)
      && FAMILY
        .iter()
        .any(|&(_, verdict, _)| matches!(verdict, Refused(_))),
    "the family lost one of its three classes"
  );
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
    // `5` is a legal value in every one of the six, so the same list reaches them all.
    for shape in ["*", "*/1", "*,*", "*,5", "5,*", "*/1,5", "5,*,6"] {
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

  // The Go dialect's `?` is another spelling of `*`, so it is a bare item too and a
  // list carrying one is unrestricted for the same reason. Only the two day fields take
  // it, which is why it is not in the sweep above.
  for (index, lo, hi) in [(3usize, 1u8, 31u8), (5, 0, 6)] {
    for shape in ["?", "?,1", "1,?"] {
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
    }
  }

  // And the year, where the difference could actually have been observed. N = 2 so
  // that every shape parses and the comparison is about the wildcard rather than the
  // width.
  for shape in ["*", "*/1", "*,2025", "2025,*"] {
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
  use crate::schedule::reference::token::{Cursor, Token};

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

// ---------------------------------------------------------------------------
// Where a lexical failure is reported, exactly.
//
// These are deliberately not differential. The parser and the reference it is measured
// against both used to answer this wrongly and identically, so no oracle comparing them
// could ever have said so: a differential proves a *change*, and says nothing about
// whether the behaviour being preserved is right. Both sides can be wrong together, and
// here they were. What follows is the contract written down — kind, span and field, for
// every position a bad byte can occupy.
// ---------------------------------------------------------------------------

/// One expression, and the error it must produce, down to the byte.
struct Reported {
  expression: &'static str,
  kind: ErrorKind,
  span: (usize, usize),
  field: Option<FieldKind>,
  why: &'static str,
}

const REPORTED: &[Reported] = &[
  // ----- an expression that really is empty -----
  Reported {
    expression: "",
    kind: ErrorKind::EmptyExpression,
    span: (0, 0),
    field: None,
    why: "nothing at all: the one thing `EmptyExpression` is for",
  },
  Reported {
    expression: "   ",
    kind: ErrorKind::EmptyExpression,
    span: (3, 3),
    field: None,
    why: "whitespace only is also nothing, and points past the whitespace",
  },
  // ----- a failure at the head of an expression -----
  Reported {
    expression: "% 2 3 4 5",
    kind: ErrorKind::UnexpectedCharacter,
    span: (0, 1),
    field: Some(FieldKind::Minute),
    why: "the first byte of the first field: reported there, not as an empty expression",
  },
  Reported {
    expression: "4294967296 * * * *",
    kind: ErrorKind::NumberTooLarge,
    span: (0, 10),
    field: Some(FieldKind::Minute),
    why: "a leading run past `u32`, over the whole run",
  },
  Reported {
    expression: "@ 2 3 4 5",
    kind: ErrorKind::UnexpectedCharacter,
    span: (0, 1),
    field: Some(FieldKind::Minute),
    why: "a lone `@` is not a nickname; it is an ordinary bad byte in an ordinary field",
  },
  Reported {
    expression: "%",
    kind: ErrorKind::WrongFieldCount {
      found: 1,
      min: 5,
      max: 5,
      dialect: "Vixie",
    },
    span: (0, 1),
    field: None,
    why: "one field is not five whatever the field contains, and the count is checked \
          first for every expression — `*` alone answers the same way",
  },
  // ----- a failure in the middle of a field -----
  Reported {
    expression: "1% 2 3 4 5",
    kind: ErrorKind::UnexpectedCharacter,
    span: (1, 2),
    field: Some(FieldKind::Minute),
    why: "after an item, in the minute — not in the hour, which is where the parser \
          used to be standing when it finally tripped over it",
  },
  Reported {
    expression: "0 % 3 4 5",
    kind: ErrorKind::UnexpectedCharacter,
    span: (2, 3),
    field: Some(FieldKind::Hour),
    why: "a whole field that is one bad byte, in the middle of the expression",
  },
  Reported {
    expression: "*4294967296 * * * *",
    kind: ErrorKind::NumberTooLarge,
    span: (1, 11),
    field: Some(FieldKind::Minute),
    why: "the other lexical failure, after an item rather than before one",
  },
  // ----- a failure in the last field, where there is no next field to blame -----
  Reported {
    expression: "0 0 * * 5%",
    kind: ErrorKind::UnexpectedCharacter,
    span: (9, 10),
    field: Some(FieldKind::DayOfWeek),
    why: "the last field: this used to be `TrailingInput` with no field at all",
  },
  Reported {
    expression: "0 0 * * %",
    kind: ErrorKind::UnexpectedCharacter,
    span: (8, 9),
    field: Some(FieldKind::DayOfWeek),
    why: "the last field, and the whole of it",
  },
  Reported {
    expression: "0 0 * * é",
    kind: ErrorKind::UnexpectedCharacter,
    span: (8, 10),
    field: Some(FieldKind::DayOfWeek),
    why: "a two-byte character is spanned whole, so the span is always sliceable",
  },
];

#[test]
fn a_lexical_failure_is_reported_where_it_happens() {
  for case in REPORTED {
    let error = Schedule::<Vixie>::parse(case.expression).expect_err(case.expression);
    assert_eq!(
      (
        *error.kind(),
        error.span().start(),
        error.span().end(),
        error.field()
      ),
      (case.kind, case.span.0, case.span.1, case.field),
      "{:?} ({})",
      case.expression,
      case.why
    );
    assert!(
      case
        .expression
        .get(error.span().start()..error.span().end())
        .is_some(),
      "{:?} reported a span that is not a slice of it",
      case.expression
    );
  }
}

/// The trailing year field reports its own bad bytes too.
///
/// Worth its own case because the year is read after the loop over the six fixed fields,
/// on a branch of its own, and it is the one field whose failure used to escape as
/// `TrailingInput`.
#[test]
fn the_year_field_reports_its_own_lexical_failure() {
  let error = Schedule::<Quartz>::parse("0 0 0 ? * 1 2020%").expect_err("a bad byte");
  assert_eq!(*error.kind(), ErrorKind::UnexpectedCharacter);
  assert_eq!((error.span().start(), error.span().end()), (16, 17));
  assert_eq!(error.field(), Some(FieldKind::Year));
}

/// What each dialect makes of `H`, named outright rather than differentially.
///
/// The reference parser is never given a seed, so it can only watch the two refusals;
/// everything a seed makes reachable is pinned here, by kind and span and field, which is
/// what the reference module's own rule asks for when a behaviour change moves both
/// parsers at once.
struct Hashed {
  expression: &'static str,
  seed: Option<u64>,
  /// `None` to accept, otherwise the kind, the span and the field it is reported in.
  expect: Option<(ErrorKind, (usize, usize), Option<FieldKind>)>,
  why: &'static str,
}

const HASHED: &[Hashed] = &[
  Hashed {
    expression: "H 0 * * *",
    seed: Some(0),
    expect: None,
    why: "a seed makes `H` an ordinary value in a dialect that has it",
  },
  Hashed {
    expression: "H 0 * * *",
    seed: None,
    expect: Some((
      ErrorKind::HashedValueNeedsSeed,
      (0, 1),
      Some(FieldKind::Minute),
    )),
    why: "the dialect has `H`; this parse has no seed, which is a different fault",
  },
  Hashed {
    expression: "H/2 0 * * *",
    seed: Some(7),
    expect: Some((ErrorKind::UnexpectedToken, (1, 2), Some(FieldKind::Minute))),
    why: "`H` is a whole item, as it is in cronexpr; a step cannot follow it",
  },
  Hashed {
    expression: "1-H 0 * * *",
    seed: Some(7),
    expect: Some((ErrorKind::UnexpectedToken, (2, 3), Some(FieldKind::Minute))),
    why: "`H` is not a value, so it cannot be one end of a range",
  },
  Hashed {
    expression: "H,30 0 * * *",
    seed: Some(3),
    expect: None,
    why: "but it may be one item of a list, which is what cronexpr does too",
  },
  Hashed {
    expression: "0 0 * * H",
    seed: Some(9),
    expect: None,
    why: "every field admits it, day-of-week included",
  },
];

#[test]
fn hashed_values_behave_as_the_dialect_declares() {
  for case in HASHED {
    let got = match case.seed {
      Some(seed) => Schedule::<crate::dialect::Cronexpr>::parse_with(case.expression, seed),
      None => Schedule::<crate::dialect::Cronexpr>::parse(case.expression),
    };
    match (case.expect, got) {
      (None, Ok(_)) => {}
      (None, Err(e)) => panic!(
        "{:?} was rejected with `{e}` ({})",
        case.expression, case.why
      ),
      (Some((kind, span, field)), Err(e)) => assert_eq!(
        (*e.kind(), (e.span().start(), e.span().end()), e.field()),
        (kind, span, field),
        "{:?} ({})",
        case.expression,
        case.why
      ),
      (Some(_), Ok(_)) => panic!("{:?} was accepted ({})", case.expression, case.why),
    }
  }
}

/// A dialect without `H` says so, wherever the `H` sits.
///
/// The behaviour change this pins is that `H` stopped being an unrecognised byte and
/// became a token that three dialects refuse by name — which is why the reference scanner
/// grew `Token::Hashed` in the same commit.
#[test]
fn a_dialect_without_hashed_values_names_the_refusal() {
  fn refused<D: Dialect>(expression: &str, span: (usize, usize)) {
    let error = Schedule::<D>::parse(expression).expect_err("no dialect here has `H`");
    assert_eq!(
      *error.kind(),
      ErrorKind::HashedValueNotSupported { dialect: D::NAME },
      "{} on {expression:?}",
      D::NAME
    );
    assert_eq!(
      (error.span().start(), error.span().end()),
      span,
      "{} on {expression:?}",
      D::NAME
    );
  }

  // Alone in a field, and trailing after a value: the two places the parser reaches it
  // by different routes.
  refused::<Vixie>("H 0 * * *", (0, 1));
  refused::<Vixie>("1H 0 * * *", (1, 2));
  refused::<Robfig>("0 H 0 * * *", (2, 3));
  refused::<Quartz>("0 H 0 ? * *", (2, 3));
}

/// A seed picks a value inside the field, and the same seed always picks the same one.
#[test]
fn a_hashed_value_lands_inside_its_own_field() {
  use crate::dialect::Cronexpr;

  for seed in [0u64, 1, 7, 42, 1_000, u64::MAX] {
    let schedule = Schedule::<Cronexpr>::parse_with("H H H H H", seed).unwrap();
    let calendar = schedule.calendar().unwrap();

    // Exactly one value per field, and it is in range. `admits_*` is false everywhere
    // else, which is what makes `H` a restriction rather than a wildcard.
    assert_eq!(
      (0..=59u8).filter(|&m| calendar.admits_minute(m)).count(),
      1,
      "seed {seed}"
    );
    assert_eq!(
      (0..=23u8).filter(|&h| calendar.admits_hour(h)).count(),
      1,
      "seed {seed}"
    );
    assert_eq!(
      (1..=31u8)
        .filter(|&d| calendar.admits_day_of_month(d))
        .count(),
      1,
      "seed {seed}"
    );
    assert_eq!(
      (1..=12u8).filter(|&m| calendar.admits_month(m)).count(),
      1,
      "seed {seed}"
    );
    assert!(calendar.day_of_month_restricted, "seed {seed}");
    assert!(!calendar.day_of_month_wildcard, "seed {seed}");

    // Deterministic: the whole point of a seed is that two hosts given the same one
    // agree, so re-parsing has to land in the same place.
    let again = Schedule::<Cronexpr>::parse_with("H H H H H", seed).unwrap();
    assert_eq!(schedule, again, "seed {seed}");
  }

  // Different seeds reach different minutes, or the hash is not spreading anything.
  let minute = |seed: u64| {
    let schedule = Schedule::<Cronexpr>::parse_with("H 0 * * *", seed).unwrap();
    (0..=59u8)
      .find(|&m| schedule.calendar().unwrap().admits_minute(m))
      .unwrap()
  };
  assert_eq!(minute(0), 0);
  assert_eq!(minute(61), 1, "the minute field folds through 60");
  assert_ne!(minute(0), minute(30));
}

/// `H` in the day-of-week field: seven days, seven buckets, one each.
///
/// The defect this pins folded the seed over the eight *digits* a `ZeroSunday`
/// day-of-week field is written with, rather than over the seven days those digits name.
/// `0` and `7` are both Sunday, so seeds congruent to either landed there and Sunday drew
/// twice the work of any other day — which is the opposite of what a fleet spreading jobs
/// with `H` asked for.
///
/// No single seed can show that: every seed returns a perfectly good weekday, and even
/// the whole residue class still yields exactly seven *distinct* days, because the
/// duplicate bucket lands on a day another bucket already reached. What separates the two
/// implementations is how *often* each day comes up, so the walk below counts.
#[test]
fn a_hashed_weekday_gives_every_day_the_same_share() {
  use crate::dialect::Cronexpr;

  /// The days a weekday field can name.
  const DAYS: u64 = 7;
  /// The digits a `ZeroSunday` field can be written with — one more than there are days,
  /// and the modulus the defect used. A window that is a multiple of both counts sees
  /// every residue of each.
  const DIGITS: u64 = 8;
  const WINDOW: u64 = DAYS * DIGITS;

  let chosen = |seed: u64| {
    let schedule = Schedule::<Cronexpr>::parse_with("0 0 * * H", seed).unwrap();
    let calendar = schedule.calendar().unwrap();
    let days: Vec<Weekday> = (0..DAYS as u8)
      .filter_map(Weekday::from_canonical)
      .filter(|&day| calendar.admits_weekday(day))
      .collect();
    assert_eq!(
      days.len(),
      1,
      "seed {seed} picked {days:?}, and `H` picks exactly one day"
    );
    days[0]
  };

  let mut share = [0usize; DAYS as usize];
  for seed in 0..WINDOW {
    share[chosen(seed).to_canonical() as usize] += 1;
  }

  assert_eq!(
    share.iter().filter(|&&count| count > 0).count(),
    DAYS as usize,
    "{WINDOW} seeds reached {share:?}, and every day has to be reachable"
  );
  assert!(
    share.iter().all(|&count| count as u64 == WINDOW / DAYS),
    "{WINDOW} seeds over seven days came out {share:?}: a day with more than its share \
     is a day two seeds collide on, which is the whole defect"
  );

  // The same property stated as a period rather than as a count, because the count is a
  // consequence of it: the seed folds through the number of days, so a seed and that
  // seed plus seven are the same day. Under the defect they are not — seed 8 wrapped to
  // the digit `0` and gave Sunday where seed 1 gives Monday.
  for seed in 0..WINDOW {
    assert_eq!(
      chosen(seed),
      chosen(seed % DAYS),
      "seed {seed} and seed {} disagree",
      seed % DAYS
    );
  }
}

/// The same property in every field at once: the seed folds through the values the field
/// has, not through the ways they can be written.
///
/// Day-of-week is the only field where the two counts differ today, so it is the only
/// field where this can fail — but the statement is about every field, and it is written
/// that way so that a field that grows a second spelling for a value is covered on the
/// day it does rather than on the day someone remembers this test exists.
#[test]
fn a_hashed_value_folds_through_its_fields_value_count() {
  use crate::dialect::Cronexpr;

  /// Where `H` is written, how many values that field has, and how to read the one it
  /// picked back out.
  struct Field {
    expression: &'static str,
    values: u64,
    chosen: fn(&Calendar<Cronexpr>) -> u64,
  }

  fn single(count: u64, admits: impl Fn(u64) -> bool, first: u64) -> u64 {
    let found: Vec<u64> = (first..first + count).filter(|&v| admits(v)).collect();
    assert_eq!(found.len(), 1, "`H` picks exactly one value: {found:?}");
    found[0]
  }

  let fields = [
    Field {
      expression: "H * * * *",
      values: 60,
      chosen: |c| single(60, |v| c.admits_minute(v as u8), 0),
    },
    Field {
      expression: "* H * * *",
      values: 24,
      chosen: |c| single(24, |v| c.admits_hour(v as u8), 0),
    },
    Field {
      expression: "* * H * *",
      values: 31,
      chosen: |c| single(31, |v| c.admits_day_of_month(v as u8), 1),
    },
    Field {
      expression: "* * * H *",
      values: 12,
      chosen: |c| single(12, |v| c.admits_month(v as u8), 1),
    },
    Field {
      expression: "* * * * H",
      values: 7,
      chosen: |c| {
        single(
          7,
          |v| Weekday::from_canonical(v as u8).is_some_and(|day| c.admits_weekday(day)),
          0,
        )
      },
    },
  ];

  for field in fields {
    let pick = |seed: u64| {
      let schedule = Schedule::<Cronexpr>::parse_with(field.expression, seed).unwrap();
      (field.chosen)(schedule.calendar().unwrap())
    };

    // One full period reaches every value exactly once, so the choice is a bijection on
    // the residues rather than a map that crowds two of them onto one value.
    let mut reached: Vec<u64> = (0..field.values).map(pick).collect();
    reached.sort_unstable();
    reached.dedup();
    assert_eq!(
      reached.len() as u64,
      field.values,
      "{:?}: {} seeds reached {} distinct values",
      field.expression,
      field.values,
      reached.len()
    );

    // And the period is that count, checked one period past it.
    for seed in field.values..field.values * 2 {
      assert_eq!(
        pick(seed),
        pick(seed % field.values),
        "{:?}: seed {seed} left the period",
        field.expression
      );
    }
  }
}

/// The union rule's second half: `*,10` and `10,*` are one set written two ways.
///
/// This is the case that tells the wildcard witness from the restriction flag, and it is
/// the reason the witness is kept at all. Both fields denote every day of the month, so
/// `day_of_month_restricted` returns the same answer for each — a rule applied through it
/// cannot tell them apart, and gets `*,10` wrong. Under `WildcardWitness::LeadingStar`
/// the witness is the question Vixie actually asks, and it separates them.
///
/// The shared answer is now `false` rather than `true`, because a union holding every day
/// restricts nothing however it is written. The equality is what this test is about and
/// it is unmoved: two spellings of one set cannot carry a rule that separates them.
///
/// Computing the witness from the restriction flag — the bug this closes — fails this
/// test on the witness assertions while leaving every other test in the crate green.
#[test]
fn the_union_rule_reads_the_text_not_the_set() {
  let star_first = Schedule::<Vixie>::parse("0 0 *,10 * MON").unwrap();
  let star_last = Schedule::<Vixie>::parse("0 0 10,* * MON").unwrap();
  let (star_first, star_last) = (
    star_first.calendar().unwrap(),
    star_last.calendar().unwrap(),
  );

  // The two expressions denote the same set of days, so every set-shaped question about
  // them has the same answer. That is what makes the restriction flag unable to decide
  // the rule.
  for day in 1..=31 {
    assert_eq!(
      star_first.admits_day_of_month(day),
      star_last.admits_day_of_month(day),
      "`*,10` and `10,*` disagree about day {day}, so they are not the same set"
    );
  }
  assert_eq!(
    star_first.day_of_month_restricted, star_last.day_of_month_restricted,
    "the restriction flag is the same for both, which is exactly why it cannot \
     carry Vixie's rule"
  );
  assert!(
    !star_first.day_of_month_restricted,
    "a union containing `*` is every day, so neither spelling restricts anything"
  );

  // The text differs, and that is the whole rule.
  assert!(
    star_first.day_of_month_wildcard,
    "`*,10` begins with a star"
  );
  assert!(
    !star_last.day_of_month_wildcard,
    "`10,*` does not begin with a star, however much it means the same set"
  );

  // Applied: with the day-of-week field restricted and not starting with a star, the
  // first expression intersects — Mondays only — and the second unions, which admits
  // every day. Two schedules, one set of days, opposite behaviour.
  for calendar in [star_first, star_last] {
    assert!(!calendar.day_of_week_wildcard);
    assert!(calendar.day_of_week_restricted);
  }
  assert!(intersects(star_first), "`*,10 * MON` fires only on Mondays");
  assert!(!intersects(star_last), "`10,* * MON` fires every day");
}

/// The union rule's "and" half, as the crate computes it.
fn intersects<D: Dialect>(calendar: &Calendar<D>) -> bool {
  calendar.day_of_month_wildcard || calendar.day_of_week_wildcard
}

/// The plain star still behaves, and so does the plain restriction.
///
/// Guards the witness against the opposite error: reporting the text faithfully but for
/// the wrong field, or reporting it only where a list forced the question.
#[test]
fn the_star_position_is_reported_for_each_day_field_separately() {
  let cases: &[(&str, bool, bool)] = &[
    ("0 0 * * *", true, true),
    ("0 0 1 * MON", false, false),
    ("0 0 * * MON", true, false),
    ("0 0 1 * *", false, true),
    ("0 0 */2 * MON", true, false),
    ("0 0 1 * *,3", false, true),
    ("0 0 1 * 3,*", false, false),
  ];
  for &(expression, dom, dow) in cases {
    let schedule = Schedule::<Vixie>::parse(expression).unwrap();
    let calendar = schedule.calendar().unwrap();
    assert_eq!(
      (
        calendar.day_of_month_wildcard,
        calendar.day_of_week_wildcard
      ),
      (dom, dow),
      "{expression:?}"
    );
  }
}

/// The same seven shapes under the dialect that answers three of them the other way.
///
/// `Robfig` carries the witness as robfig's `starBit` does — OR'd across the items of a
/// list, and cleared by a stride above one — so `10,*` is a witness where Vixie says it
/// is not and `*/2` is not one where Vixie says it is. Two dialects, one field parser,
/// opposite answers: the witness is a per-item fact folded the way the dialect asks, and
/// this is the case that would fail if it were a syntactic constant again.
///
/// `?` is the fourth shape and only this dialect has one. Robfig reads it as another
/// spelling of `*`, `extra = starBit` and all, so a bare `?` in the day-of-month field
/// makes `0 0 0 ? * MON` fire on Mondays rather than on every day.
#[test]
fn the_go_dialect_witnesses_a_wildcard_wherever_it_sits() {
  let cases: &[(&str, bool, bool)] = &[
    ("0 0 0 * * *", true, true),
    ("0 0 0 1 * MON", false, false),
    ("0 0 0 * * MON", true, false),
    ("0 0 0 1 * *", false, true),
    // The three the census measured, each the other way round from Vixie's column.
    ("0 0 0 */2 * MON", false, false),
    ("0 0 0 10,* * MON", true, false),
    ("0 0 0 ? * MON", true, false),
    // And the list forms, which robfig ORs whichever end the star sits at.
    ("0 0 0 1 * *,3", false, true),
    ("0 0 0 1 * 3,*", false, true),
    ("0 0 0 ?,1 * MON", true, false),
  ];
  for &(expression, dom, dow) in cases {
    let schedule = Schedule::<Robfig>::parse(expression).unwrap();
    let calendar = schedule.calendar().unwrap();
    assert_eq!(
      (
        calendar.day_of_month_wildcard,
        calendar.day_of_week_wildcard
      ),
      (dom, dow),
      "{expression:?}"
    );
  }
}

/// Quartz asks no such question, so nothing may answer one for it.
///
/// The witness lives inside `DomDowRule::Union` precisely so that a dialect whose rule is
/// `Exclusive` has none to state. What that means for a parsed field is that the flag
/// stays false however the field was written — including for the `*` and the `?` that a
/// Vixie or Robfig field would witness.
#[test]
fn an_exclusive_dialect_witnesses_no_wildcard_at_all() {
  for expression in [
    "0 0 0 * * ?",
    "0 0 0 ? * *",
    "0 0 0 ? * MON",
    "0 0 0 */2 * ?",
    "0 0 0 *,10 * ?",
  ] {
    let schedule = Schedule::<Quartz>::parse(expression).unwrap();
    let calendar = schedule.calendar().unwrap();
    assert_eq!(
      (
        calendar.day_of_month_wildcard,
        calendar.day_of_week_wildcard
      ),
      (false, false),
      "{expression:?}"
    );
  }
}

// ---------------------------------------------------------------------------
// The matcher, in the cases a differential cannot reach.
//
// `tests/matcher_differential.rs` holds `Schedule::matches` against cronexpr, cron,
// croner and saffron over a corpus of expressions and a year of instants. What it cannot
// hold is anything no upstream has: a variant that is not a set of instants, a dialect
// bound no upstream declares, and a predicate every upstream refuses.
// ---------------------------------------------------------------------------

/// A date, or a panic naming the caller's mistake rather than the crate's.
fn at(year: u16, month: u8, day: u8, hour: u8, minute: u8, second: u8) -> CivilDateTime {
  CivilDateTime::new(year, month, day, hour, minute, second).expect("a real date")
}

/// Midnight on a date.
fn on(year: u16, month: u8, day: u8) -> CivilDateTime {
  at(year, month, day, 0, 0, 0)
}

#[test]
fn the_two_variants_that_are_not_a_set_of_instants_never_match() {
  // `@every` denotes a length of time. Without an anchor it has no instants at all, and
  // this crate has no anchor to offer; `@reboot` needs a process lifetime, which its own
  // documentation says. Answering `false` is not a limitation discovered at runtime, it
  // is what these two variants mean.
  let every = Schedule::<Robfig>::parse("@every 1h").unwrap();
  let reboot = Schedule::<Vixie>::parse("@reboot").unwrap();
  for when in [on(2026, 8, 12), at(2026, 8, 12, 1, 0, 0), on(1970, 1, 1)] {
    assert!(!every.matches(when), "@every at {when}");
    assert!(!reboot.matches(when), "@reboot at {when}");
  }
}

#[test]
fn a_five_field_expression_fires_only_at_second_zero() {
  // The dialect has no seconds field, so the parser pins one. Two of the four upstreams
  // ignore the seconds component of the instant they are given, so this is not a case a
  // differential against them could make.
  let schedule = Schedule::<Vixie>::parse("0 0 * * *").unwrap();
  assert!(schedule.matches(at(2026, 8, 12, 0, 0, 0)));
  for second in [1u8, 30, 59] {
    assert!(
      !schedule.matches(at(2026, 8, 12, 0, 0, second)),
      "second {second}"
    );
  }
}

#[test]
fn the_exclusive_rule_reads_the_field_that_is_not_a_question_mark() {
  // Quartz refuses two restricted day fields, so one of them is always a `?` — which
  // admits every value. "Both must match" is therefore what reading the other one alone
  // amounts to, and this is the pair that would fail if it were a union instead.
  let weekdays = Schedule::<Quartz>::parse("0 0 0 ? * MON").unwrap();
  assert!(weekdays.matches(on(2026, 8, 10)), "a Monday");
  assert!(weekdays.matches(on(2026, 8, 3)), "also a Monday");
  assert!(!weekdays.matches(on(2026, 8, 11)), "a Tuesday");

  let first = Schedule::<Quartz>::parse("0 0 0 1 * ?").unwrap();
  assert!(first.matches(on(2026, 8, 1)));
  assert!(
    !first.matches(on(2026, 8, 10)),
    "a Monday, but not the first"
  );
}

#[test]
fn a_date_predicate_answers_for_its_whole_field() {
  // A predicate is the whole field and writes no bits, so a matcher that read the bitset
  // would answer "no day at all" — which is what the day-of-month accessor does on its
  // own, and why it is not on the front door. Every one of these is a Quartz predicate
  // and `L-n` is refused by every upstream in the differential, so these are contract
  // cases rather than differential ones.
  /// An expression, the dates it must fire on, and the dates it must not.
  type Case = (&'static str, &'static [Ymd], &'static [Ymd]);
  /// A year, a month and a day of that month.
  type Ymd = (u16, u8, u8);

  let cases: &[Case] = &[
    // `L`: the last day, whatever the month's length.
    (
      "0 0 0 L * ?",
      &[(2026, 3, 31), (2026, 2, 28), (2024, 2, 29), (2026, 12, 31)],
      &[(2026, 3, 30), (2024, 2, 28), (2026, 4, 29)],
    ),
    // `L-3`: three days before it.
    (
      "0 0 0 L-3 * ?",
      &[(2026, 3, 28), (2026, 2, 25)],
      &[(2026, 3, 31), (2026, 3, 27)],
    ),
    // `LW`: the last Monday-to-Friday. May 2026 ends on a Sunday, so it is the 29th.
    (
      "0 0 0 LW * ?",
      &[(2026, 5, 29), (2026, 3, 31)],
      &[(2026, 5, 31), (2026, 5, 30)],
    ),
    // `15W`: the weekday nearest the 15th. In March 2026 the 15th is a Sunday.
    (
      "0 0 0 15W * ?",
      &[(2026, 3, 16), (2026, 4, 15)],
      &[(2026, 3, 15), (2026, 3, 13)],
    ),
    // `6#3`: the third Friday.
    (
      "0 0 0 ? * 6#3",
      &[(2026, 8, 21), (2026, 9, 18)],
      &[(2026, 8, 14), (2026, 8, 28)],
    ),
    // `6L`: the last Friday.
    (
      "0 0 0 ? * 6L",
      &[(2026, 8, 28), (2026, 9, 25)],
      &[(2026, 8, 21), (2026, 8, 27)],
    ),
  ];

  for &(expression, fires, does_not) in cases {
    let schedule = Schedule::<Quartz>::parse(expression).unwrap();
    for &(year, month, day) in fires {
      assert!(
        schedule.matches(on(year, month, day)),
        "{expression} must fire on {year}-{month}-{day}"
      );
    }
    for &(year, month, day) in does_not {
      assert!(
        !schedule.matches(on(year, month, day)),
        "{expression} must not fire on {year}-{month}-{day}"
      );
    }
  }
}

#[test]
fn a_predicate_and_a_weekday_still_take_the_union_rule() {
  // The two halves of the day decision are a predicate and a bitset here, and neither
  // day field carries a wildcard, so Vixie's rule unions them. Reading the day-of-month
  // bitset instead of the predicate would make this fire on Mondays only — which is the
  // census's row 8, one level up.
  let schedule = Schedule::<Cronexpr>::parse("0 0 L * MON").unwrap();
  assert!(schedule.matches(on(2026, 3, 31)), "the last day of March");
  assert!(schedule.matches(on(2026, 8, 10)), "a Monday");
  assert!(!schedule.matches(on(2026, 8, 11)), "neither");
}

#[test]
fn the_dialects_year_bound_reaches_the_matcher() {
  // Quartz declares `1970..=2099` and refuses an explicit 2100, so a Quartz schedule
  // cannot fire in 2100 merely because its year field was left as `*`. No upstream
  // declares this bound, so no differential could check it.
  let star = Schedule::<Quartz>::parse("0 0 0 1 1 ? *").unwrap();
  assert!(star.matches(on(2099, 1, 1)));
  assert!(!star.matches(on(2100, 1, 1)));

  let listed = Schedule::<Quartz>::parse("0 0 0 1 1 ? 2026-2028").unwrap();
  assert!(listed.matches(on(2026, 1, 1)));
  assert!(listed.matches(on(2028, 1, 1)));
  assert!(!listed.matches(on(2025, 1, 1)));
  assert!(!listed.matches(on(2029, 1, 1)));

  // A dialect with no year field is the unbounded case.
  let vixie = Schedule::<Vixie>::parse("0 0 1 1 *").unwrap();
  assert!(vixie.matches(on(2100, 1, 1)));
  assert!(vixie.matches(on(1, 1, 1)));
}

/// Every expression the dialect table exercises.
///
/// Exposed to the crate so that the lexer's differential oracle can scan the whole
/// parser corpus rather than a copy of it, which would drift the moment a row is added
/// here and not there.
pub(crate) fn corpus() -> impl Iterator<Item = &'static str> {
  TABLE.iter().map(|row| row.expression)
}
