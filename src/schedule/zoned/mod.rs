//! A schedule that carries the timezone it was written in.
//!
//! Three tiers, and which one a build has is a question about features rather than about
//! dialects. The grammar half — may an expression in this dialect end with an IANA name
//! at all — is [`Dialect::TIMEZONE`], a compile-time constant like every other dialect
//! decision. The capability half — can this build turn that name into a
//! [`jiff::tz::TimeZone`] — is a cargo feature, because it is a question about what was
//! compiled in and not about what the text may say.
//!
//! | tier | feature | what it answers | how |
//! |---|---|---|---|
//! | parse | *(default)* | is this **shaped like** a name, and is it one of *mine* | [`ZonedSchedule::timezone_name`], [`ZonedSchedule::validate_in`] |
//! | static | `tz-static` | is this one of the zones I compiled in | `resolve_in` |
//! | runtime | `tz` | is this a zone at all | `resolve` |
//!
//! The parsing tier is not a degraded mode. It is the same boundary this crate already
//! draws for calendar arithmetic: cronp reads the expression and hands the caller
//! everything it said, and what the caller does with a timezone name is theirs. A
//! `no_std`, no-alloc build gets the name.
//!
//! # What the parsing tier does *not* promise
//!
//! It never decides whether a zone **exists**. `is_timezone_name` judges shape, and shape
//! cannot tell `Asia/Shanghai` from `Definitely/NotAZone`, or the weekday range `MON-FRI`
//! from the real zone `W-SU`. Tightening the grammar would not fix that and was rejected
//! for the reason it deserves to be: a rule like "a component must be at least two
//! characters" refuses `L` and `W` but is a heuristic aimed at cron with no identifier
//! grammar behind it, and the next zone-shaped cron field walks straight through it.
//!
//! So the tier says so twice, in the two places a caller meets it. The accessor is
//! [`ZonedSchedule::timezone_name`] rather than `timezone`, because what you get back is a
//! *name* and not a zone. And [`ZonedSchedule::validate_in`] is the parsing tier's own
//! answer to "does this one exist": the caller's list is the database, because at this
//! tier there is no other. It is the same shape as `resolve_in` and returns the same
//! [`UnknownTimeZone`], so a build that later turns a feature on changes which database
//! answers and not how the refusal is spelled.
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
/// that omits it is still a line the dialect accepts. [`Self::timezone_name`] is `None` then,
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
  /// than silently parsing as a plain [`Schedule`] would. It is reported whatever the text
  /// says, because no edit to the text would help.
  ///
  /// Otherwise the first thing wrong with the expression, exactly as [`Schedule::parse`]
  /// reports it — the same span contract included — and [`ErrorKind::MalformedTimezone`]
  /// for a trailing field that cannot be a timezone name only once everything before it
  /// has parsed. The timezone is the last field, so it is the last thing that can be
  /// wrong; an expression with a fault in both halves reports the one in the cron half.
  ///
  /// A run in the timezone position that is not shaped like a name is reported *there*,
  /// as `MalformedTimezone` over its own bytes, rather than folded back into the field
  /// count. At one run more than the dialect takes, the last run is the timezone by
  /// position, and naming the run that cannot be one says more than
  /// [`ErrorKind::WrongFieldCount`] over the whole expression could.
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

    // The expression without its timezone. A prefix slice keeps every byte at the offset
    // it had, so the spans the schedule parser reports still point into `input`.
    //
    // Parsed *before* the trailing field is checked, and that order is the contract
    // rather than a convenience. The prefix occupies `..name.start` and the name occupies
    // `name.start..`, so every failure the schedule parser can raise sits to the left of
    // every failure this can: reporting the timezone first would hand back the last thing
    // wrong. It sent a caller off to correct a timezone while the expression it belongs
    // to was still invalid — and a caller branching on `MalformedTimezone` cannot tell
    // that case from one where the timezone is the only fault.
    let expression = input.get(..name.start).unwrap_or_default();
    let schedule = Schedule::parse_seeded(expression, seed)?;

    let text = input.get(name.clone()).unwrap_or_default();
    if !is_timezone_name(text) {
      return Err(ParseError::new(ErrorKind::MalformedTimezone, name.into()));
    }

    Ok(Self {
      schedule,
      timezone: Some(text),
    })
  }

  /// The schedule, without the timezone.
  #[must_use]
  pub const fn schedule(&self) -> &Schedule<D, N> {
    &self.schedule
  }

  /// The timezone **name** the expression ended with, exactly as it was written.
  ///
  /// Named for what it hands back. This is a name and not a zone: it is retained, not
  /// resolved, and not checked against any database, because at the default tier there is
  /// no database to check it against. What is guaranteed is the *shape* — `/`-separated
  /// components, each beginning with a letter, see [`ErrorKind::MalformedTimezone`] — and
  /// shape cannot tell a real zone from a plausible one. `Definitely/NotAZone` and the
  /// weekday range `MON-FRI` both arrive here intact.
  ///
  /// [`Self::validate_in`] is how this tier answers whether the name is one you accept.
  /// The `tz-static` and `tz` tiers answer it against jiff instead.
  #[must_use]
  pub const fn timezone_name(&self) -> Option<&'a str> {
    self.timezone
  }

  /// Checks the retained name against the names the caller accepts.
  ///
  /// The parsing tier's own answer to "does this zone exist", and the only one it can
  /// honestly give: **the caller's list is the database**, because this tier has no other
  /// and will not pretend to. That makes the "refused somewhere" half of the tier design
  /// real at the default tier rather than a promise redeemable only by turning a feature
  /// on — which was the gap, and which is why a shape check alone is not a design.
  ///
  /// Deliberately by name and not by zone: resolving needs jiff, and this tier does not
  /// have it. A build that later enables `tz-static` gets `resolve_in`, whose refusal is
  /// this same [`UnknownTimeZone`], so only the database changes.
  ///
  /// ```
  /// use cronp::{Cronexpr, ZonedSchedule};
  ///
  /// const ZONES: &[&str] = &["Asia/Shanghai", "UTC"];
  ///
  /// let good = ZonedSchedule::<Cronexpr>::parse("0 4 * * * Asia/Shanghai").unwrap();
  /// assert_eq!(good.validate_in(ZONES).unwrap(), Some("Asia/Shanghai"));
  ///
  /// // Well shaped, so the parser kept it; not a zone this caller accepts, so this is
  /// // where it is refused. Nothing else in the default tier would have said so.
  /// let bad = ZonedSchedule::<Cronexpr>::parse("0 4 * * * MON-FRI").unwrap();
  /// assert_eq!(bad.validate_in(ZONES).unwrap_err().name(), "MON-FRI");
  ///
  /// // An expression that named none has nothing to check.
  /// let bare = ZonedSchedule::<Cronexpr>::parse("0 4 * * *").unwrap();
  /// assert_eq!(bare.validate_in(ZONES).unwrap(), None);
  /// ```
  ///
  /// # Errors
  ///
  /// [`UnknownTimeZone`] when the expression named a timezone `accepted` does not carry.
  /// `Ok(None)` when it named none.
  pub fn validate_in(&self, accepted: &[&str]) -> Result<Option<&'a str>, UnknownTimeZone<'a>> {
    let Some(name) = self.timezone else {
      return Ok(None);
    };
    if accepted.contains(&name) {
      Ok(Some(name))
    } else {
      Err(UnknownTimeZone { name })
    }
  }

  /// The schedule and the timezone name, given up together.
  ///
  /// The second element is [`Self::timezone_name`]'s value and carries its caveat: a
  /// name, shape-checked and unresolved.
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

