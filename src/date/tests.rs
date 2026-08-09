#![allow(
  clippy::indexing_slicing,
  clippy::unwrap_used,
  clippy::expect_used,
  clippy::panic
)]

use super::{CivilDateTime, DateComponent, Weekday, days_in_month, is_leap_year};

fn at(year: u16, month: u8, day: u8) -> CivilDateTime {
  CivilDateTime::new(year, month, day, 0, 0, 0).expect("a real date")
}

#[test]
fn the_weekday_is_derived_and_correct() {
  // 1970-01-01 is the anchor the arithmetic is shifted onto, so it goes first.
  assert_eq!(at(1970, 1, 1).weekday(), Weekday::Thursday);

  // A month whose first day is a Sunday, which is the fixture the `W` predicate needs.
  assert_eq!(at(2023, 1, 1).weekday(), Weekday::Sunday);
  assert_eq!(at(2023, 4, 1).weekday(), Weekday::Saturday);
  assert_eq!(at(2023, 12, 31).weekday(), Weekday::Sunday);

  // Across a leap day, where an off-by-one in the February branch would show.
  assert_eq!(at(2024, 2, 28).weekday(), Weekday::Wednesday);
  assert_eq!(at(2024, 2, 29).weekday(), Weekday::Thursday);
  assert_eq!(at(2024, 3, 1).weekday(), Weekday::Friday);

  // A century that is not a leap year, and one that is.
  assert_eq!(at(1900, 3, 1).weekday(), Weekday::Thursday);
  assert_eq!(at(2000, 3, 1).weekday(), Weekday::Wednesday);
}

#[test]
fn a_date_that_does_not_exist_is_rejected_by_component() {
  let error = CivilDateTime::new(2023, 2, 29, 0, 0, 0).expect_err("2023 is not a leap year");
  assert_eq!(error.component(), DateComponent::Day);
  assert_eq!((error.value(), error.min(), error.max()), (29, 1, 28));

  CivilDateTime::new(2024, 2, 29, 0, 0, 0).expect("2024 is a leap year");

  let error = CivilDateTime::new(2023, 4, 31, 0, 0, 0).expect_err("April has thirty days");
  assert_eq!((error.value(), error.min(), error.max()), (31, 1, 30));
}

#[test]
fn every_out_of_range_component_is_named() {
  let cases: [(u16, u8, u8, u8, u8, u8, DateComponent); 8] = [
    (0, 1, 1, 0, 0, 0, DateComponent::Year),
    (10_000, 1, 1, 0, 0, 0, DateComponent::Year),
    (2023, 0, 1, 0, 0, 0, DateComponent::Month),
    (2023, 13, 1, 0, 0, 0, DateComponent::Month),
    (2023, 1, 0, 0, 0, 0, DateComponent::Day),
    (2023, 1, 1, 24, 0, 0, DateComponent::Hour),
    (2023, 1, 1, 0, 60, 0, DateComponent::Minute),
    (2023, 1, 1, 0, 0, 60, DateComponent::Second),
  ];
  for (y, mo, d, h, mi, s, expected) in cases {
    let error = CivilDateTime::new(y, mo, d, h, mi, s).expect_err("must be rejected");
    assert_eq!(error.component(), expected, "{y}-{mo}-{d} {h}:{mi}:{s}");
  }
}

#[test]
fn the_boundaries_of_every_component_are_accepted() {
  CivilDateTime::new(1, 1, 1, 0, 0, 0).unwrap();
  CivilDateTime::new(9999, 12, 31, 23, 59, 59).unwrap();
}

#[test]
fn leap_years_follow_the_gregorian_rule() {
  assert!(is_leap_year(2024));
  assert!(!is_leap_year(2023));
  assert!(!is_leap_year(1900), "divisible by 100 but not 400");
  assert!(is_leap_year(2000), "divisible by 400");
  assert_eq!(days_in_month(2024, 2), 29);
  assert_eq!(days_in_month(2023, 2), 28);
  assert_eq!(days_in_month(1900, 2), 28);
  assert_eq!(days_in_month(2000, 2), 29);
}

#[test]
fn month_lengths_are_right_and_a_bad_month_is_empty_rather_than_a_panic() {
  let lengths = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
  for (index, expected) in lengths.iter().enumerate() {
    let month = u8::try_from(index + 1).unwrap();
    assert_eq!(days_in_month(2023, month), *expected, "month {month}");
  }
  assert_eq!(days_in_month(2023, 0), 0);
  assert_eq!(days_in_month(2023, 13), 0);
}

#[test]
fn another_days_weekday_is_derived_from_this_one() {
  let january = at(2023, 1, 15);
  assert_eq!(january.weekday(), Weekday::Sunday);
  assert_eq!(january.weekday_of_day(1), Weekday::Sunday);
  assert_eq!(january.weekday_of_day(16), Weekday::Monday);
  assert_eq!(january.weekday_of_day(31), Weekday::Tuesday);

  // Backwards across more than a week, where a plain `%` rather than `rem_euclid`
  // would produce a negative remainder.
  let end = at(2023, 1, 31);
  assert_eq!(end.weekday_of_day(1), Weekday::Sunday);
}

#[test]
fn weekdays_know_which_of_them_are_working_days() {
  assert!(!Weekday::Sunday.is_weekday());
  assert!(!Weekday::Saturday.is_weekday());
  for day in [
    Weekday::Monday,
    Weekday::Tuesday,
    Weekday::Wednesday,
    Weekday::Thursday,
    Weekday::Friday,
  ] {
    assert!(day.is_weekday(), "{day:?}");
  }

  for canonical in 0..7u8 {
    let day = Weekday::from_canonical(canonical).expect("0..=6 are days");
    assert_eq!(day.to_canonical(), canonical);
  }
  assert_eq!(Weekday::from_canonical(7), None);
}
