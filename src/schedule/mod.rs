//! Whole expressions, and the dialect gating that decides what each one may contain.

use core::{marker::PhantomData, ops::Range, time::Duration};

use crate::{
  date::{CivilDateTime, Weekday},
  dialect::{Dialect, DomDowRule},
  error::{ErrorKind, ParseError, Span},
  every,
  field::{FieldOutcome, FieldSpec, Mask, Modifier, Parsed, parse_field},
  modifier::{DayOfMonthModifier, DayOfWeekModifier},
  token::{Cursor, is_space_byte},
  years::Years,
};

#[cfg(test)]
pub(crate) mod reference;
#[cfg(test)]
pub(crate) mod tests;
mod zoned;

pub use zoned::{UnknownTimeZone, ZonedSchedule};

/// What an expression denoted.
///
/// Three shapes, because cron has three. Most expressions are a set of calendar
/// instants; `@every` is a period, which is not a set of instants and is not stored as
/// one; and `@reboot` is neither.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Schedule<D, const N: usize = 1> {
  /// A set of calendar instants.
  Calendar(Calendar<D, N>),
  /// `@every <duration>`: a fixed period between firings.
  ///
  /// It denotes a length of time rather than a set of instants, so there is nothing for
  /// a bitset to hold.
  Every(Duration),
  /// `@reboot`: fire once when the daemon starts.
  ///
  /// Legal Vixie syntax, and parsed rather than rejected, because a parser that refuses
  /// legal input cannot read real crontabs. Its *meaning* needs a process lifetime,
  /// which this crate does not have and will not invent: a `@reboot` schedule never
  /// matches an instant and has no next occurrence. That is a property of this variant,
  /// stated here, not a limitation to be discovered at runtime.
  Reboot,
}

impl<D: Dialect, const N: usize> Schedule<D, N> {
  /// Parses an expression in the dialect `D`.
  ///
  /// # Errors
  ///
  /// Returns the first thing wrong with the expression, with a byte span into `input`
  /// and, once the parser knows which field it was in, the field.
  pub fn parse(input: &str) -> Result<Self, ParseError> {
    Self::parse_seeded(input, None)
  }

  /// Parses an expression in the dialect `D`, resolving `H` against `seed`.
  ///
  /// A second entry point rather than a parameter on the first, because the two inputs
  /// are not the same kind of thing. Every other decision a dialect makes is an
  /// associated constant and costs a parse nothing; the seed arrives at runtime and
  /// cannot be one. Keeping it here leaves [`Self::parse`] — the call every dialect
  /// without `H` makes — exactly as it was.
  ///
  /// `H` stands for a value chosen by hashing the seed into the values the field admits,
  /// so the same seed always yields the same schedule and different seeds spread load
  /// across callers. It is the *values* rather than the digits they are written with, and
  /// the two differ in day-of-week, where `0` and `7` are both Sunday: hashing over the
  /// digits would give Sunday two of eight buckets and every other day one. Only a
  /// dialect whose [`Dialect::HASHED_VALUES`] is set admits `H`; the rest report
  /// [`ErrorKind::HashedValueNotSupported`] whatever seed is passed.
  ///
  /// # Errors
  ///
  /// As [`Self::parse`].
  pub fn parse_with(input: &str, seed: u64) -> Result<Self, ParseError> {
    Self::parse_seeded(input, Some(seed))
  }

  fn parse_seeded(input: &str, seed: Option<u64>) -> Result<Self, ParseError> {
    let mut cursor = Cursor::new(input);
    cursor.skip_space();

    // Nothing here decides anything about *content*. An expression is empty when there
    // is nothing left after the leading whitespace, and that is the only thing this
    // reports; a nickname is recognised because it is a whole expression rather than a
    // field, and everything else — including a byte that begins no token — goes to
    // `parse_calendar`, which counts the fields and then reads them. That is where a bad
    // byte gets a span and the name of the field it sits in.
    match cursor.peek() {
      None => Err(ParseError::new(
        ErrorKind::EmptyExpression,
        Span::new(input.len(), input.len()),
      )),
      Some(b'@') => {
        let start = cursor.pos();
        let taken = cursor.take_macro();
        let span = start..cursor.pos();
        match taken {
          Some(raw) => parse_macro::<D, N>(&mut cursor, raw, span),
          // A lone `@` is not a nickname. `take_macro` leaves the cursor on it for
          // exactly this reason: it is an ordinary bad byte in an ordinary field, and
          // the field is what should say so.
          None => parse_calendar::<D, N>(&mut cursor, input, seed).map(Schedule::Calendar),
        }
      }
      Some(_) => parse_calendar::<D, N>(&mut cursor, input, seed).map(Schedule::Calendar),
    }
  }

