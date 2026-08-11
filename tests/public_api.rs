//! The crate through its front door.
//!
//! Everything here goes through `cronp::` rather than through a module path, so a
//! re-export that stops being exported breaks this file rather than being noticed by
//! nobody. The unit tests reach inside; this one deliberately cannot.

use cronp::{
  Calendar, CivilDateTime, Cronexpr, DateComponent, DayOfMonthModifier, DayOfWeekModifier, Dialect,
  DomDowRule, EPOCH, ErrorKind, FieldKind, Quartz, QuestionMark, Robfig, Schedule, Span, Vixie,
  Weekday, WeekdayNumbering, YearField, Years, ZonedSchedule,
};

#[test]
fn the_three_dialects_are_reachable_and_declare_what_they_should() {
  assert_eq!(Vixie::NAME, "Vixie");
  assert_eq!(Quartz::NAME, "Quartz");
  assert_eq!(Robfig::NAME, "Robfig");

  assert_eq!(Vixie::DOM_DOW, DomDowRule::Union);
  assert_eq!(Quartz::DOM_DOW, DomDowRule::Exclusive);
  assert_eq!(Vixie::QUESTION_MARK, QuestionMark::Forbidden);
  assert_eq!(Vixie::WEEKDAY, WeekdayNumbering::ZeroSunday);
  assert_eq!(Quartz::WEEKDAY, WeekdayNumbering::OneSunday);
  assert_eq!(Vixie::YEAR, YearField::Absent);
}

#[test]
fn a_schedule_parses_and_can_be_asked_about_its_fields() {
  let schedule: Schedule<Vixie> = Schedule::parse("30 2 * * 1-5").expect("valid Vixie");
  let calendar: &Calendar<Vixie> = schedule.calendar().expect("a calendar");

  assert!(calendar.admits_minute(30));
  assert!(calendar.admits_hour(2));
  assert!(calendar.admits_weekday(Weekday::Monday));
  assert!(calendar.admits_weekday(Weekday::Friday));
  assert!(!calendar.admits_weekday(Weekday::Sunday));
  assert!(calendar.day_of_week_restricted());
  assert!(!calendar.day_of_month_restricted());
  assert_eq!(calendar.dom_dow_rule(), DomDowRule::Union);
}

#[test]
fn the_error_type_carries_a_kind_a_span_and_a_field() {
  let error = Schedule::<Vixie>::parse("0 0 * * 9").expect_err("9 is not a weekday");
  assert_eq!(
    *error.kind(),
    ErrorKind::ValueOutOfRange {
      value: 9,
      min: 0,
      max: 7
    }
  );
  assert_eq!(error.span(), Span::new(8, 9));
  assert_eq!(error.field(), Some(FieldKind::DayOfWeek));
  assert!(!format!("{error}").is_empty());

  // It is a `core::error::Error`, which needs no `std` feature.
  let as_error: &dyn core::error::Error = &error;
  assert!(!as_error.to_string().is_empty());
}

#[test]
fn the_predicates_are_reachable_and_evaluate_against_a_date() {
  let schedule = Schedule::<Quartz>::parse("0 0 12 LW * ?").expect("valid Quartz");
  let calendar = schedule.calendar().expect("a calendar");
  assert_eq!(
    calendar.day_of_month_modifier(),
    Some(DayOfMonthModifier::LastWeekday)
  );
  assert_eq!(calendar.day_of_week_modifier(), None);

  // April 2023 ends on a Sunday, so the last weekday is Friday the 28th.
  let friday = CivilDateTime::new(2023, 4, 28, 12, 0, 0).expect("a real date");
  let sunday = CivilDateTime::new(2023, 4, 30, 12, 0, 0).expect("a real date");
  assert_eq!(friday.weekday(), Weekday::Friday);
  assert!(DayOfMonthModifier::LastWeekday.matches(&friday));
  assert!(!DayOfMonthModifier::LastWeekday.matches(&sunday));

  let nth = Schedule::<Quartz>::parse("0 0 12 ? * 6#3").expect("valid Quartz");
  assert_eq!(
    nth.calendar().and_then(Calendar::day_of_week_modifier),
    Some(DayOfWeekModifier::Nth {
      weekday: Weekday::Friday,
      nth: 3
    })
  );
}

