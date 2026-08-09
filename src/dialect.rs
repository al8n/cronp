//! The dialect is a type, not a runtime tag.
//!
//! Cron dialects disagree in ways that change what a *stored* schedule means, not merely
//! what text is accepted. The sharpest case is the day-of-month against day-of-week rule:
//! the same two bitsets mean "either" under Vixie and are illegal under Quartz. A single
//! schedule type carrying a runtime dialect tag would turn every match into a branch that
//! no compiler can check, which is precisely the confusion a caller most wants prevented.
//!
//! So [`Dialect`] is a trait implemented by zero-sized types carrying the decisions as
//! associated constants. Because the dialect is known at compile time, the branches those
//! constants drive monomorphise away.

#[cfg(test)]
mod tests;

/// How many dialects this crate implements.
///
/// Every implementor's ordinal is asserted to be below this at compile time, and the
/// dialect tests fill one slot per ordinal, so a dialect cannot be added without both
/// numbers moving.
pub(crate) const DIALECT_COUNT: usize = 3;

mod sealed {
  /// Closes [`super::Dialect`] to outside implementors.
  ///
  /// The set of associated constants is not yet settled, and adding one is a breaking
  /// change for an outside implementor but not for a sealed trait. `INDEX` is a stable
  /// ordinal used to check that every dialect is covered by the tests.
  pub trait Sealed {
    const INDEX: usize;
  }
}

/// Whether a dialect has a year field, and what it admits.
///
/// The bounds live inside the `Optional` variant rather than beside the enum, so a
/// dialect without a year field has no year bounds to get wrong.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum YearField {
  /// The dialect has no year field.
  Absent,
  /// The dialect accepts a trailing year field, bounded inclusively.
  Optional {
    /// The earliest year the dialect admits.
    min: u16,
    /// The latest year the dialect admits.
    max: u16,
  },
}

impl YearField {
  /// Whether an expression in this dialect may carry a trailing year field.
  #[must_use]
  pub const fn is_optional(self) -> bool {
    matches!(self, Self::Optional { .. })
  }

  /// Whether the dialect admits this year at all.
  ///
  /// The single source for that question. A year written out is checked against these
  /// bounds at parse time and a year *not* written out is checked against them at match
  /// time, and the two answers have to agree: a dialect that refuses an explicit 2100
  /// cannot fire in 2100 merely because the expression left the field as `*`.
  ///
  /// A dialect with no year field admits every year, because it places no year
  /// restriction of any kind. That is the one genuinely unbounded case.
  #[must_use]
  pub const fn admits(self, year: u16) -> bool {
    match self {
      Self::Absent => true,
      Self::Optional { min, max } => year >= min && year <= max,
    }
  }
}

/// How a dialect numbers the days of the week.
///
/// Canonically — that is, in the bitset this crate stores — `0` is Sunday and `6` is
/// Saturday. Every dialect's numbering is converted to that at parse time, because
/// storing each dialect's own numbering would make a stored schedule uninterpretable
/// without knowing which dialect produced it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum WeekdayNumbering {
  /// `0..=6` with `0` = Sunday, and `7` accepted as a second spelling of Sunday.
  ZeroSunday,
  /// `1..=7` with `1` = Sunday, so `7` is Saturday.
  OneSunday,
}

impl WeekdayNumbering {
  /// Converts a digit written in this numbering to the canonical `0` = Sunday form.
  ///
  /// Returns `None` for a digit this numbering does not admit. The two numberings
  /// disagree about `0` and `7` as well as about every digit in between: `7` is Sunday
  /// under [`Self::ZeroSunday`] and Saturday under [`Self::OneSunday`].
  #[must_use]
  pub const fn canonical(self, digit: u32) -> Option<u8> {
    match self {
      Self::ZeroSunday => match digit {
        0..=6 => Some(digit as u8),
        7 => Some(0),
        _ => None,
      },
      Self::OneSunday => match digit {
        1..=7 => Some(digit as u8 - 1),
        _ => None,
      },
    }
  }

  /// The inclusive bounds a digit may take in this numbering.
  ///
  /// [`Self::ZeroSunday`] admits `7` as a second spelling of Sunday, so its bounds run
  /// to `7` and the fold to canonical happens per value. That is what makes a Vixie
  /// `5-7` mean Friday, Saturday and Sunday rather than wrapping or failing.
  #[must_use]
  pub const fn raw_bounds(self) -> (u32, u32) {
    match self {
      Self::ZeroSunday => (0, 7),
      Self::OneSunday => (1, 7),
    }
  }

