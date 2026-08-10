//! Quartz's date predicates, which are not set members.
//!
//! `L`, `LW`, `L-n`, `nW` and `n#m` do not name values a field admits; they name a
//! property of a date. "The last weekday of the month" is not day 28, 29, 30 or 31 — it
//! is whichever of those the calendar produces, and `nW` can move the matched day into
//! an adjacent week. None of that fits in a bitset, so it lives beside one.

use crate::date::{CivilDateTime, Weekday};

#[cfg(test)]
mod tests;

/// A predicate over the day of the month.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum DayOfMonthModifier {
  /// `L` — the last day of the month.
  Last,
  /// `L-n` — `n` days before the last day of the month.
  LastOffset {
    /// How many days before the last, `1..=30`.
    days: u8,
  },
  /// `LW` — the last Monday-to-Friday of the month.
  LastWeekday,
  /// `nW` — the weekday nearest day `n`, without leaving the month.
  ///
  /// If day `n` is a Saturday the match moves back a day, and if it is a Sunday it moves
  /// forward a day — unless that would cross a month boundary, in which case it moves
  /// the other way. `1W` in a month beginning on a Sunday matches the 2nd, which is in
  /// the following week.
  NearestWeekday {
    /// The target day, `1..=31`.
    day: u8,
  },
}

impl DayOfMonthModifier {
  /// Whether the date satisfies the predicate.
  #[must_use]
  pub fn matches(self, date: &CivilDateTime) -> bool {
    let last = date.days_in_month();
    match self {
      Self::Last => date.day() == last,
      Self::LastOffset { days } => match last.checked_sub(days) {
        Some(target) if target >= 1 => date.day() == target,
        // `L-31` in February names no day. Quartz treats that as never firing rather
        // than as an error, because the same expression is satisfiable in a longer
        // month.
        _ => false,
      },
      Self::LastWeekday => match last_weekday_of_month(date) {
        Some(target) => date.day() == target,
        None => false,
      },
      Self::NearestWeekday { day } => match nearest_weekday(date, day) {
        Some(target) => date.day() == target,
        None => false,
      },
    }
  }
}

/// A predicate over the day of the week.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum DayOfWeekModifier {
  /// `nL` — the last such weekday of the month.
  Last {
    /// The day of the week.
    weekday: Weekday,
  },
  /// `n#m` — the `m`th such weekday of the month.
  Nth {
    /// The day of the week.
    weekday: Weekday,
    /// Which one, `1..=5`.
    nth: u8,
  },
}

impl DayOfWeekModifier {
  /// Whether the date satisfies the predicate.
  #[must_use]
  #[inline(always)]
  pub fn matches(self, date: &CivilDateTime) -> bool {
    match self {
      Self::Last { weekday } => {
        date.weekday() == weekday && date.day().saturating_add(7) > date.days_in_month()
      }
      Self::Nth { weekday, nth } => {
        date.weekday() == weekday && (date.day().saturating_sub(1) / 7).saturating_add(1) == nth
      }
    }
  }
}

/// The last Monday-to-Friday of the date's month.
fn last_weekday_of_month(date: &CivilDateTime) -> Option<u8> {
  let mut day = date.days_in_month();
  // At most two steps: the last three days of a month cannot all be weekend days.
  while day >= 1 {
    if date.weekday_of_day(day).is_weekday() {
      return Some(day);
    }
    day = day.checked_sub(1)?;
  }
  None
}

/// The day Quartz's `nW` fires on, or `None` when the month is too short.
fn nearest_weekday(date: &CivilDateTime, target: u8) -> Option<u8> {
  let last = date.days_in_month();
  if target < 1 || target > last {
    return None;
  }

  match date.weekday_of_day(target) {
    Weekday::Saturday => {
      let back = target.checked_sub(1)?;
      // `1W` on a Saturday cannot fire on the previous month's Friday, so it jumps
      // forward to the Monday instead.
      if back >= 1 {
        Some(back)
      } else {
        let forward = target.checked_add(2)?;
        (forward <= last).then_some(forward)
      }
    }
    Weekday::Sunday => {
      let forward = target.checked_add(1)?;
      if forward <= last {
        Some(forward)
      } else {
        let back = target.checked_sub(2)?;
        (back >= 1).then_some(back)
      }
    }
    _ => Some(target),
  }
}
