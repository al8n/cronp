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
  assert_eq!(schedule.timezone(), Some("Asia/Shanghai"));

  // The schedule underneath is the same one the plain parser produces from the same
  // five fields, so retaining a timezone costs the expression nothing.
  let plain = crate::Schedule::<Cronexpr>::parse("0 4 * * *").unwrap();
  assert_eq!(schedule.schedule(), &plain);
}

#[test]
fn a_timezone_is_optional_even_where_the_dialect_takes_one() {
  let schedule = ZonedSchedule::<Cronexpr>::parse("0 4 * * *").unwrap();
  assert_eq!(schedule.timezone(), None);
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
    assert_eq!(schedule.timezone(), Some(name));
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
  assert_eq!(schedule.timezone(), Some("Asia/Shanghai"));
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
  assert_eq!(schedule.timezone(), Some("Asia/Shanghai"));

  let schedule = ZonedSchedule::<Cronexpr>::parse("  0 4 * * * UTC\n").unwrap();
  assert_eq!(schedule.timezone(), Some("UTC"));
}

#[test]
fn last_field_finds_the_last_run_of_non_whitespace() {
  assert_eq!(last_field("a bb ccc"), Some(5..8));
  assert_eq!(last_field("a bb ccc   "), Some(5..8));
  assert_eq!(last_field("solo"), Some(0..4));
  assert_eq!(last_field("   "), None);
  assert_eq!(last_field(""), None);
}

#[test]
fn the_shape_check_admits_iana_and_nothing_else() {
  assert!(!is_timezone_name(""));
  assert!(!is_timezone_name("*"));
  assert!(!is_timezone_name("Asia/Shang hai"));
  assert!(!is_timezone_name("Asia/Shanghai!"));
  assert!(is_timezone_name("Etc/GMT-14"));
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
