#![allow(
  clippy::indexing_slicing,
  clippy::unwrap_used,
  clippy::expect_used,
  clippy::panic
)]

use super::{ZonedSchedule, is_timezone_name, last_field};
use crate::{
  dialect::{Cronexpr, Quartz, Robfig, Vixie},
  error::{ErrorKind, FieldKind},
};

// ---------------------------------------------------------------------------
// The parsing tier, which every build has.
// ---------------------------------------------------------------------------

#[test]
fn the_name_is_retained_exactly_as_written() {
  let schedule = ZonedSchedule::<Cronexpr>::parse("0 4 * * * Asia/Shanghai").unwrap();
  assert_eq!(schedule.timezone_name(), Some("Asia/Shanghai"));

  // The schedule underneath is the same one the plain parser produces from the same
  // five fields, so retaining a timezone costs the expression nothing.
  let plain = crate::Schedule::<Cronexpr>::parse("0 4 * * *").unwrap();
  assert_eq!(schedule.schedule(), &plain);
}

#[test]
fn a_timezone_is_optional_even_where_the_dialect_takes_one() {
  let schedule = ZonedSchedule::<Cronexpr>::parse("0 4 * * *").unwrap();
  assert_eq!(schedule.timezone_name(), None);
  assert_eq!(
    schedule.schedule(),
    &crate::Schedule::<Cronexpr>::parse("0 4 * * *").unwrap()
  );
}

#[test]
fn the_names_iana_actually_uses_all_parse() {
  for name in [
    "UTC",
    "Asia/Shanghai",
    "America/Argentina/Buenos_Aires",
    "Etc/GMT+5",
    "America/Port-au-Prince",
  ] {
    let expression = std::format!("0 4 * * * {name}");
    let schedule = ZonedSchedule::<Cronexpr>::parse(&expression)
      .unwrap_or_else(|e| panic!("{name} was rejected: {e}"));
    assert_eq!(schedule.timezone_name(), Some(name));
    assert!(is_timezone_name(name));
  }
}

#[test]
fn a_dialect_that_takes_no_timezone_says_so() {
  // The type is the mistake here, not the text: these dialects have no timezone in their
  // grammar at all, so the refusal names the dialect rather than the trailing field.
  fn refused<D: crate::Dialect>() {
    let error = ZonedSchedule::<D>::parse("0 4 * * * Asia/Shanghai")
      .expect_err("this dialect has no timezone");
    assert_eq!(
      *error.kind(),
      ErrorKind::TimezoneNotSupported { dialect: D::NAME }
    );
  }
  refused::<Vixie>();
  refused::<Quartz>();
  refused::<Robfig>();

  // Even without a timezone written, because the type says one was expected.
  assert_eq!(
    *ZonedSchedule::<Vixie>::parse("0 4 * * *")
      .expect_err("Vixie has no timezone")
      .kind(),
    ErrorKind::TimezoneNotSupported { dialect: "Vixie" }
  );
}

#[test]
fn a_trailing_field_that_cannot_be_a_timezone_is_named() {
  let error = ZonedSchedule::<Cronexpr>::parse("0 4 * * * *").expect_err("`*` is not a timezone");
  assert_eq!(*error.kind(), ErrorKind::MalformedTimezone);
  assert_eq!((error.span().start(), error.span().end()), (10, 11));

  let error =
    ZonedSchedule::<Cronexpr>::parse("0 4 * * * Asia/Shang!hai").expect_err("`!` is not IANA");
  assert_eq!(*error.kind(), ErrorKind::MalformedTimezone);
}

