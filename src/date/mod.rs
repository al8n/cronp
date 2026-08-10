//! The date a schedule is matched against.
//!
//! A structure with a checked constructor rather than a tuple of seven integers.
//! `(2024, 3, 1, 9, 30, 0, 5)` is trivially callable in the wrong order and nothing
//! catches it; and the seventh component, the weekday, cannot be supplied by a caller at
//! all without opening the possibility that it disagrees with the other six. It is
//! derived here instead, so a date cannot lie about what day of the week it falls on.
//!
//! The default tier cannot depend on jiff, so this type owes nothing to it. With the
//! `jiff` feature on there is a conversion from `jiff::civil::DateTime`.

use core::fmt;

#[cfg(test)]
mod tests;

/// The first year a [`CivilDateTime`] may name.
const MIN_YEAR: u16 = 1;
/// The last year a [`CivilDateTime`] may name.
const MAX_YEAR: u16 = 9999;

/// A day of the week, counted from Sunday.
///
/// Cron dialects disagree about which *digit* means which day, so this crate stores and
/// reports days as a name rather than a number. The digits are converted at parse time
/// and never escape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Weekday {
  /// Sunday, canonical `0`.
  Sunday,
  /// Monday, canonical `1`.
  Monday,
  /// Tuesday, canonical `2`.
  Tuesday,
  /// Wednesday, canonical `3`.
  Wednesday,
  /// Thursday, canonical `4`.
  Thursday,
  /// Friday, canonical `5`.
  Friday,
  /// Saturday, canonical `6`.
  Saturday,
}

impl Weekday {
  /// The canonical number, `0` for Sunday through `6` for Saturday.
  #[must_use]
  #[inline(always)]
  pub const fn to_canonical(self) -> u8 {
    self as u8
  }

  /// The day a canonical number names, or `None` above `6`.
  #[must_use]
  #[inline(always)]
  pub const fn from_canonical(canonical: u8) -> Option<Self> {
    Some(match canonical {
      0 => Self::Sunday,
      1 => Self::Monday,
      2 => Self::Tuesday,
      3 => Self::Wednesday,
      4 => Self::Thursday,
      5 => Self::Friday,
      6 => Self::Saturday,
      _ => return None,
    })
  }

  /// Whether this is a working day, Monday through Friday.
  ///
  /// Quartz's `W` and `LW` predicates are defined in terms of it.
  #[must_use]
  #[inline(always)]
  pub const fn is_weekday(self) -> bool {
    !matches!(self, Self::Saturday | Self::Sunday)
  }
}

/// Which component of a date was out of range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum DateComponent {
  /// The year.
  Year,
  /// The month.
  Month,
  /// The day of the month.
  Day,
  /// The hour.
  Hour,
  /// The minute.
  Minute,
  /// The second.
  Second,
}

impl DateComponent {
  /// The component's name, as it appears in a message.
  #[must_use]
  #[inline(always)]
  pub const fn as_str(self) -> &'static str {
    match self {
      Self::Year => "year",
      Self::Month => "month",
      Self::Day => "day",
      Self::Hour => "hour",
      Self::Minute => "minute",
      Self::Second => "second",
    }
  }
}

impl fmt::Display for DateComponent {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str(self.as_str())
  }
}

/// A date that does not exist.
///
/// One shape for every component, because every failure is the same failure: a value
/// outside the range that component admits. For [`DateComponent::Day`] the range is the
/// one the *month* admits, so a February 30th says `1..=29` rather than `1..=31`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DateError {
  component: DateComponent,
  value: u32,
  min: u32,
  max: u32,
}

impl DateError {
  /// Which component was wrong.
  #[must_use]
  #[inline(always)]
  pub const fn component(&self) -> DateComponent {
    self.component
  }

  /// The value as supplied.
  #[must_use]
  #[inline(always)]
  pub const fn value(&self) -> u32 {
    self.value
  }

  /// The lowest value that component admits here.
  #[must_use]
  #[inline(always)]
  pub const fn min(&self) -> u32 {
    self.min
  }

  /// The highest value that component admits here.
  #[must_use]
  #[inline(always)]
  pub const fn max(&self) -> u32 {
    self.max
  }
}

impl fmt::Display for DateError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(
      f,
      "{} {} is outside {}..={}",
      self.component, self.value, self.min, self.max
    )
  }
}

impl core::error::Error for DateError {}

/// An instant on the civil calendar, with no time zone and no clock.
///
/// Construction is checked: a `CivilDateTime` that exists is a date that exists, and its
/// weekday agrees with its date because the weekday is computed rather than supplied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CivilDateTime {
  year: u16,
  month: u8,
  day: u8,
  hour: u8,
  minute: u8,
  second: u8,
  weekday: Weekday,
}