  /// Whether the schedule fires at this civil instant.
  ///
  /// The whole decision, taken inside the crate: every field, both date predicates, and
  /// the dialect's day-of-month against day-of-week rule. There is nothing left for a
  /// caller to combine, which is the point — the rule keys on facts about how each day
  /// field was written, and those are not recoverable from the set of days it denotes.
  ///
  /// Civil time, and no time zone. Whether some absolute instant *is* this civil instant
  /// is a question about a zone and an offset, and it is the caller's; a schedule fires
  /// at wall-clock times, which is what makes `0 2 * * *` mean two in the morning
  /// wherever the machine happens to be. [`CivilDateTime`] derives its own weekday, so
  /// there is no way to ask about a date and a day of the week that disagree.
  ///
  /// # The two variants that are not a set of instants
  ///
  /// [`Schedule::Every`] denotes a length of time rather than a set of instants, so it
  /// has no instant to fire at until something anchors it, and this crate has no anchor
  /// to offer: a caller holding one computes from [`Self::every`]. [`Schedule::Reboot`]
  /// needs a process lifetime, as its own documentation says. Both answer `false` for
  /// every instant.
  ///
  /// ```
  /// use cronp::{CivilDateTime, Schedule, Vixie};
  ///
  /// let weekdays = Schedule::<Vixie>::parse("30 2 * * 1-5")?;
  /// // 2026-08-12 is a Wednesday.
  /// assert!(weekdays.matches(CivilDateTime::new(2026, 8, 12, 2, 30, 0)?));
  /// assert!(!weekdays.matches(CivilDateTime::new(2026, 8, 12, 2, 31, 0)?));
  /// // 2026-08-15 is a Saturday.
  /// assert!(!weekdays.matches(CivilDateTime::new(2026, 8, 15, 2, 30, 0)?));
  ///
  /// // Vixie's union rule: neither day field begins with a star, so either may match.
  /// let either = Schedule::<Vixie>::parse("0 0 1 * MON")?;
  /// assert!(either.matches(CivilDateTime::new(2026, 8, 1, 0, 0, 0)?)); // a Saturday
  /// assert!(either.matches(CivilDateTime::new(2026, 8, 10, 0, 0, 0)?)); // a Monday
  /// # Ok::<(), Box<dyn core::error::Error>>(())
  /// ```
  #[must_use]
  pub fn matches(&self, when: CivilDateTime) -> bool {
    match self {
      Self::Calendar(calendar) => calendar.matches(when),
      Self::Every(_) | Self::Reboot => false,
    }
  }

  /// The period of an `@every` schedule.
  #[must_use]
  pub const fn every(&self) -> Option<Duration> {
    match self {
      Self::Every(period) => Some(*period),
      _ => None,
    }
  }

  /// The calendar behind an ordinary schedule.
  #[must_use]
  pub const fn calendar(&self) -> Option<&Calendar<D, N>> {
    match self {
      Self::Calendar(calendar) => Some(calendar),
      _ => None,
    }
  }
}

/// The set of instants an ordinary cron expression denotes.
///
/// Fixed size and allocation-free. The day-of-week bitset is in the canonical numbering,
/// `0` for Sunday, whatever numbering the expression was written in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Calendar<D, const N: usize = 1> {
  seconds: u64,
  minutes: u64,
  hours: u32,
  days_of_month: u32,
  months: u16,
  days_of_week: u8,
  years: Years<N>,
  day_of_month_modifier: Option<DayOfMonthModifier>,
  day_of_week_modifier: Option<DayOfWeekModifier>,
  // One flag per field, because a field that constrains nothing stores no bits and an
  // empty set has to be read as "every value" rather than as "none". "Constrains
  // nothing" is a question about the union the field denotes, not about how it was
  // written: `*`, `?`, `*/1` and any list with one of those in it all set this false.
  seconds_restricted: bool,
  minutes_restricted: bool,
  hours_restricted: bool,
  day_of_month_restricted: bool,
  months_restricted: bool,
  day_of_week_restricted: bool,
  year_restricted: bool,
  // Whether each day field carries the wildcard its dialect's union rule keys off. Kept
  // only for the two fields that rule reads, because no other field has a question that
  // the restriction flag above cannot already answer.
  day_of_month_wildcard: bool,
  day_of_week_wildcard: bool,
  dialect: PhantomData<D>,
}