#[test]
fn a_fault_in_the_expression_outranks_a_malformed_timezone() {
  // Two faults at once, and the contract says which one a caller hears about: the first
  // thing wrong, exactly as `Schedule::parse` reports it. The minute is wrong at byte 0
  // and the trailing field is not a timezone at byte 11, so the minute is the answer —
  // checking the suffix first sent a caller off to fix a timezone while the expression
  // it belongs to was still invalid.
  let error =
    ZonedSchedule::<Cronexpr>::parse("99 0 * * * @").expect_err("99 is not a minute either way");
  assert_eq!(
    *error.kind(),
    ErrorKind::ValueOutOfRange {
      value: 99,
      min: 0,
      max: 59,
    }
  );
  assert_eq!(error.field(), Some(FieldKind::Minute));
  assert_eq!((error.span().start(), error.span().end()), (0, 2));

  // And the same expression with its prefix repaired is where `MalformedTimezone` lives:
  // the suffix check is not weakened, it is sequenced.
  assert_eq!(
    *ZonedSchedule::<Cronexpr>::parse("0 0 * * * @")
      .expect_err("`@` is not a timezone")
      .kind(),
    ErrorKind::MalformedTimezone
  );

  // The prefix is reported by the same rule whatever kind its fault is, so this does not
  // hold for `ValueOutOfRange` alone.
  assert_eq!(
    *ZonedSchedule::<Cronexpr>::parse("* * * * JAN @")
      .expect_err("a month is not a weekday")
      .kind(),
    ErrorKind::UnknownName
  );
  assert_eq!(
    *ZonedSchedule::<Cronexpr>::parse("H * * * * @")
      .expect_err("no seed")
      .kind(),
    ErrorKind::HashedValueNeedsSeed
  );
}

#[test]
fn a_wrong_field_count_is_still_the_schedule_parsers_to_report() {
  // Seven fields is not "five plus a timezone plus something"; splitting one off would
  // report whatever the leftovers looked like instead of the count that is actually
  // wrong.
  let error = ZonedSchedule::<Cronexpr>::parse("0 4 * * * * * Asia/Shanghai")
    .expect_err("far too many fields");
  assert_eq!(
    *error.kind(),
    ErrorKind::WrongFieldCount {
      found: 8,
      min: 5,
      max: 5,
      dialect: "Cronexpr",
    }
  );
}

#[test]
fn spans_still_point_into_the_whole_expression() {
  // The schedule is parsed from a prefix slice, so an offset inside it has to be the
  // offset in the original text and not in the slice.
  let error =
    ZonedSchedule::<Cronexpr>::parse("0 99 * * * Asia/Shanghai").expect_err("99 is not an hour");
  assert_eq!(error.field(), Some(FieldKind::Hour));
  assert_eq!((error.span().start(), error.span().end()), (2, 4));
  assert_eq!(&"0 99 * * * Asia/Shanghai"[2..4], "99");
}

#[test]
fn a_seed_reaches_through_the_zoned_entry_point_too() {
  let schedule = ZonedSchedule::<Cronexpr>::parse_with("H 4 * * * Asia/Shanghai", 30).unwrap();
  assert_eq!(schedule.timezone_name(), Some("Asia/Shanghai"));
  assert!(schedule.schedule().calendar().unwrap().admits_minute(30));

  assert_eq!(
    *ZonedSchedule::<Cronexpr>::parse("H 4 * * * Asia/Shanghai")
      .expect_err("no seed")
      .kind(),
    ErrorKind::HashedValueNeedsSeed
  );
}

#[test]
fn trailing_whitespace_does_not_become_part_of_the_name() {
  let schedule = ZonedSchedule::<Cronexpr>::parse("0 4 * * * Asia/Shanghai   ").unwrap();
  assert_eq!(schedule.timezone_name(), Some("Asia/Shanghai"));

  let schedule = ZonedSchedule::<Cronexpr>::parse("  0 4 * * * UTC\n").unwrap();
  assert_eq!(schedule.timezone_name(), Some("UTC"));
}

#[test]
fn last_field_finds_the_last_run_of_non_whitespace() {
  assert_eq!(last_field("a bb ccc"), Some(5..8));
  assert_eq!(last_field("a bb ccc   "), Some(5..8));
  assert_eq!(last_field("solo"), Some(0..4));
  assert_eq!(last_field("   "), None);
  assert_eq!(last_field(""), None);
}

/// What the parsing tier does with a run in the timezone position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Decided {
  /// Could be no zone under any database, so it is refused as `MalformedTimezone` over
  /// its own bytes.
  Refused,
  /// Has the shape of a zone name, so it is kept verbatim and never resolved here.
  Retained,
}