#[test]
fn the_date_type_rejects_a_date_that_does_not_exist() {
  let error = CivilDateTime::new(2023, 2, 29, 0, 0, 0).expect_err("2023 is not a leap year");
  assert_eq!(error.component(), DateComponent::Day);
  assert_eq!(error.max(), 28);
  assert!(CivilDateTime::new(2024, 2, 29, 0, 0, 0).is_ok());
}

#[test]
fn the_year_set_is_reachable_and_states_its_own_range() {
  assert_eq!(EPOCH, 1970);
  assert_eq!(Years::<1>::MAX, 2097);
  assert_eq!(Years::<2>::MAX, 2225);

  let mut years = Years::<1>::default();
  assert!(years.is_empty());
  years.insert(2025).expect("inside the range");
  assert!(years.contains(2025));
  assert_eq!(
    years.insert(2098),
    Err(ErrorKind::YearNotRepresentable {
      year: 2098,
      max_representable: 2097,
      required_n: 2,
    })
  );
}

#[test]
fn reboot_and_every_are_variants_a_caller_can_see() {
  assert_eq!(
    Schedule::<Vixie>::parse("@reboot").expect("legal Vixie"),
    Schedule::Reboot
  );
  assert_eq!(
    Schedule::<Robfig>::parse("@every 90m")
      .expect("legal Go")
      .every(),
    Some(core::time::Duration::from_secs(5400))
  );
}

#[test]
fn the_fourth_dialect_is_reachable_and_declares_its_two_constructs() {
  assert_eq!(Cronexpr::NAME, "Cronexpr");

  // Read into runtime values before asserting. An associated constant inside `assert!`
  // folds the whole assertion away, which checks the compiler rather than the dialect —
  // the same reason `dialect/tests.rs` reads every fact through a lookup.
  fn declared<D: Dialect>() -> (bool, bool) {
    (D::HASHED_VALUES, D::TIMEZONE)
  }
  assert_eq!(declared::<Cronexpr>(), (true, true));
  assert_eq!(declared::<Vixie>(), (false, false));
  assert_eq!(declared::<Quartz>(), (false, false));
  assert_eq!(declared::<Robfig>(), (false, false));
}

#[test]
fn the_star_position_accessors_are_reachable() {
  // The pair the Vixie union rule turns on, through the front door: same set, same
  // restriction flag, different answer.
  let star_first = Schedule::<Vixie>::parse("0 0 *,10 * MON").expect("legal Vixie");
  let star_last = Schedule::<Vixie>::parse("0 0 10,* * MON").expect("legal Vixie");
  let star_first: &Calendar<Vixie> = star_first.calendar().expect("a calendar");
  let star_last: &Calendar<Vixie> = star_last.calendar().expect("a calendar");

  assert_eq!(
    star_first.day_of_month_restricted(),
    star_last.day_of_month_restricted()
  );
  assert!(star_first.day_of_month_starts_with_star());
  assert!(!star_last.day_of_month_starts_with_star());
  assert!(!star_first.day_of_week_starts_with_star());
}

#[test]
fn a_hashed_value_needs_the_seeded_entry_point() {
  assert_eq!(
    *Schedule::<Cronexpr>::parse("H 0 * * *")
      .expect_err("no seed was given")
      .kind(),
    ErrorKind::HashedValueNeedsSeed
  );

  let schedule = Schedule::<Cronexpr>::parse_with("H 0 * * *", 30).expect("seeded");
  assert!(schedule.calendar().expect("a calendar").admits_minute(30));

  assert_eq!(
    *Schedule::<Vixie>::parse("H 0 * * *")
      .expect_err("Vixie has no `H`")
      .kind(),
    ErrorKind::HashedValueNotSupported { dialect: "Vixie" }
  );
}

#[test]
fn a_timezone_is_retained_at_the_default_tier() {
  // No feature enabled here: the name comes back as written and nothing resolves it,
  // which is the whole of what the parsing tier promises.
  let zoned = ZonedSchedule::<Cronexpr>::parse("30 2 * * 1-5 Asia/Shanghai").expect("legal");
  assert_eq!(zoned.timezone(), Some("Asia/Shanghai"));

  let calendar: &Calendar<Cronexpr> = zoned.schedule().calendar().expect("a calendar");
  assert!(calendar.admits_minute(30));
  assert!(calendar.admits_weekday(Weekday::Monday));

  assert_eq!(
    *ZonedSchedule::<Vixie>::parse("30 2 * * 1-5 Asia/Shanghai")
      .expect_err("Vixie carries no timezone")
      .kind(),
    ErrorKind::TimezoneNotSupported { dialect: "Vixie" }
  );
}