impl<D, const N: usize> Calendar<D, N> {
  /// Whether the schedule admits this second.
  #[must_use]
  pub const fn admits_second(&self, second: u8) -> bool {
    admits(self.seconds_restricted, self.seconds, second, 0, 59)
  }

  /// Whether the schedule admits this minute.
  #[must_use]
  pub const fn admits_minute(&self, minute: u8) -> bool {
    admits(self.minutes_restricted, self.minutes, minute, 0, 59)
  }

  /// Whether the schedule admits this hour.
  #[must_use]
  pub const fn admits_hour(&self, hour: u8) -> bool {
    admits(self.hours_restricted, self.hours as u64, hour, 0, 23)
  }

  /// Whether the day-of-month *bitset* admits this day.
  ///
  /// Not on the front door, and this is the accessor the audit was about: it is the
  /// bitset alone, so a field carrying `L` or `15W` answers `false` for every day of
  /// every month, and even where it does answer for the field, combining it with the
  /// day-of-week field takes a rule no accessor here can carry. [`Self::matches`] is the
  /// answer; this is one term of it.
  pub(crate) const fn admits_day_of_month(&self, day: u8) -> bool {
    admits(
      self.day_of_month_restricted,
      self.days_of_month as u64,
      day,
      1,
      31,
    )
  }

  /// Whether the schedule admits this month.
  #[must_use]
  pub const fn admits_month(&self, month: u8) -> bool {
    admits(self.months_restricted, self.months as u64, month, 1, 12)
  }

  /// Whether the day-of-week bitset admits this day.
  ///
  /// The day-of-week half of [`Self::admits_day_of_month`], and off the front door for
  /// the same two reasons.
  pub(crate) const fn admits_weekday(&self, weekday: Weekday) -> bool {
    admits(
      self.day_of_week_restricted,
      self.days_of_week as u64,
      weekday.to_canonical(),
      0,
      6,
    )
  }

  /// Whether the schedule admits this year.
  ///
  /// Two questions in order, and the order is the point. First, does the *dialect*
  /// admit the year: Quartz declares `1970..=2099` and refuses an explicit 2100, so a
  /// Quartz schedule cannot fire in 2100 merely because its year field was left as `*`.
  /// Only a dialect with no year field at all is unbounded. Second, if the expression
  /// narrowed the years, is this one among them.
  ///
  /// A year field that constrains nothing therefore admits every year the dialect does,
  /// including years beyond what `N` can enumerate: the expression placed no
  /// restriction, so none is applied. That covers `*` and every list with a `*` in it —
  /// `2025,*` denotes the same years as `*`, and both are answered here without `N`
  /// entering into it.
  #[must_use]
  pub fn admits_year(&self, year: u16) -> bool
  where
    D: Dialect,
  {
    D::YEAR.admits(year) && (!self.year_restricted || self.years.contains(year))
  }

  /// The years the expression enumerated.
  ///
  /// Empty exactly when the expression narrowed nothing: a year field absent altogether,
  /// or one whose union is every year — `*`, `*/1`, and any list with one of those in it
  /// — places no constraint and writes nothing, so there is no truncated set to misread.
  /// A year field that *did* narrow admits at least one year, so an empty set is never a
  /// restriction to none. [`Self::admits_year`] is the authoritative answer either way,
  /// because it also applies the bounds the dialect declares.
  #[must_use]
  pub const fn years(&self) -> &Years<N> {
    &self.years
  }

  /// The day-of-month predicate, if the expression carried one.
  #[must_use]
  pub const fn day_of_month_modifier(&self) -> Option<DayOfMonthModifier> {
    self.day_of_month_modifier
  }

  /// The day-of-week predicate, if the expression carried one.
  #[must_use]
  pub const fn day_of_week_modifier(&self) -> Option<DayOfWeekModifier> {
    self.day_of_week_modifier
  }