/// Every shape a sixth field can take, and what this tier does with each.
///
/// The gate hands [`is_timezone_name`] whatever run sits one field past the dialect's
/// maximum, so the question "what else can that run be" is the whole of the check's
/// design. It used to be answered by a character allowlist, and digits were in it: `2025`
/// was retained as a timezone, and so were `1-5`, `1/5`, `2025-2030`, every run of bare
/// separators, and the three ways to write an empty component.
///
/// The two `Retained` rows at the bottom are the boundary this tier cannot cross rather
/// than a gap in it. `MON` and `MON-FRI` have the shape of `Cuba` and `W-SU`; separating
/// them is a database lookup, which is the next tier up.
const SIXTH_FIELD_SHAPES: &[(&str, Decided, &str)] = &[
  // The stray cron field the split exists to catch, in each of its spellings.
  (
    "2025",
    Decided::Refused,
    "a year: no component starts with a digit",
  ),
  ("2025-2030", Decided::Refused, "a range of years"),
  ("1-5", Decided::Refused, "a range"),
  ("1/5", Decided::Refused, "a step"),
  ("15W", Decided::Refused, "a nearest-weekday predicate"),
  (
    "6#3",
    Decided::Refused,
    "an nth-weekday predicate: `#` is in no identifier",
  ),
  // Cron punctuation that no identifier has.
  ("*", Decided::Refused, "a wildcard"),
  ("*/5", Decided::Refused, "a stepped wildcard"),
  ("1,2", Decided::Refused, "a list: `,` is in no identifier"),
  ("?", Decided::Refused, "a question mark"),
  (
    "@daily",
    Decided::Refused,
    "a nickname: `@` is in no identifier",
  ),
  (
    "Asia/Shanghai!",
    Decided::Refused,
    "a byte no identifier has",
  ),
  ("Asia/Shàng", Decided::Refused, "a non-ASCII byte"),
  // Runs that are only separators. Every component of these is empty or starts with one.
  ("-", Decided::Refused, "one separator"),
  ("+", Decided::Refused, "one separator"),
  ("_", Decided::Refused, "one separator"),
  (
    ".",
    Decided::Refused,
    "the current-directory shape `zic` forbids",
  ),
  (
    "..",
    Decided::Refused,
    "the parent-directory shape `zic` forbids",
  ),
  (
    "///",
    Decided::Refused,
    "only separators, so every component is empty",
  ),
  // An empty component, which no database can hold.
  ("/Asia/Shanghai", Decided::Refused, "a leading `/`"),
  ("Asia//Shanghai", Decided::Refused, "a doubled `/`"),
  ("Asia/Shanghai/", Decided::Refused, "a trailing `/`"),
  // Names IANA defines, which is what the check exists to let through.
  ("UTC", Decided::Retained, "a one-component name"),
  (
    "Asia/Shanghai",
    Decided::Retained,
    "the ordinary two-component name",
  ),
  (
    "America/Argentina/Buenos_Aires",
    Decided::Retained,
    "three components and a `_`",
  ),
  ("Etc/GMT+5", Decided::Retained, "a `+`"),
  (
    "America/Port-au-Prince",
    Decided::Retained,
    "hyphens inside a component",
  ),
  ("Etc/GMT-14", Decided::Retained, "a `-` before digits"),
  ("EST5EDT", Decided::Retained, "digits inside a component"),
  ("W-SU", Decided::Retained, "a one-letter first component"),
  // Well shaped and defined by nothing, which is the tier boundary rather than a gap.
  (
    "Mars/Olympus_Mons",
    Decided::Retained,
    "well shaped, and no database has it",
  ),
  (
    "MON",
    Decided::Retained,
    "a weekday name, shaped exactly like `Cuba`",
  ),
  (
    "MON-FRI",
    Decided::Retained,
    "a weekday range, shaped exactly like `W-SU`",
  ),
  (
    "L",
    Decided::Retained,
    "a bare predicate, shaped like a one-letter component",
  ),
];

