#![allow(
  clippy::indexing_slicing,
  clippy::unwrap_used,
  clippy::expect_used,
  clippy::panic
)]

use std::{vec, vec::Vec};

use super::{DayOfMonthModifier, DayOfWeekModifier};
use crate::date::{CivilDateTime, Weekday};

fn at(year: u16, month: u8, day: u8) -> CivilDateTime {
  CivilDateTime::new(year, month, day, 0, 0, 0).expect("a real date")
}

/// Every day of a month on which the predicate holds.
fn firing_days(year: u16, month: u8, predicate: impl Fn(&CivilDateTime) -> bool) -> Vec<u8> {
  let last = crate::date::days_in_month(year, month);
  (1..=last)
    .filter(|day| predicate(&at(year, month, *day)))
    .collect()
}

// ---------------------------------------------------------------------------
// L, L-n, LW
// ---------------------------------------------------------------------------

#[test]
fn last_is_the_last_day_however_long_the_month() {
  let last = DayOfMonthModifier::Last;
  assert_eq!(firing_days(2023, 1, |d| last.matches(d)), vec![31]);
  assert_eq!(firing_days(2023, 4, |d| last.matches(d)), vec![30]);
  assert_eq!(
    firing_days(2023, 2, |d| last.matches(d)),
    vec![28],
    "a common year"
  );
  assert_eq!(
    firing_days(2024, 2, |d| last.matches(d)),
    vec![29],
    "a leap year"
  );
}

#[test]
fn last_offset_counts_back_from_the_end() {
  let three = DayOfMonthModifier::LastOffset { days: 3 };
  assert_eq!(firing_days(2023, 1, |d| three.matches(d)), vec![28]);
  assert_eq!(firing_days(2023, 4, |d| three.matches(d)), vec![27]);
  assert_eq!(firing_days(2024, 2, |d| three.matches(d)), vec![26]);

  // An offset longer than the month names no day rather than wrapping or panicking.
  let thirty = DayOfMonthModifier::LastOffset { days: 30 };
  assert_eq!(
    firing_days(2023, 2, |d| thirty.matches(d)),
    Vec::<u8>::new()
  );
  assert_eq!(firing_days(2023, 1, |d| thirty.matches(d)), vec![1]);
}

#[test]
fn last_weekday_walks_back_over_the_weekend() {
  let lw = DayOfMonthModifier::LastWeekday;

  // April 2023 ends on Sunday the 30th, so the last weekday is Friday the 28th:
  // two steps back, which is the most the predicate ever has to take.
  assert_eq!(at(2023, 4, 30).weekday(), Weekday::Sunday);
  assert_eq!(firing_days(2023, 4, |d| lw.matches(d)), vec![28]);

  // December 2023 ends on Sunday the 31st: back to Friday the 29th.
  assert_eq!(at(2023, 12, 31).weekday(), Weekday::Sunday);
  assert_eq!(firing_days(2023, 12, |d| lw.matches(d)), vec![29]);

  // June 2023 ends on Friday the 30th: no walking at all.
  assert_eq!(at(2023, 6, 30).weekday(), Weekday::Friday);
  assert_eq!(firing_days(2023, 6, |d| lw.matches(d)), vec![30]);

  // September 2023 ends on Saturday the 30th: one step back.
  assert_eq!(at(2023, 9, 30).weekday(), Weekday::Saturday);
  assert_eq!(firing_days(2023, 9, |d| lw.matches(d)), vec![29]);
}

// ---------------------------------------------------------------------------
// nW — the predicate that can move the matched day into an adjacent week.
// ---------------------------------------------------------------------------

#[test]
fn nearest_weekday_moves_forward_from_a_sunday_first_of_month() {
  // The fixture the plan asks for: a first of the month falling on a Sunday. `1W`
  // then fires on Monday the 2nd, which is in the *following* week — the whole reason
  // this is a predicate and not a bitset entry.
  assert_eq!(at(2023, 1, 1).weekday(), Weekday::Sunday);
  let one = DayOfMonthModifier::NearestWeekday { day: 1 };
  assert_eq!(firing_days(2023, 1, |d| one.matches(d)), vec![2]);
  assert_eq!(at(2023, 1, 2).weekday(), Weekday::Monday);
}

#[test]
fn nearest_weekday_does_not_jump_out_of_the_month() {
  // April 2023 begins on a Saturday. Moving back would land on March 31st, so `1W`
  // moves forward two days to Monday the 3rd instead.
  assert_eq!(at(2023, 4, 1).weekday(), Weekday::Saturday);
  let one = DayOfMonthModifier::NearestWeekday { day: 1 };
  assert_eq!(firing_days(2023, 4, |d| one.matches(d)), vec![3]);

  // December 2023 ends on a Sunday. `31W` cannot move forward into January, so it
  // moves back two days to Friday the 29th.
  assert_eq!(at(2023, 12, 31).weekday(), Weekday::Sunday);
  let thirty_one = DayOfMonthModifier::NearestWeekday { day: 31 };
  assert_eq!(firing_days(2023, 12, |d| thirty_one.matches(d)), vec![29]);
}