  /// Whether the calendar fires at this civil instant.
  ///
  /// [`Schedule::matches`] is the usual entry point; this is the same answer for a
  /// calendar held on its own.
  ///
  /// Every field has to admit its component and the two day fields have to satisfy the
  /// dialect's rule between them. The day fields are the only ones that need a rule:
  /// nothing combines the seconds with the minutes except "and".
  #[must_use]
  pub fn matches(&self, when: CivilDateTime) -> bool
  where
    D: Dialect,
  {
    self.admits_year(when.year())
      && self.admits_month(when.month())
      && self.admits_hour(when.hour())
      && self.admits_minute(when.minute())
      && self.admits_second(when.second())
      && self.admits_date(&when)
  }

  /// Whether the two day fields, combined by the dialect's rule, admit this date.
  fn admits_date(&self, when: &CivilDateTime) -> bool
  where
    D: Dialect,
  {
    let day_of_month = match self.day_of_month_modifier {
      // A predicate is the whole field — `parse_field` rejects one in a list — so the
      // bitset behind it is empty and there is nothing to combine it with. `L` is not
      // day 28, 29, 30 or 31; it is whichever of those the calendar produces.
      Some(modifier) => modifier.matches(when),
      None => self.admits_day_of_month(when.day()),
    };
    let day_of_week = match self.day_of_week_modifier {
      Some(modifier) => modifier.matches(when),
      None => self.admits_weekday(when.weekday()),
    };

    match D::DOM_DOW {
      // Exactly one of the two fields is a `?`, which admits everything, so "and" is
      // what reading the specified field alone amounts to.
      DomDowRule::Exclusive => day_of_month && day_of_week,
      // Vixie's rule, and the reason the wildcard is folded per item at parse time: a
      // field that carries the wildcard turns "or" into "and". `*,10` and `10,*` are one
      // set of days written two ways and land on opposite sides of it.
      DomDowRule::Union { .. } => {
        if self.day_of_month_wildcard || self.day_of_week_wildcard {
          day_of_month && day_of_week
        } else {
          day_of_month || day_of_week
        }
      }
    }
  }

  /// Assembles a calendar from every field of one.
  ///
  /// The only way to build a `Calendar`, and it takes a [`FieldOutcome`] per field
  /// because the alternative was tried and failed: the fields used to be assigned one at
  /// a time from a zeroed value, and the nickname path assigned the bitsets and left
  /// every outcome at `false`. Nothing said so — a missing assignment looks like every
  /// other line that is not there — and `@weekly` fired on Wednesdays for it. A caller
  /// that has no text behind a field still has to say what the field means, and
  /// [`FieldOutcome::star`] and [`FieldOutcome::value`] are how it says it.
  fn new(fields: Fields<N>) -> Self {
    // Each mask arrives as the sink's full `u64` and is narrowed here, once. Every
    // narrowing is bounded by the `FieldSpec` the field was parsed against — hours reach
    // 23, days of the month 31, months 12, weekdays 6 — so no set bit is ever above the
    // width it is narrowed to.
    Self {
      seconds: fields.seconds.values,
      minutes: fields.minutes.values,
      hours: fields.hours.values as u32,
      days_of_month: fields.days_of_month.values as u32,
      months: fields.months.values as u16,
      days_of_week: fields.days_of_week.values as u8,
      years: fields.years.values,
      day_of_month_modifier: match fields.days_of_month.outcome.modifier {
        Some(Modifier::DayOfMonth(modifier)) => Some(modifier),
        // A day-of-week predicate cannot reach the day-of-month field: `parse_item`
        // refuses `nL` and `n#m` outside day-of-week and `nW` outside day-of-month.
        _ => None,
      },
      day_of_week_modifier: match fields.days_of_week.outcome.modifier {
        Some(Modifier::DayOfWeek(modifier)) => Some(modifier),
        _ => None,
      },
      seconds_restricted: fields.seconds.outcome.restricted,
      minutes_restricted: fields.minutes.outcome.restricted,
      hours_restricted: fields.hours.outcome.restricted,
      day_of_month_restricted: fields.days_of_month.outcome.restricted,
      months_restricted: fields.months.outcome.restricted,
      day_of_week_restricted: fields.days_of_week.outcome.restricted,
      year_restricted: fields.years.outcome.restricted,
      day_of_month_wildcard: fields.days_of_month.outcome.wildcard,
      day_of_week_wildcard: fields.days_of_week.outcome.wildcard,
      dialect: PhantomData,
    }
  }
}