  /// Converts a canonical `0` = Sunday weekday back into this numbering.
  #[must_use]
  pub const fn raw_from_canonical(self, canonical: u8) -> u32 {
    match self {
      Self::ZeroSunday => canonical as u32,
      Self::OneSunday => canonical as u32 + 1,
    }
  }

  /// Converts a three-letter weekday name to the canonical `0` = Sunday form.
  ///
  /// Takes no `self` because the names do not vary by numbering. `SUN` is Sunday in
  /// every dialect; only the *digits* disagree, which is why a crontab written with
  /// names is portable and one written with digits is not.
  #[must_use]
  pub fn canonical_name(name: &str) -> Option<u8> {
    const NAMES: [&str; 7] = ["SUN", "MON", "TUE", "WED", "THU", "FRI", "SAT"];
    NAMES
      .iter()
      .position(|candidate| candidate.eq_ignore_ascii_case(name))
      .map(|index| index as u8)
  }
}

/// What it means for a dialect to have both day-of-month and day-of-week restricted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum DomDowRule {
  /// The schedule fires when *either* matches.
  ///
  /// A historical quirk of Vixie cron rather than a bug, and load-bearing: `0 0 1 * MON`
  /// fires on the first of the month and on every Monday, not on Mondays that fall on
  /// the first.
  Union,
  /// Restricting both is rejected; exactly one must be `?`.
  Exclusive,
}

/// What a dialect makes of a range whose first value is after its last.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum RangePolicy {
  /// A backwards range is an error.
  ///
  /// `cron` 0.17 guards its range expansion with `start <= end` and reports an invalid
  /// range otherwise; Vixie and the Go dialect do the same.
  Ascending,
  /// A backwards range runs on through the field's ceiling and back round to its floor.
  ///
  /// Quartz documents this: `NOV-FEB` is November through February and `FRI-MON` is
  /// Friday through Monday. The year field is the exception even here — there is no
  /// modulus a year could wrap through — and Quartz refuses a backwards year outright.
  Wrapping,
}

/// Whether a dialect has the `?` token, and what it means.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum QuestionMark {
  /// The dialect has no `?`; it is a syntax error wherever it appears.
  Forbidden,
  /// `?` is another spelling of `*`, and nothing more.
  ///
  /// It admits every value and may be one item of a list, exactly as `*` may. This is
  /// what `cron` 0.17 and the Go dialect do: `?` maps to the same specifier as `*`.
  /// Like `*`, it is still legal only in day-of-month and day-of-week.
  Wildcard,
  /// `?` marks the field as *unspecified*, which is a different thing from `*`.
  ///
  /// It admits every value, but it also tells the dialect's day-of-month against
  /// day-of-week rule which of the two fields to ignore, so it must be the entire
  /// field: `?,1` would leave that rule reading a field that is half-specified.
  /// Legal only in day-of-month and day-of-week.
  NoSpecificValue,
}

impl QuestionMark {
  /// Whether the dialect has a `?` at all.
  #[must_use]
  pub const fn is_supported(self) -> bool {
    !matches!(self, Self::Forbidden)
  }

  /// Whether a `?` has to be the whole field rather than one item of a list.
  #[must_use]
  pub const fn must_be_alone(self) -> bool {
    matches!(self, Self::NoSpecificValue)
  }
}

/// The decisions that distinguish one cron dialect from another.
///
/// Sealed: see [`sealed::Sealed`]. Implementors are zero-sized, so choosing a dialect
/// costs a schedule nothing at runtime.
pub trait Dialect: sealed::Sealed + Copy + core::fmt::Debug + Default + Eq + 'static {
  /// The dialect's name, for diagnostics.
  const NAME: &'static str;

  /// Whether the expression begins with a seconds field.
  const HAS_SECONDS: bool;

  /// Whether the expression may end with a year field, and what years it admits.
  const YEAR: YearField;

  /// Which digit means which day of the week.
  const WEEKDAY: WeekdayNumbering;

  /// What restricting both day-of-month and day-of-week means.
  const DOM_DOW: DomDowRule;

  /// Whether `?` is a token, and what it means.
  const QUESTION_MARK: QuestionMark;

  /// What a range whose first value is after its last means.
  const RANGES: RangePolicy;

  /// Whether Quartz's date predicates — `L`, `LW`, `L-n`, `nW`, `n#m` — are legal.
  const MODIFIERS: bool;

  /// Whether the nickname macros `@yearly` through `@hourly` are legal.
  const MACROS: bool;

  /// Whether `@reboot` is legal.
  const REBOOT: bool;

  /// Whether `@every <duration>` is legal.
  const EVERY: bool;

  /// Whether `5/15` is read as `5-max/15`.
  ///
  /// Quartz and the Go dialect accept a step with a bare start value; Vixie requires a
  /// range or `*` before the `/`.
  const OPEN_ENDED_STEP: bool;

  /// The fewest whitespace-separated fields an expression may have.
  const MIN_FIELDS: u8 = 5 + Self::HAS_SECONDS as u8;

  /// The most whitespace-separated fields an expression may have.
  const MAX_FIELDS: u8 = 5 + Self::HAS_SECONDS as u8 + Self::YEAR.is_optional() as u8;
}

