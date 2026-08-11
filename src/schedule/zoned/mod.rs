//! A schedule that carries the timezone it was written in.
//!
//! Three tiers, and which one a build has is a question about features rather than about
//! dialects. The grammar half — may an expression in this dialect end with an IANA name
//! at all — is [`Dialect::TIMEZONE`], a compile-time constant like every other dialect
//! decision. The capability half — can this build turn that name into a
//! [`jiff::tz::TimeZone`] — is a cargo feature, because it is a question about what was
//! compiled in and not about what the text may say.
//!
//! | tier | feature | what it does |
//! |---|---|---|
//! | parse | *(default)* | retains the name as a borrowed `&str` and resolves nothing |
//! | static | `tz-static` | resolves against timezones the application names at compile time |
//! | runtime | `tz` | resolves any IANA name at runtime |
//!
//! The parsing tier is not a degraded mode. It is the same boundary this crate already
//! draws for calendar arithmetic: cronp reads the expression and hands the caller
//! everything it said, and what the caller does with a timezone name is theirs. A
//! `no_std`, no-alloc build gets the name.
//!
//! # Why a separate type
//!
//! [`Schedule`] and [`Calendar`](crate::Calendar) have no lifetime and are named
//! everywhere; a timezone is borrowed from the input and would give them one. But the
//! reason this is a sibling rather than a field is semantic rather than a matter of
//! borrowing: a schedule *with* a timezone denotes different instants from the same
//! schedule without one, which is the same reason the dialect is a type instead of a
//! runtime tag.

use core::ops::Range;

use crate::{
  dialect::Dialect,
  error::{ErrorKind, ParseError, Span},
  token::is_space_byte,
};

use super::{Schedule, count_fields};

#[cfg(test)]
mod tests;

/// A [`Schedule`] and the IANA timezone name the expression ended with.
///
/// Parsed by [`Self::parse`], which accepts the trailing name only where the dialect
/// declares [`Dialect::TIMEZONE`]. The name is borrowed from the input and is *not*
/// resolved: see the [module documentation](self) for the tier that resolves it.
///
/// The timezone is optional even where the dialect admits one, because a crontab line
/// that omits it is still a line the dialect accepts. [`Self::timezone`] is `None` then,
/// and the schedule means whatever the caller's own default timezone says it means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZonedSchedule<'a, D, const N: usize = 1> {
  schedule: Schedule<D, N>,
  timezone: Option<&'a str>,
}

impl<'a, D: Dialect, const N: usize> ZonedSchedule<'a, D, N> {
  /// Parses an expression that may end with an IANA timezone name.
  ///
  /// # Errors
  ///
  /// [`ErrorKind::TimezoneNotSupported`] when the dialect takes no timezone —
  /// constructing this type for such a dialect is the mistake, and it is reported rather
  /// than silently parsing as a plain [`Schedule`] would. Otherwise the first thing wrong
  /// with the expression, exactly as [`Schedule::parse`] reports it, plus
  /// [`ErrorKind::MalformedTimezone`] for a trailing field that cannot be a timezone name.
  pub fn parse(input: &'a str) -> Result<Self, ParseError> {
    Self::parse_seeded(input, None)
  }

  /// Parses an expression that may end with an IANA timezone name, resolving `H` against
  /// `seed`.
  ///
  /// [`Schedule::parse_with`] is the same entry point for an expression without a
  /// timezone, and the reason there are two of each is the same: the seed is a runtime
  /// input and cannot be an associated constant.
  ///
  /// # Errors
  ///
  /// As [`Self::parse`].
  pub fn parse_with(input: &'a str, seed: u64) -> Result<Self, ParseError> {
    Self::parse_seeded(input, Some(seed))
  }

  fn parse_seeded(input: &'a str, seed: Option<u64>) -> Result<Self, ParseError> {
    if !D::TIMEZONE {
      return Err(ParseError::new(
        ErrorKind::TimezoneNotSupported { dialect: D::NAME },
        Span::new(0, input.len()),
      ));
    }

    // One field more than the dialect's own maximum is the timezone, and any other count
    // is left to the schedule parser to report — which knows the dialect's bounds and
    // says so in one error, instead of this splitting a field off an expression that was
    // already the wrong length and mislabelling whatever it found.
    let expected = usize::from(D::MAX_FIELDS);
    let Some(name) = (count_fields(input) == expected.saturating_add(1))
      .then(|| last_field(input))
      .flatten()
    else {
      return Ok(Self {
        schedule: Schedule::parse_seeded(input, seed)?,
        timezone: None,
      });
    };

    let text = input.get(name.clone()).unwrap_or_default();
    if !is_timezone_name(text) {
      return Err(ParseError::new(ErrorKind::MalformedTimezone, name.into()));
    }

    // The expression without its timezone. A prefix slice keeps every byte at the offset
    // it had, so the spans the schedule parser reports still point into `input`.
    let expression = input.get(..name.start).unwrap_or_default();
    Ok(Self {
      schedule: Schedule::parse_seeded(expression, seed)?,
      timezone: Some(text),
    })
  }

  /// The schedule, without the timezone.
  #[must_use]
  pub const fn schedule(&self) -> &Schedule<D, N> {
    &self.schedule
  }

  /// The IANA timezone name the expression ended with, exactly as it was written.
  ///
  /// Retained, not resolved, and not checked against any database: at the default tier
  /// there is no database to check it against. What is guaranteed is that it is made of
  /// the characters an IANA identifier is made of — see [`ErrorKind::MalformedTimezone`].
  #[must_use]
  pub const fn timezone(&self) -> Option<&'a str> {
    self.timezone
  }

  /// The schedule and the timezone name, given up together.
  #[must_use]
  pub const fn into_parts(self) -> (Schedule<D, N>, Option<&'a str>) {
    (self.schedule, self.timezone)
  }
}