/// Every field of a calendar, each with the outcome its parse reported.
///
/// Named fields rather than seven arguments, so that a construction site cannot supply
/// the minutes where the seconds go and cannot leave a field out: Rust has no partial
/// struct literal, so the type is what makes the omission unwritable.
pub(crate) struct Fields<const N: usize> {
  pub(crate) seconds: Parsed<u64>,
  pub(crate) minutes: Parsed<u64>,
  pub(crate) hours: Parsed<u64>,
  pub(crate) days_of_month: Parsed<u64>,
  pub(crate) months: Parsed<u64>,
  pub(crate) days_of_week: Parsed<u64>,
  pub(crate) years: Parsed<Years<N>>,
}

/// Whether a field admits a value.
///
/// An unrestricted field stores no bits, so an empty set means "every value" rather
/// than "none". The bounds check applies either way: no field admits a value outside
/// the range it is defined over, however the field was written.
const fn admits(restricted: bool, bits: u64, value: u8, min: u8, max: u8) -> bool {
  if value < min || value > max {
    return false;
  }
  !restricted || bit_set_64(bits, value)
}

const fn bit_set_64(bits: u64, index: u8) -> bool {
  if index >= 64 {
    return false;
  }
  bits >> index & 1 == 1
}

// ---------------------------------------------------------------------------
// Macros.
// ---------------------------------------------------------------------------

/// The nickname macros, and the fields each one pins.
///
/// Built directly rather than by re-parsing a substitute expression, because the
/// substitute would have to be written in the dialect's own weekday numbering and
/// `@weekly` would then mean Sunday under Vixie and Monday under Quartz.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Nickname {
  Yearly,
  Monthly,
  Weekly,
  Daily,
  Hourly,
}

impl Nickname {
  fn from_name(name: &str) -> Option<Self> {
    Some(match name {
      "yearly" | "annually" => Self::Yearly,
      "monthly" => Self::Monthly,
      "weekly" => Self::Weekly,
      "daily" | "midnight" => Self::Daily,
      "hourly" => Self::Hourly,
      _ => return None,
    })
  }
}

fn parse_macro<D: Dialect, const N: usize>(
  cursor: &mut Cursor<'_>,
  raw: &str,
  raw_span: Range<usize>,
) -> Result<Schedule<D, N>, ParseError> {
  let span: Span = raw_span.into();

  // `raw` includes the `@`. Names are compared case-insensitively because crontabs in
  // the wild are not consistent about it.
  let name = raw.get(1..).unwrap_or("");
  let lowered = |candidate: &str| name.eq_ignore_ascii_case(candidate);

  if lowered("every") {
    if !D::EVERY {
      return Err(ParseError::new(
        ErrorKind::EveryNotSupported { dialect: D::NAME },
        span,
      ));
    }
    // Two faults, and one kind used to answer for both. Nothing after `@every` is an
    // empty duration and carets at the end of the input; something that is not
    // whitespace is a *separator* fault, and `@every1s` in Robfig reported "needs a
    // duration after it" over the `1` of a duration that was right there. `EmptyDuration`
    // is a statement about text that is absent, so it may only be raised where the text
    // is.
    match cursor.peek() {
      None => {
        return Err(ParseError::new(
          ErrorKind::EmptyDuration,
          cursor.end_span().into(),
        ));
      }
      Some(byte) if !is_space_byte(byte) => {
        return Err(ParseError::new(
          ErrorKind::UnexpectedToken,
          cursor.next_span().into(),
        ));
      }
      Some(_) => {}
    }
    cursor.skip_space();
    let (text, base) = cursor.rest();
    return every::parse(text.trim_end(), base).map(Schedule::Every);
  }

  if lowered("reboot") {
    if !D::REBOOT {
      return Err(ParseError::new(
        ErrorKind::RebootNotSupported { dialect: D::NAME },
        span,
      ));
    }
    expect_end(cursor)?;
    return Ok(Schedule::Reboot);
  }

  let Some(nickname) = Nickname::from_name(&lowercase_name(name)) else {
    return Err(ParseError::new(ErrorKind::UnknownMacro, span));
  };
  if !D::MACROS {
    return Err(ParseError::new(
      ErrorKind::MacroNotSupported { dialect: D::NAME },
      span,
    ));
  }
  expect_end(cursor)?;
  Ok(Schedule::Calendar(nickname_calendar::<D, N>(nickname)))
}