#[test]
fn the_shapes_a_sixth_field_can_take_are_each_decided() {
  for &(run, decided, why) in SIXTH_FIELD_SHAPES {
    assert_eq!(
      is_timezone_name(run),
      decided == Decided::Retained,
      "{run:?} ({why}) is decided {decided:?}, and the shape check disagrees"
    );

    // And end to end, because the shape check is only half of the decision: the run has
    // to reach it, and a refusal has to come back over the run's own bytes.
    let expression = std::format!("0 0 * * * {run}");
    match (decided, ZonedSchedule::<Cronexpr>::parse(&expression)) {
      (Decided::Retained, Ok(schedule)) => assert_eq!(schedule.timezone_name(), Some(run)),
      (Decided::Refused, Err(error)) => {
        assert_eq!(
          *error.kind(),
          ErrorKind::MalformedTimezone,
          "{expression:?}"
        );
        assert_eq!(
          (error.span().start(), error.span().end()),
          (10, 10 + run.len()),
          "{expression:?}: the refusal points at {:?} rather than at the run",
          expression.get(error.span().start()..error.span().end()),
        );
      }
      (_, outcome) => panic!("{expression:?} ({why}) is {decided:?}, and parsed as {outcome:?}"),
    }
  }

  // The one shape the table cannot express, because it is not a run: `count_fields`
  // counts runs of non-whitespace, so the field the gate splits off always has a byte in
  // it. The check answers anyway rather than relying on that.
  assert!(!is_timezone_name(""));

  // A run with a space in it is two runs, so it never reaches the check at all — the
  // count is what answers, and that is the row above this one in the census.
  assert_eq!(
    *ZonedSchedule::<Cronexpr>::parse("0 0 * * * Asia/Shang hai")
      .expect_err("seven runs")
      .kind(),
    ErrorKind::WrongFieldCount {
      found: 7,
      min: 5,
      max: 5,
      dialect: "Cronexpr",
    }
  );
}

/// A well-shaped name that no database defines reaches the caller, and the default tier
/// has its own way to refuse it.
///
/// The line the parsing tier draws, stated as behaviour rather than as prose. `is` and
/// `could be` are different questions and this tier answers only the second — so the
/// answer to the first has to be reachable *here*, not only two feature flags away. It
/// was not: the earlier design said "retained now, refused by whichever tier resolves",
/// and the default tier resolves at no tier, so a typo'd or zone-shaped cron field was
/// accepted with nothing anywhere to say otherwise.
#[test]
fn a_well_shaped_name_that_does_not_exist_is_retained_and_refusable_here() {
  const ACCEPTED: &[&str] = &["Asia/Shanghai", "UTC"];

  // Shape says yes, so the parse succeeds and the name comes back exactly as written.
  let schedule = ZonedSchedule::<Cronexpr>::parse("0 4 * * * Mars/Olympus_Mons")
    .expect("the parsing tier judges shape, not existence");
  assert_eq!(schedule.timezone_name(), Some("Mars/Olympus_Mons"));

  // And the default tier can still refuse it, against the only database it has: the
  // caller's. This is the half that did not exist.
  let refused = schedule
    .validate_in(ACCEPTED)
    .expect_err("no caller accepts a zone on Mars");
  assert_eq!(refused.name(), "Mars/Olympus_Mons");

  // Including the residue the shape check is documented as keeping: a weekday range has
  // the shape of `W-SU`, and only a database tells them apart.
  let cron_field = ZonedSchedule::<Cronexpr>::parse("0 4 * * * MON-FRI").expect("well shaped");
  assert_eq!(
    cron_field
      .validate_in(ACCEPTED)
      .expect_err("not a zone anyone accepts")
      .name(),
    "MON-FRI"
  );

  // A name the caller does accept passes through, and an expression with no timezone has
  // nothing to check.
  let good = ZonedSchedule::<Cronexpr>::parse("0 4 * * * UTC").expect("well shaped");
  assert_eq!(good.validate_in(ACCEPTED), Ok(Some("UTC")));
  let bare = ZonedSchedule::<Cronexpr>::parse("0 4 * * *").expect("no timezone");
  assert_eq!(bare.validate_in(ACCEPTED), Ok(None));
  assert_eq!(bare.validate_in(&[]), Ok(None));
}