#[cfg(feature = "tz-static")]
#[cfg_attr(docsrs, doc(cfg(feature = "tz-static")))]
impl<'a, D, const N: usize> ZonedSchedule<'a, D, N> {
  /// Resolves the retained name against a table the application built at compile time.
  ///
  /// The `tz-static` tier, which is `no_std` and allocation-free. jiff can build a
  /// [`TimeZone`](jiff::tz::TimeZone) in a `const` context from an IANA name written as a
  /// literal, so an application that knows which timezones it will ever see can embed
  /// exactly those and nothing else — no database, no filesystem, no allocator. What it
  /// cannot do is resolve a name nobody wrote down, and that is the whole difference
  /// between this tier and `tz`.
  ///
  /// ```
  /// # #[cfg(feature = "tz-static")] {
  /// use cronp::{Cronexpr, ZonedSchedule};
  /// use jiff::tz::{self, TimeZone};
  ///
  /// static ZONES: &[(&str, TimeZone)] = &[
  ///   ("Asia/Shanghai", tz::get!("Asia/Shanghai")),
  ///   ("UTC", tz::get!("UTC")),
  /// ];
  ///
  /// let schedule = ZonedSchedule::<Cronexpr>::parse("0 4 * * * Asia/Shanghai").unwrap();
  /// let zone = schedule.resolve_in(ZONES).unwrap().unwrap();
  /// assert_eq!(zone.iana_name(), Some("Asia/Shanghai"));
  /// # }
  /// ```
  ///
  /// # Errors
  ///
  /// [`UnknownTimeZone`] when the expression named a timezone the table does not carry.
  /// `Ok(None)` when it named none.
  pub fn resolve_in<'t>(
    &self,
    table: &'t [(&str, jiff::tz::TimeZone)],
  ) -> Result<Option<&'t jiff::tz::TimeZone>, UnknownTimeZone<'a>> {
    let Some(name) = self.timezone else {
      return Ok(None);
    };
    table
      .iter()
      .find(|(candidate, _)| *candidate == name)
      .map(|(_, zone)| Some(zone))
      .ok_or(UnknownTimeZone { name })
  }
}

#[cfg(feature = "tz")]
#[cfg_attr(docsrs, doc(cfg(feature = "tz")))]
impl<D, const N: usize> ZonedSchedule<'_, D, N> {
  /// Resolves the retained name against the IANA database compiled into this build.
  ///
  /// The `tz` tier. Unlike [`Self::resolve_in`] this needs no registration — any name the
  /// database knows resolves — and that is what it costs `std` and an allocator for.
  ///
  /// # Errors
  ///
  /// jiff's own error when the name does not resolve, because a caller already using jiff
  /// should not have to translate one. `Ok(None)` when the expression named no timezone.
  pub fn resolve(&self) -> Result<Option<jiff::tz::TimeZone>, jiff::Error> {
    match self.timezone {
      None => Ok(None),
      Some(name) => jiff::tz::TimeZone::get(name).map(Some),
    }
  }
}

/// A timezone name that the application's static table does not carry.
///
/// Only the `tz-static` tier produces this. At the `tz` tier a name that does not resolve
/// is jiff's own error, because there the database is jiff's rather than the caller's.
#[cfg(feature = "tz-static")]
#[cfg_attr(docsrs, doc(cfg(feature = "tz-static")))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UnknownTimeZone<'a> {
  name: &'a str,
}

#[cfg(feature = "tz-static")]
impl<'a> UnknownTimeZone<'a> {
  /// The name the expression carried.
  #[must_use]
  pub const fn name(&self) -> &'a str {
    self.name
  }
}

#[cfg(feature = "tz-static")]
impl core::fmt::Display for UnknownTimeZone<'_> {
  fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
    write!(
      f,
      "no timezone named `{}` was registered with this build",
      self.name
    )
  }
}

#[cfg(feature = "tz-static")]
impl core::error::Error for UnknownTimeZone<'_> {}

/// The byte range of the last whitespace-separated field.
///
/// `None` for an input that is empty or entirely whitespace, which
/// [`count_fields`] has already counted as no fields at all.
fn last_field(input: &str) -> Option<Range<usize>> {
  let bytes = input.as_bytes();

  let mut end = bytes.len();
  while end > 0 {
    match bytes.get(end.wrapping_sub(1)) {
      Some(&byte) if is_space_byte(byte) => end -= 1,
      _ => break,
    }
  }
  if end == 0 {
    return None;
  }

  let mut start = end;
  while start > 0 {
    match bytes.get(start.wrapping_sub(1)) {
      Some(&byte) if !is_space_byte(byte) => start -= 1,
      _ => break,
    }
  }
  Some(start..end)
}

/// Whether the text is made of the characters an IANA timezone identifier is made of.
///
/// A shape check and deliberately nothing more. Whether a well-formed name *exists* is a
/// question for a database, which the parsing tier does not have and will not pretend to;
/// what this rules out is a trailing field that could not be a timezone under any
/// database, so that a sixth cron field written by mistake is not silently retained as
/// one. Every character IANA uses is admitted: letters, digits, `/`, `_`, `-`, `+` and
/// `.`, which together cover `Etc/GMT+5` and `America/Port-au-Prince` as well as `UTC`.
fn is_timezone_name(text: &str) -> bool {
  !text.is_empty()
    && text
      .bytes()
      .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'_' | b'-' | b'+' | b'.'))
}