/// Lowercases a macro name into a fixed buffer.
///
/// Macro names are at most eight ASCII letters, so no allocation is needed and a longer
/// name simply fails to match, which is the right answer for it anyway.
fn lowercase_name(name: &str) -> LowercaseName {
  let mut buffer = [0u8; 8];
  let mut length = 0usize;
  for byte in name.bytes() {
    let Some(slot) = buffer.get_mut(length) else {
      return LowercaseName {
        buffer,
        length: usize::MAX,
      };
    };
    *slot = byte.to_ascii_lowercase();
    length = length.saturating_add(1);
  }
  LowercaseName { buffer, length }
}

struct LowercaseName {
  buffer: [u8; 8],
  length: usize,
}

impl core::ops::Deref for LowercaseName {
  type Target = str;

  fn deref(&self) -> &str {
    // `length == usize::MAX` marks a name too long to be a nickname; an empty string
    // matches no nickname, which is the answer.
    self
      .buffer
      .get(..self.length)
      .and_then(|bytes| core::str::from_utf8(bytes).ok())
      .unwrap_or("")
  }
}

fn nickname_calendar<D: Dialect, const N: usize>(nickname: Nickname) -> Calendar<D, N> {
  // A field a nickname leaves open is a field written `*`, and a field it pins is a
  // field written as one value. Both take their outcome from the facts that spelling
  // parses to, which is what stops this path and the parsing path disagreeing about a
  // question neither of them writes down twice.
  //
  // The one that mattered is the wildcard: upstream vixie's `entry.c` sets `DOM_STAR` or
  // `DOW_STAR` on every nickname it expands, and robfig's `all()` carries `starBit` into
  // whichever fields it fills, so `@weekly` intersects and fires on Sundays. Leaving the
  // outcomes at their zero values made it union and fire every day.
  let open = FieldOutcome::star::<D>();
  let pinned = FieldOutcome::value::<D>();
  let pin = |values: u64| Parsed {
    values,
    outcome: pinned,
  };
  let leave_open = |values: u64| Parsed {
    values,
    outcome: open,
  };

  // Every nickname pins the time of day and leaves the date fields open — and an open
  // field stores no bits, because its emptiness is read as "every value". A dialect with
  // no seconds field still fires at second zero of the matching minute, so the seconds
  // field is pinned either way.
  let mut fields = Fields::<N> {
    seconds: pin(1),
    minutes: pin(1),
    hours: pin(1),
    days_of_month: leave_open(0),
    months: leave_open(0),
    days_of_week: leave_open(0),
    years: Parsed {
      values: Years::new(),
      outcome: open,
    },
  };

  match nickname {
    Nickname::Yearly => {
      fields.days_of_month = pin(1 << 1);
      fields.months = pin(1 << 1);
    }
    Nickname::Monthly => {
      fields.days_of_month = pin(1 << 1);
    }
    Nickname::Weekly => {
      fields.days_of_week = pin(1 << Weekday::Sunday.to_canonical());
    }
    Nickname::Daily => {}
    Nickname::Hourly => {
      fields.hours = leave_open(0);
    }
  }

  Calendar::new(fields)
}

fn expect_end(cursor: &mut Cursor<'_>) -> Result<(), ParseError> {
  cursor.skip_space();
  if cursor.at_end() {
    Ok(())
  } else {
    Err(ParseError::new(
      ErrorKind::TrailingInput,
      cursor.next_span().into(),
    ))
  }
}

// ---------------------------------------------------------------------------
// Ordinary expressions.
// ---------------------------------------------------------------------------