#[test]
fn nearest_weekday_steps_one_day_in_the_ordinary_cases() {
  // The 15th of April 2023 is a Saturday: back to Friday the 14th.
  assert_eq!(at(2023, 4, 15).weekday(), Weekday::Saturday);
  let fifteen = DayOfMonthModifier::NearestWeekday { day: 15 };
  assert_eq!(firing_days(2023, 4, |d| fifteen.matches(d)), vec![14]);

  // The 15th of January 2023 is a Sunday: forward to Monday the 16th.
  assert_eq!(at(2023, 1, 15).weekday(), Weekday::Sunday);
  assert_eq!(firing_days(2023, 1, |d| fifteen.matches(d)), vec![16]);

  // The 15th of March 2023 is a Wednesday: no movement.
  assert_eq!(at(2023, 3, 15).weekday(), Weekday::Wednesday);
  assert_eq!(firing_days(2023, 3, |d| fifteen.matches(d)), vec![15]);
}

#[test]
fn nearest_weekday_in_a_month_too_short_names_no_day() {
  let thirty_one = DayOfMonthModifier::NearestWeekday { day: 31 };
  assert_eq!(
    firing_days(2023, 2, |d| thirty_one.matches(d)),
    Vec::<u8>::new(),
    "February has no 31st, so `31W` fires in February not at all"
  );
  assert_eq!(
    firing_days(2023, 4, |d| thirty_one.matches(d)),
    Vec::<u8>::new(),
    "April has thirty days"
  );
}

#[test]
fn every_day_of_a_month_has_exactly_one_nearest_weekday_target() {
  // A sweep rather than a spot check: for every month of a leap year and a common
  // year, `nW` must fire on exactly one day for every `n` the month contains.
  for year in [2023u16, 2024] {
    for month in 1..=12u8 {
      let last = crate::date::days_in_month(year, month);
      for day in 1..=last {
        let predicate = DayOfMonthModifier::NearestWeekday { day };
        let days = firing_days(year, month, |d| predicate.matches(d));
        assert_eq!(days.len(), 1, "{year}-{month:02} {day}W fired on {days:?}");
        let fired = days[0];
        assert!(
          at(year, month, fired).weekday().is_weekday(),
          "{year}-{month:02} {day}W fired on a weekend day, {fired}"
        );
        assert!(
          fired.abs_diff(day) <= 2,
          "{year}-{month:02} {day}W fired {fired}, more than two days away"
        );
      }
    }
  }
}

// ---------------------------------------------------------------------------
// nL and n#m
// ---------------------------------------------------------------------------

#[test]
fn the_last_weekday_of_its_kind_is_the_one_in_the_final_week() {
  let last_friday = DayOfWeekModifier::Last {
    weekday: Weekday::Friday,
  };
  // April 2023's Fridays are the 7th, 14th, 21st and 28th.
  assert_eq!(firing_days(2023, 4, |d| last_friday.matches(d)), vec![28]);

  let last_sunday = DayOfWeekModifier::Last {
    weekday: Weekday::Sunday,
  };
  assert_eq!(firing_days(2023, 4, |d| last_sunday.matches(d)), vec![30]);

  // February of a leap year has exactly four of each weekday, so the last Thursday
  // is the 29th — the one day an off-by-one in the month length would move.
  let last_thursday = DayOfWeekModifier::Last {
    weekday: Weekday::Thursday,
  };
  assert_eq!(firing_days(2024, 2, |d| last_thursday.matches(d)), vec![29]);
}

#[test]
fn the_nth_weekday_counts_from_the_first_of_the_month() {
  let third_friday = DayOfWeekModifier::Nth {
    weekday: Weekday::Friday,
    nth: 3,
  };
  assert_eq!(firing_days(2023, 4, |d| third_friday.matches(d)), vec![21]);

  let first_friday = DayOfWeekModifier::Nth {
    weekday: Weekday::Friday,
    nth: 1,
  };
  assert_eq!(firing_days(2023, 4, |d| first_friday.matches(d)), vec![7]);

  // A fifth occurrence exists in some months and not others, and must simply not fire
  // in the months that lack it.
  let fifth_sunday = DayOfWeekModifier::Nth {
    weekday: Weekday::Sunday,
    nth: 5,
  };
  assert_eq!(firing_days(2023, 4, |d| fifth_sunday.matches(d)), vec![30]);
  assert_eq!(
    firing_days(2023, 6, |d| fifth_sunday.matches(d)),
    Vec::<u8>::new()
  );
}

#[test]
fn nth_and_last_agree_on_the_final_occurrence() {
  // Whichever the final occurrence is, `nL` and the `n#m` that names it must pick the
  // same day. A disagreement would mean one of the two arithmetics is wrong.
  for year in [2023u16, 2024] {
    for month in 1..=12u8 {
      for canonical in 0..7u8 {
        let weekday = Weekday::from_canonical(canonical).unwrap();
        let last = DayOfWeekModifier::Last { weekday };
        let fired = firing_days(year, month, |d| last.matches(d));
        assert_eq!(fired.len(), 1, "{year}-{month:02} {weekday:?}L");

        let occurrence = (fired[0] - 1) / 7 + 1;
        let nth = DayOfWeekModifier::Nth {
          weekday,
          nth: occurrence,
        };
        assert_eq!(
          firing_days(year, month, |d| nth.matches(d)),
          fired,
          "{year}-{month:02} {weekday:?}L is occurrence {occurrence}"
        );
      }
    }
  }
}
