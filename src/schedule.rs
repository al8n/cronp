//! Whole expressions, and the dialect gating that decides what each one may contain.

use core::{marker::PhantomData, time::Duration};

use crate::{
  date::Weekday,
  dialect::{Dialect, DomDowRule},
  error::{ErrorKind, ParseError, Span},
  every,
  field::{parse_field, FieldOutcome, FieldSpec, Mask, Modifier},
  modifier::{DayOfMonthModifier, DayOfWeekModifier},
  token::{is_space_byte, Cursor, Token},
  years::Years,
};

#[cfg(test)]
pub(crate) mod tests;

/// What an expression denoted.
///
/// Three shapes, because cron has three. Most expressions are a set of calendar
/// instants; `@every` is a period, which is not a set of instants and is not stored as
/// one; and `@reboot` is neither.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Schedule<D: Dialect, const N: usize = 1> {
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
    let mut cursor = Cursor::new(input);
    skip_space(&mut cursor);

    match cursor.peek_token() {
      None => Err(ParseError::new(
        ErrorKind::EmptyExpression,
        cursor.end_span().into(),
      )),
      Some(Token::Macro(name)) => parse_macro::<D, N>(&mut cursor, name),
      _ => parse_calendar::<D, N>(&mut cursor, input).map(Schedule::Calendar),
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
pub struct Calendar<D: Dialect, const N: usize = 1> {
  seconds: u64,
  minutes: u64,
  hours: u32,
  days_of_month: u32,
  months: u16,
  days_of_week: u8,
  years: Years<N>,
  day_of_month_modifier: Option<DayOfMonthModifier>,
  day_of_week_modifier: Option<DayOfWeekModifier>,
  // One flag per field, because a field left as `*` stores no bits and an empty set
  // has to be read as "every value" rather than as "none".
  seconds_restricted: bool,
  minutes_restricted: bool,
  hours_restricted: bool,
  day_of_month_restricted: bool,
  months_restricted: bool,
  day_of_week_restricted: bool,
  year_restricted: bool,
  dialect: PhantomData<D>,
}

impl<D: Dialect, const N: usize> Calendar<D, N> {
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
  /// This is the bitset alone. A schedule can also carry a day-of-month predicate, and
  /// under [`DomDowRule::Union`] the day-of-week field can admit a day this does not,
  /// so this is a component of a match rather than an answer to one.
  #[must_use]
  pub const fn admits_day_of_month(&self, day: u8) -> bool {
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
  #[must_use]
  pub const fn admits_weekday(&self, weekday: Weekday) -> bool {
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
  /// A year field written as `*` therefore admits every year the dialect does,
  /// including years beyond what `N` can enumerate: the expression placed no
  /// restriction, so none is applied.
  #[must_use]
  pub fn admits_year(&self, year: u16) -> bool {
    D::YEAR.admits(year) && (!self.year_restricted || self.years.contains(year))
  }

  /// The years the expression enumerated.
  ///
  /// Empty unless [`Self::year_restricted`] is true: a year field left as `*` places no
  /// constraint and writes nothing, so there is no truncated set to misread.
  /// [`Self::admits_year`] is the authoritative answer.
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

  /// Whether the day-of-month field was written as something other than `*` or `?`.
  ///
  /// The question the Vixie union rule keys off. `1-31` counts as a restriction even
  /// though it admits every day, because Vixie asks whether the field was a star.
  #[must_use]
  pub const fn day_of_month_restricted(&self) -> bool {
    self.day_of_month_restricted
  }

  /// Whether the day-of-week field was written as something other than `*` or `?`.
  #[must_use]
  pub const fn day_of_week_restricted(&self) -> bool {
    self.day_of_week_restricted
  }

  /// Whether the expression named specific years.
  #[must_use]
  pub const fn year_restricted(&self) -> bool {
    self.year_restricted
  }

  /// The rule this schedule's dialect applies when both day fields are restricted.
  #[must_use]
  pub const fn dom_dow_rule(&self) -> DomDowRule {
    D::DOM_DOW
  }

  fn empty() -> Self {
    Self {
      seconds: 0,
      minutes: 0,
      hours: 0,
      days_of_month: 0,
      months: 0,
      days_of_week: 0,
      years: Years::new(),
      day_of_month_modifier: None,
      day_of_week_modifier: None,
      seconds_restricted: false,
      minutes_restricted: false,
      hours_restricted: false,
      day_of_month_restricted: false,
      months_restricted: false,
      day_of_week_restricted: false,
      year_restricted: false,
      dialect: PhantomData,
    }
  }
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

fn skip_space(cursor: &mut Cursor<'_>) {
  while cursor.peek_token() == Some(Token::Space) {
    cursor.bump();
  }
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
) -> Result<Schedule<D, N>, ParseError> {
  let span: Span = cursor.next_span().into();
  cursor.bump();

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
    if cursor.peek_token() != Some(Token::Space) {
      return Err(ParseError::new(
        ErrorKind::EmptyDuration,
        cursor.next_span().into(),
      ));
    }
    skip_space(cursor);
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
  let mut calendar = Calendar::<D, N>::empty();

  // Every nickname pins the time of day and leaves the date fields open, so the
  // starting point is midnight with the three date fields unrestricted — which, since
  // an unrestricted field stores no bits, means leaving them empty. A dialect with no
  // seconds field still fires at second zero of the matching minute, so the seconds
  // field is pinned either way.
  calendar.seconds = 1;
  calendar.seconds_restricted = true;
  calendar.minutes = 1;
  calendar.minutes_restricted = true;
  calendar.hours = 1;
  calendar.hours_restricted = true;

  match nickname {
    Nickname::Yearly => {
      calendar.days_of_month = 1 << 1;
      calendar.day_of_month_restricted = true;
      calendar.months = 1 << 1;
      calendar.months_restricted = true;
    }
    Nickname::Monthly => {
      calendar.days_of_month = 1 << 1;
      calendar.day_of_month_restricted = true;
    }
    Nickname::Weekly => {
      calendar.days_of_week = 1 << Weekday::Sunday.to_canonical();
      calendar.day_of_week_restricted = true;
    }
    Nickname::Daily => {}
    Nickname::Hourly => {
      calendar.hours = 0;
      calendar.hours_restricted = false;
    }
  }

  calendar
}

fn expect_end(cursor: &mut Cursor<'_>) -> Result<(), ParseError> {
  skip_space(cursor);
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

  let mut calendar = Calendar::<D, N>::empty();

  if D::HAS_SECONDS {
    let mut mask = Mask::default();
    let seconds = read_field::<D, _>(cursor, FieldSpec::SECOND, &mut mask)?;
    calendar.seconds = mask.bits();
    calendar.seconds_restricted = seconds.restricted;
  } else {
    // No seconds field means second zero, the same as every five-field cron. That is
    // a restriction — it admits one second out of sixty — so the flag says so.
    calendar.seconds = 1;
    calendar.seconds_restricted = true;
  }

  let mut minutes = Mask::default();
  let minute = read_field::<D, _>(cursor, FieldSpec::MINUTE, &mut minutes)?;
  calendar.minutes = minutes.bits();
  calendar.minutes_restricted = minute.restricted;

  let mut hours = Mask::default();
  let hour = read_field::<D, _>(cursor, FieldSpec::HOUR, &mut hours)?;
  calendar.hours = hours.bits() as u32;
  calendar.hours_restricted = hour.restricted;

  let mut days = Mask::default();
  let dom = read_field::<D, _>(cursor, FieldSpec::DAY_OF_MONTH, &mut days)?;
  calendar.days_of_month = days.bits() as u32;
  calendar.day_of_month_restricted = dom.restricted;
  if let Some(Modifier::DayOfMonth(modifier)) = dom.modifier {
    calendar.day_of_month_modifier = Some(modifier);
  }

  let mut months = Mask::default();
  let month = read_field::<D, _>(cursor, FieldSpec::MONTH, &mut months)?;
  calendar.months = months.bits() as u16;
  calendar.months_restricted = month.restricted;

  let mut weekdays = Mask::default();
  let dow = read_field::<D, _>(cursor, FieldSpec::day_of_week::<D>(), &mut weekdays)?;
  calendar.days_of_week = weekdays.bits() as u8;
  calendar.day_of_week_restricted = dow.restricted;
  if let Some(Modifier::DayOfWeek(modifier)) = dow.modifier {
    calendar.day_of_week_modifier = Some(modifier);
  }

  skip_space(cursor);
  if !cursor.at_end() {
    match FieldSpec::year::<D>() {
      Some(spec) => {
        let year = read_field::<D, _>(cursor, spec, &mut calendar.years)?;
        calendar.year_restricted = year.restricted;
      }
      // `count_fields` already rejected an expression with more fields than the
      // dialect takes, so this is unreachable; it is written out rather than
      // asserted so that the parser has no way to panic.
      None => {
        return Err(ParseError::new(
          ErrorKind::TrailingInput,
          cursor.next_span().into(),
        ))
      }
    }
    skip_space(cursor);
    if !cursor.at_end() {
      return Err(ParseError::new(
        ErrorKind::TrailingInput,
        cursor.next_span().into(),
      ));
    }
  }

  check_dom_dow::<D>(input, dom, dow)?;
  Ok(calendar)
}

/// Parses one field and the whitespace after it.
fn read_field<D: Dialect, S: crate::field::ValueSink>(
  cursor: &mut Cursor<'_>,
  spec: FieldSpec,
  sink: &mut S,
) -> Result<FieldOutcome, ParseError> {
  let outcome = parse_field::<D, S>(cursor, spec, sink)?;
  skip_space(cursor);
  Ok(outcome)
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
/// coincidence: [`Token::Space`] is the only token whose text is whitespace, no other
/// token's span contains a whitespace byte, and a byte that begins no token at all still
/// advances the lexer past itself. So the runs of non-`Space` tokens and the runs of
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