fn parse_calendar<D: Dialect, const N: usize>(
  cursor: &mut Cursor<'_>,
  input: &str,
  seed: Option<u64>,
) -> Result<Calendar<D, N>, ParseError> {
  let found = count_fields(input);
  let min = usize::from(D::MIN_FIELDS);
  let max = usize::from(D::MAX_FIELDS);
  if found < min || found > max {
    return Err(ParseError::new(
      ErrorKind::WrongFieldCount {
        found,
        min,
        max,
        dialect: D::NAME,
      },
      Span::new(0, input.len()),
    ));
  }

  let seconds = if D::HAS_SECONDS {
    read_mask::<D>(cursor, FieldSpec::SECOND, seed)?
  } else {
    // No seconds field means second zero, the same as every five-field cron. That is
    // a restriction — it admits one second out of sixty — so the outcome says so.
    Parsed {
      values: 1,
      outcome: FieldOutcome::value::<D>(),
    }
  };

  let minutes = read_mask::<D>(cursor, FieldSpec::MINUTE, seed)?;
  let hours = read_mask::<D>(cursor, FieldSpec::HOUR, seed)?;
  let days_of_month = read_mask::<D>(cursor, FieldSpec::DAY_OF_MONTH, seed)?;
  let months = read_mask::<D>(cursor, FieldSpec::MONTH, seed)?;
  let days_of_week = read_mask::<D>(cursor, FieldSpec::day_of_week::<D>(), seed)?;

  cursor.skip_space();
  let years = if cursor.at_end() {
    // A year field nobody wrote places no restriction, which is the same thing a `*`
    // year field says.
    Parsed {
      values: Years::new(),
      outcome: FieldOutcome::star::<D>(),
    }
  } else {
    let Some(spec) = FieldSpec::year::<D>() else {
      // `count_fields` already rejected an expression with more fields than the
      // dialect takes, so this is unreachable; it is written out rather than
      // asserted so that the parser has no way to panic.
      return Err(ParseError::new(
        ErrorKind::TrailingInput,
        cursor.next_span().into(),
      ));
    };
    let mut values = Years::new();
    let outcome = parse_field::<D, _>(cursor, spec, &mut values, seed)?;
    cursor.skip_space();
    if !cursor.at_end() {
      return Err(ParseError::new(
        ErrorKind::TrailingInput,
        cursor.next_span().into(),
      ));
    }
    Parsed { values, outcome }
  };

  check_dom_dow::<D>(input, days_of_month.outcome, days_of_week.outcome)?;
  Ok(Calendar::new(Fields {
    seconds,
    minutes,
    hours,
    days_of_month,
    months,
    days_of_week,
    years,
  }))
}

/// Parses one bitset field and the whitespace after it.
///
/// The values and the outcome leave together, so nothing downstream can take one and
/// forget the other.
fn read_mask<D: Dialect>(
  cursor: &mut Cursor<'_>,
  spec: FieldSpec,
  seed: Option<u64>,
) -> Result<Parsed<u64>, ParseError> {
  let mut mask = Mask::default();
  let outcome = parse_field::<D, Mask>(cursor, spec, &mut mask, seed)?;
  cursor.skip_space();
  Ok(Parsed {
    values: mask.bits(),
    outcome,
  })
}

/// Applies the dialect's day-of-month against day-of-week rule.
fn check_dom_dow<D: Dialect>(
  input: &str,
  dom: FieldOutcome,
  dow: FieldOutcome,
) -> Result<(), ParseError> {
  let DomDowRule::Exclusive = D::DOM_DOW else {
    // Under the union rule both fields may be restricted; what that *means* is the
    // matcher's business, not the parser's.
    return Ok(());
  };

  let whole = Span::new(0, input.len());
  match (dom.question_mark, dow.question_mark) {
    (true, false) | (false, true) => Ok(()),
    (false, false) => Err(ParseError::new(
      ErrorKind::QuestionMarkRequired { dialect: D::NAME },
      whole,
    )),
    (true, true) => Err(ParseError::new(
      ErrorKind::QuestionMarkInBothDayFields { dialect: D::NAME },
      whole,
    )),
  }
}

/// Counts the whitespace-separated fields, so that a wrong count can be reported as one
/// rather than as whatever the first field to run out of input happens to complain about.
///
/// A field is a maximal run of non-whitespace bytes. That is the same answer a walk over
/// the token stream gives, and it is the same answer for a reason rather than by
/// coincidence: a whitespace run is the only lexeme whose text is whitespace, no other
/// lexeme's span contains a whitespace byte, and a byte that begins no token at all still
/// advances the scan past itself. So the runs of non-whitespace lexemes and the runs of
/// non-whitespace bytes partition the input identically. `equivalent_to_the_token_walk`
/// holds the two against each other rather than leaving that argument unchecked.
///
/// Counting bytes rather than tokens is also what keeps this off the lexer: the tokens
/// this used to produce were all discarded, so the expression was tokenised twice.
fn count_fields(input: &str) -> usize {
  let mut fields = 0usize;
  let mut inside = false;
  for &byte in input.as_bytes() {
    if is_space_byte(byte) {
      inside = false;
    } else if !inside {
      inside = true;
      fields = fields.saturating_add(1);
    }
  }
  fields
}