/// Vixie cron: the five-field dialect of `crontab(5)`.
///
/// Five fields, weekdays numbered from Sunday as `0` with `7` also accepted as Sunday,
/// the union rule when day-of-month and day-of-week are both restricted, the nickname
/// macros including `@reboot`, and none of Quartz's date predicates.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Vixie;

impl sealed::Sealed for Vixie {
  const INDEX: usize = 0;
}

impl Dialect for Vixie {
  const NAME: &'static str = "Vixie";
  const HAS_SECONDS: bool = false;
  const YEAR: YearField = YearField::Absent;
  const WEEKDAY: WeekdayNumbering = WeekdayNumbering::ZeroSunday;
  const DOM_DOW: DomDowRule = DomDowRule::Union;
  const QUESTION_MARK: QuestionMark = QuestionMark::Forbidden;
  const RANGES: RangePolicy = RangePolicy::Ascending;
  const MODIFIERS: bool = false;
  const MACROS: bool = true;
  const REBOOT: bool = true;
  const EVERY: bool = false;
  const OPEN_ENDED_STEP: bool = false;
}

/// Quartz: the six-or-seven-field dialect of the Java scheduler.
///
/// A leading seconds field, an optional trailing year field, weekdays numbered from
/// Sunday as `1`, the date predicates `L`, `LW`, `L-n`, `nW` and `n#m`, and a refusal to
/// let day-of-month and day-of-week both be restricted: exactly one must be `?`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Quartz;

impl sealed::Sealed for Quartz {
  const INDEX: usize = 1;
}

impl Dialect for Quartz {
  const NAME: &'static str = "Quartz";
  const HAS_SECONDS: bool = true;
  const YEAR: YearField = YearField::Optional {
    min: 1970,
    max: 2099,
  };
  const WEEKDAY: WeekdayNumbering = WeekdayNumbering::OneSunday;
  const DOM_DOW: DomDowRule = DomDowRule::Exclusive;
  const QUESTION_MARK: QuestionMark = QuestionMark::NoSpecificValue;
  const RANGES: RangePolicy = RangePolicy::Wrapping;
  const MODIFIERS: bool = true;
  const MACROS: bool = false;
  const REBOOT: bool = false;
  const EVERY: bool = false;
  const OPEN_ENDED_STEP: bool = true;
}

/// The Go dialect: `github.com/robfig/cron` with its seconds field enabled.
///
/// Six fields, Vixie's weekday numbering and union rule, `?` as a synonym for `*`, the
/// nickname macros without `@reboot`, and `@every <duration>` — the only form in cron
/// that denotes a length of time rather than a set of instants.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Robfig;

impl sealed::Sealed for Robfig {
  const INDEX: usize = 2;
}

impl Dialect for Robfig {
  const NAME: &'static str = "Robfig";
  const HAS_SECONDS: bool = true;
  const YEAR: YearField = YearField::Absent;
  const WEEKDAY: WeekdayNumbering = WeekdayNumbering::ZeroSunday;
  const DOM_DOW: DomDowRule = DomDowRule::Union;
  const QUESTION_MARK: QuestionMark = QuestionMark::Wildcard;
  const RANGES: RangePolicy = RangePolicy::Ascending;
  const MODIFIERS: bool = false;
  const MACROS: bool = true;
  const REBOOT: bool = false;
  const EVERY: bool = true;
  const OPEN_ENDED_STEP: bool = true;
}

const _: () = {
  assert!(<Vixie as sealed::Sealed>::INDEX < DIALECT_COUNT);
  assert!(<Quartz as sealed::Sealed>::INDEX < DIALECT_COUNT);
  assert!(<Robfig as sealed::Sealed>::INDEX < DIALECT_COUNT);
};