/// The default tier's refusal is the same type the `tz-static` tier's is.
///
/// Not a coincidence to preserve by hand: `UnknownTimeZone` is ungated precisely so that
/// turning a feature on changes which database answers and not how a caller writes the
/// arm that handles a name it does not know.
#[test]
fn the_two_table_tiers_refuse_with_one_type() {
  let schedule = ZonedSchedule::<Cronexpr>::parse("0 4 * * * Europe/Zurich").unwrap();
  let from_parse: crate::UnknownTimeZone<'_> = schedule.validate_in(&["UTC"]).unwrap_err();
  assert_eq!(from_parse.name(), "Europe/Zurich");

  #[cfg(feature = "tz-static")]
  {
    let from_static: crate::UnknownTimeZone<'_> = schedule.resolve_in(&[]).unwrap_err();
    assert_eq!(from_static, from_parse);
  }
}

#[test]
fn into_parts_gives_both_halves_up() {
  let (schedule, zone) = ZonedSchedule::<Cronexpr>::parse("0 4 * * * UTC")
    .unwrap()
    .into_parts();
  assert_eq!(zone, Some("UTC"));
  assert_eq!(
    schedule,
    crate::Schedule::<Cronexpr>::parse("0 4 * * *").unwrap()
  );
}

// ---------------------------------------------------------------------------
// The `tz-static` tier.
//
// What makes this tier a tier, and not just a different spelling of `tz`, is that it
// resolves exactly what the application registered and refuses everything else. The
// second assertion below is the one a `tz` build cannot satisfy: at that tier
// `Europe/Zurich` is in the database whether anybody asked for it or not.
// ---------------------------------------------------------------------------

#[cfg(feature = "tz-static")]
mod tz_static {
  use super::*;
  use jiff::tz::{self, TimeZone};

  /// The application's table: two zones compiled in, and no database behind them.
  static ZONES: &[(&str, TimeZone)] = &[
    ("Asia/Shanghai", tz::get!("Asia/Shanghai")),
    ("UTC", tz::get!("UTC")),
  ];

  #[test]
  fn a_registered_name_resolves() {
    let schedule = ZonedSchedule::<Cronexpr>::parse("0 4 * * * Asia/Shanghai").unwrap();
    let zone = schedule.resolve_in(ZONES).unwrap().unwrap();
    assert_eq!(zone.iana_name(), Some("Asia/Shanghai"));
  }

  #[test]
  fn an_unregistered_name_does_not() {
    // `Europe/Zurich` is a perfectly real IANA zone. At the `tz` tier it resolves; here
    // it does not, because nobody compiled it in. That difference is the tier.
    let schedule = ZonedSchedule::<Cronexpr>::parse("0 4 * * * Europe/Zurich").unwrap();
    let error = schedule
      .resolve_in(ZONES)
      .expect_err("Europe/Zurich was never registered");
    assert_eq!(error.name(), "Europe/Zurich");
  }

  #[test]
  fn no_timezone_resolves_to_nothing_rather_than_failing() {
    let schedule = ZonedSchedule::<Cronexpr>::parse("0 4 * * *").unwrap();
    assert!(schedule.resolve_in(ZONES).unwrap().is_none());
  }
}

// ---------------------------------------------------------------------------
// The `tz` tier.
//
// The converse: any IANA name resolves, with nothing registered anywhere. A
// `tz-static`-only build has no `resolve` to call, and could not answer this if it did.
// ---------------------------------------------------------------------------

#[cfg(feature = "tz")]
mod tz_runtime {
  use super::*;

  #[test]
  fn any_iana_name_resolves_with_nothing_registered() {
    for name in ["Asia/Shanghai", "Europe/Zurich", "America/New_York", "UTC"] {
      let expression = std::format!("0 4 * * * {name}");
      let schedule = ZonedSchedule::<Cronexpr>::parse(&expression).unwrap();
      let zone = schedule
        .resolve()
        .unwrap_or_else(|e| panic!("{name} did not resolve: {e}"))
        .expect("the expression named one");
      assert_eq!(zone.iana_name(), Some(name));
    }
  }

  #[test]
  fn a_name_no_database_knows_is_jiffs_error() {
    let schedule = ZonedSchedule::<Cronexpr>::parse("0 4 * * * Mars/Olympus_Mons").unwrap();
    assert!(schedule.resolve().is_err());
  }

  #[test]
  fn no_timezone_resolves_to_nothing_rather_than_failing() {
    let schedule = ZonedSchedule::<Cronexpr>::parse("0 4 * * *").unwrap();
    assert!(schedule.resolve().unwrap().is_none());
  }
}