impl CivilDateTime {
  /// Builds a date, rejecting one that does not exist.
  ///
  /// The day is checked against the length of the given month in the given year, so
  /// February 29th is accepted in 2024 and rejected in 2023.
  pub const fn new(
    year: u16,
    month: u8,
    day: u8,
    hour: u8,
    minute: u8,
    second: u8,
  ) -> Result<Self, DateError> {
    if year < MIN_YEAR || year > MAX_YEAR {
      return Err(out_of_range(
        DateComponent::Year,
        year as u32,
        MIN_YEAR as u32,
        MAX_YEAR as u32,
      ));
    }
    if month < 1 || month > 12 {
      return Err(out_of_range(DateComponent::Month, month as u32, 1, 12));
    }
    let last = days_in_month(year, month);
    if day < 1 || day > last {
      return Err(out_of_range(DateComponent::Day, day as u32, 1, last as u32));
    }
    if hour > 23 {
      return Err(out_of_range(DateComponent::Hour, hour as u32, 0, 23));
    }
    if minute > 59 {
      return Err(out_of_range(DateComponent::Minute, minute as u32, 0, 59));
    }
    if second > 59 {
      return Err(out_of_range(DateComponent::Second, second as u32, 0, 59));
    }

    let weekday = match Weekday::from_canonical(weekday_of(year, month, day)) {
      Some(weekday) => weekday,
      // `weekday_of` returns a remainder modulo seven, so this cannot happen; a
      // `match` rather than an unwrap keeps the function `const` and panic-free.
      None => Weekday::Sunday,
    };

    Ok(Self {
      year,
      month,
      day,
      hour,
      minute,
      second,
      weekday,
    })
  }

  /// The year.
  #[must_use]
  #[inline(always)]
  pub const fn year(&self) -> u16 {
    self.year
  }

  /// The month, `1..=12`.
  #[must_use]
  #[inline(always)]
  pub const fn month(&self) -> u8 {
    self.month
  }

  /// The day of the month, `1..=31`.
  #[must_use]
  #[inline(always)]
  pub const fn day(&self) -> u8 {
    self.day
  }

  /// The hour, `0..=23`.
  #[must_use]
  #[inline(always)]
  pub const fn hour(&self) -> u8 {
    self.hour
  }

  /// The minute, `0..=59`.
  #[must_use]
  #[inline(always)]
  pub const fn minute(&self) -> u8 {
    self.minute
  }

  /// The second, `0..=59`.
  #[must_use]
  #[inline(always)]
  pub const fn second(&self) -> u8 {
    self.second
  }

  /// The day of the week, derived from the date.
  #[must_use]
  #[inline(always)]
  pub const fn weekday(&self) -> Weekday {
    self.weekday
  }

  /// How many days this date's month has.
  #[must_use]
  #[inline(always)]
  pub const fn days_in_month(&self) -> u8 {
    days_in_month(self.year, self.month)
  }

  /// The weekday of another day in this same month.
  ///
  /// Quartz's `W` predicate has to know what day of the week the *target* day falls on,
  /// which is rarely the day being tested: `15W` in a month where the 15th is a Sunday
  /// matches the 16th. Deriving it by offset from this date avoids a second calendar
  /// computation and cannot disagree with [`Self::weekday`].
  #[must_use]
  #[inline(always)]
  pub const fn weekday_of_day(&self, day: u8) -> Weekday {
    let offset = day as i32 - self.day as i32;
    let canonical = (self.weekday.to_canonical() as i32 + offset).rem_euclid(7);
    match Weekday::from_canonical(canonical as u8) {
      Some(weekday) => weekday,
      None => self.weekday,
    }
  }
}

impl fmt::Display for CivilDateTime {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(
      f,
      "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
      self.year, self.month, self.day, self.hour, self.minute, self.second
    )
  }
}

#[inline(always)]
const fn out_of_range(component: DateComponent, value: u32, min: u32, max: u32) -> DateError {
  DateError {
    component,
    value,
    min,
    max,
  }
}

/// Whether a year has a February 29th.
#[must_use]
#[inline(always)]
pub const fn is_leap_year(year: u16) -> bool {
  year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

/// How many days a month has.
///
/// A month outside `1..=12` reports 0, so a caller that skipped the range check gets an
/// empty month rather than a panic.
#[must_use]
#[inline(always)]
pub const fn days_in_month(year: u16, month: u8) -> u8 {
  match month {
    1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
    4 | 6 | 9 | 11 => 30,
    2 => {
      if is_leap_year(year) {
        29
      } else {
        28
      }
    }
    _ => 0,
  }
}

/// The canonical weekday of a civil date, `0` for Sunday.
///
/// Howard Hinnant's days-from-civil, shifted so that 1970-01-01 — a Thursday — lands on
/// `4`. Integer arithmetic only, so it is `const` and cannot overflow for any year this
/// type admits.
#[inline(always)]
const fn weekday_of(year: u16, month: u8, day: u8) -> u8 {
  let y = year as i32;
  let m = month as i32;
  let d = day as i32;

  let y = if m <= 2 { y - 1 } else { y };
  let era = if y >= 0 { y } else { y - 399 } / 400;
  let year_of_era = y - era * 400;
  let month_shifted = (m + 9) % 12;
  let day_of_year = (153 * month_shifted + 2) / 5 + d - 1;
  let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
  let days = era * 146097 + day_of_era - 719_468;

  (days + 4).rem_euclid(7) as u8
}