/// A timezone name that the database it was checked against does not carry.
///
/// Two tiers raise it and they differ only in whose database that is: the caller's list at
/// the parsing tier ([`ZonedSchedule::validate_in`]), the application's compiled-in table
/// at `tz-static` (`resolve_in`). Ungated for that reason — the concept is "this name is
/// not in the set you gave me", which needs no jiff and no allocator. The `tz` tier does
/// not raise it, because there the database is jiff's and the refusal is jiff's own error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UnknownTimeZone<'a> {
  name: &'a str,
}

impl<'a> UnknownTimeZone<'a> {
  /// The name the expression carried.
  #[must_use]
  pub const fn name(&self) -> &'a str {
    self.name
  }
}

impl core::fmt::Display for UnknownTimeZone<'_> {
  fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
    write!(f, "no timezone named `{}` was accepted here", self.name)
  }
}

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

/// Whether the text has the shape of an IANA timezone identifier.
///
/// A shape check, and the line it draws is the tier boundary itself: this decides whether
/// a run *could* be a zone name under some database, never whether it *is* one under this
/// build's. The parsing tier has no database — that is what makes it the parsing tier — so
/// a well-shaped name that nothing defines is accepted here and handed back verbatim, and
/// the tiers that resolve are where it is refused: `UnknownTimeZone` at `tz-static`, jiff's
/// own error at `tz`. Nothing on this path is a lookup. (Named rather than linked, because
/// the type it names exists only when that feature is on.)
///
/// The shape is `zic`'s, as the tzdata `Theory` file states it: one or more components
/// separated by `/`, each **non-empty**, each **beginning with an ASCII letter**, and each
/// continuing with ASCII alphanumerics, `_`, `-`, `+` or `.`. That admits every name IANA
/// defines — `UTC`, `Asia/Shanghai`, `Etc/GMT+5`, `America/Port-au-Prince`,
/// `America/Argentina/Buenos_Aires`, `EST5EDT`, `W-SU` — and refuses the shapes a cron
/// field takes, which is the only reason to check at all.
///
/// # What a character allowlist let through
///
/// This used to admit any run of `[A-Za-z0-9/_+.-]`, and admitting a leading digit made it
/// far too wide. `ZonedSchedule::<Cronexpr>::parse("0 0 * * * 2025")` was accepted with
/// the timezone `Some("2025")` — a year written into a dialect that has no year field,
/// retained as a zone rather than refused, which is exactly the extra cron field the split
/// exists to catch. `1-5`, `1/5` and `2025-2030` went the same way, as did every run of
/// bare separators (`-`, `.`, `..`, `+`, `_`, `///`) and `/Asia/Shanghai`,
/// `Asia//Shanghai` and `Asia/Shanghai/`, whose empty components no database can hold.
///
/// # What it still admits, and why it must
///
/// A run of letters, or of letters and hyphens, is where a real name and a name-spelled
/// cron field are the *same shape*: `MON` and `MON-FRI` cannot be told from `Cuba`,
/// `Japan`, `Zulu`, `W-SU` or `GB-Eire` by looking at them. Separating those is a database
/// question, so this keeps them and the resolving tiers answer it. That residue is the
/// price of the boundary rather than a gap in the check, and
/// `the_shapes_a_sixth_field_can_take_are_each_decided` writes down every shape on both
/// sides of it.
fn is_timezone_name(text: &str) -> bool {
  // `"".split('/')` yields one empty component, so an empty run is refused by the
  // component rule and needs no guard of its own.
  text.split('/').all(is_timezone_component)
}

/// Whether one `/`-separated component of an identifier is well formed.
///
/// The leading-letter rule is what a digit-first allowlist was missing, and it is a rule
/// about identifiers rather than a rule about cron: no zone IANA defines begins a
/// component with anything but a letter, and every shape a cron field takes that survives
/// the character test — a year, a range of years, a step — begins one with a digit.
fn is_timezone_component(component: &str) -> bool {
  let mut bytes = component.bytes();
  bytes.next().is_some_and(|byte| byte.is_ascii_alphabetic())
    && bytes.all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'+' | b'.'))
}
