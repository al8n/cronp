//! One error type, no allocation, spans as byte offsets.

use core::{fmt, ops::Range};

/// Which field of an expression an error came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum FieldKind {
  /// The seconds field, present only in the six- and seven-field dialects.
  Second,
  /// The minutes field.
  Minute,
  /// The hours field.
  Hour,
  /// The day-of-month field.
  DayOfMonth,
  /// The month field.
  Month,
  /// The day-of-week field.
  DayOfWeek,
  /// The trailing year field, present only in Quartz.
  Year,
}

impl FieldKind {
  /// The field's name, as it appears in a message.
  #[must_use]
  pub const fn as_str(self) -> &'static str {
    match self {
      Self::Second => "second",
      Self::Minute => "minute",
      Self::Hour => "hour",
      Self::DayOfMonth => "day of month",
      Self::Month => "month",
      Self::DayOfWeek => "day of week",
      Self::Year => "year",
    }
  }
}

impl fmt::Display for FieldKind {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str(self.as_str())
  }
}

/// A half-open byte range into the expression that was parsed.
///
/// Byte offsets rather than a borrowed slice: an error outlives the borrow of its input
/// and must not need an allocation to say where it happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
  start: usize,
  end: usize,
}

impl Span {
  /// A span covering `start..end`.
  #[must_use]
  #[inline(always)]
  pub const fn new(start: usize, end: usize) -> Self {
    Self { start, end }
  }

  /// The first byte of the offending text.
  #[must_use]
  #[inline(always)]
  pub const fn start(self) -> usize {
    self.start
  }

  /// One past the last byte of the offending text.
  ///
  /// Equal to [`Self::start`] when the error is that the expression ended too early,
  /// which is the one case with no text to point at.
  #[must_use]
  #[inline(always)]
  pub const fn end(self) -> usize {
    self.end
  }

  /// Whether the span covers no text.
  #[must_use]
  #[inline(always)]
  pub const fn is_empty(self) -> bool {
    self.start >= self.end
  }
}

impl From<Range<usize>> for Span {
  #[inline(always)]
  fn from(range: Range<usize>) -> Self {
    Self::new(range.start, range.end)
  }
}

impl From<Span> for Range<usize> {
  #[inline(always)]
  fn from(span: Span) -> Self {
    span.start..span.end
  }
}

impl fmt::Display for Span {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "{}..{}", self.start, self.end)
  }
}

/// Why an expression could not be parsed.
///
/// `#[non_exhaustive]`: dialect support grows, and a caller that matches every variant
/// today should keep compiling when it does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ErrorKind {
  /// A byte that begins no token in any dialect.
  UnexpectedCharacter,
  /// A run of digits too long to be a value in any field.
  NumberTooLarge,
  /// A token that cannot appear at this point.
  UnexpectedToken,
  /// The expression ended in the middle of something.
  UnexpectedEnd,
  /// A value the field does not admit.
  ValueOutOfRange {
    /// The value as written.
    value: u32,
    /// The lowest value the field admits, in the dialect's own numbering.
    min: u32,
    /// The highest value the field admits, in the dialect's own numbering.
    max: u32,
  },
  /// A month or weekday name this field does not know.
  UnknownName,
  /// A range whose first value is after its last.
  ReversedRange {
    /// The value before the hyphen.
    start: u32,
    /// The value after it.
    end: u32,
  },
  /// A step of zero, which would select no values and never terminate a walk.
  ZeroStep,
  /// `5/15` in a dialect that requires a range or `*` before the step.
  OpenEndedStepNotSupported {
    /// The dialect that refused it.
    dialect: &'static str,
  },
  /// `?` in a dialect that has no `?`.
  QuestionMarkNotSupported {
    /// The dialect that refused it.
    dialect: &'static str,
  },
  /// `?` in a field other than day-of-month or day-of-week.
  QuestionMarkNotValidHere,
  /// `?` written as one item of a list, in a dialect where it means the whole field.
  ///
  /// Only dialects whose `?` means "no specific value" refuse this. Where `?` is
  /// merely another spelling of `*` it is an ordinary list item, and this is not
  /// raised.
  QuestionMarkMustBeAlone {
    /// The dialect that refused it.
    dialect: &'static str,
  },
  /// `L`, `W`, `LW`, `L-n` or `n#m` in a dialect that has no date predicates.
  ModifierNotSupported {
    /// The dialect that refused it.
    dialect: &'static str,
  },
  /// A date predicate in a field that has no such predicate.
  ///
  /// `nW` belongs to day-of-month and `n#m` to day-of-week; neither works in the other.
  ModifierNotValidHere,
  /// A date predicate written as one entry of a list.
  ///
  /// A predicate is not a set member, so it cannot be one alternative among several:
  /// it has to be the whole field.
  ModifierMustBeAlone,
  /// The expression was empty or only whitespace.
  EmptyExpression,
  /// The expression has a number of fields the dialect does not take.
  WrongFieldCount {
    /// How many whitespace-separated fields were found.
    found: usize,
    /// The fewest the dialect takes.
    min: usize,
    /// The most the dialect takes.
    max: usize,
    /// The dialect that refused it.
    dialect: &'static str,
  },
  /// Something followed an expression that was already complete.
  TrailingInput,
  /// An `@` nickname no dialect defines.
  UnknownMacro,
  /// A nickname macro in a dialect that has none.
  MacroNotSupported {
    /// The dialect that refused it.
    dialect: &'static str,
  },
  /// `@reboot` in a dialect that has no such nickname.
  RebootNotSupported {
    /// The dialect that refused it.
    dialect: &'static str,
  },
  /// `@every` in a dialect that has no such nickname.
  EveryNotSupported {
    /// The dialect that refused it.
    dialect: &'static str,
  },
  /// Neither day-of-month nor day-of-week was `?` in a dialect that requires one.
  QuestionMarkRequired {
    /// The dialect that requires it.
    dialect: &'static str,
  },
  /// Both day-of-month and day-of-week were `?`, leaving neither field to fire on.
  QuestionMarkInBothDayFields {
    /// The dialect that refused it.
    dialect: &'static str,
  },
  /// `@every` with no duration after it.
  EmptyDuration,
  /// A duration component that is neither a number nor a unit.
  MalformedDuration,
  /// A duration component with a value but no unit.
  DurationMissingUnit,
  /// A duration unit that is not one of `ns`, `us`, `ms`, `s`, `m` or `h`.
  UnknownDurationUnit,
  /// A duration too long to represent.
  DurationOverflow,
  /// A period of zero, which would fire without advancing.
  ZeroDuration,
  /// `H` in a dialect that has no hashed values.
  HashedValueNotSupported {
    /// The dialect that refused it.
    dialect: &'static str,
  },
  /// `H` in a parse that was given no seed.
  ///
  /// The dialect admits `H`, but the value it stands for is chosen by hashing a seed and
  /// no seed was supplied. [`Schedule::parse_with`](crate::Schedule::parse_with) is the
  /// entry point that carries one.
  HashedValueNeedsSeed,
  /// An expression ended with a timezone name in a dialect that takes none.
  TimezoneNotSupported {
    /// The dialect that refused it.
    dialect: &'static str,
  },
  /// A trailing field that cannot be an IANA timezone name.
  ///
  /// A shape check over the bytes, not a lookup: the name is checked for the characters
  /// an IANA identifier is made of and nothing more, because whether the name *exists* is
  /// a question for the database, which the parsing tier does not have.
  MalformedTimezone,
  /// A year below the epoch this crate counts from.
  YearBelowEpoch {
    /// The year as written.
    year: u16,
    /// The first year any `N` can represent.
    epoch: u16,
  },
  /// A year that is legal in the dialect but outside the range this `N` represents.
  ///
  /// Distinct from [`Self::ValueOutOfRange`] on purpose. The year is not wrong; the
  /// schedule was instantiated too narrow to hold it, and the message says which `N`
  /// would. A flat rejection of legal cron teaches a user that the parser is broken,
  /// and they would be half right.
  YearNotRepresentable {
    /// The year as written.
    year: u16,
    /// The last year the current `N` represents.
    max_representable: u16,
    /// The smallest `N` that would hold `year`.
    required_n: usize,
  },
}

/// Why an expression could not be parsed, and where.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ParseError {
  kind: ErrorKind,
  span: Span,
  field: Option<FieldKind>,
}

impl ParseError {
  /// An error with no field attribution — one raised before the fields were split up.
  #[must_use]
  pub(crate) const fn new(kind: ErrorKind, span: Span) -> Self {
    Self {
      kind,
      span,
      field: None,
    }
  }

  /// Attributes this error to a field.
  #[must_use]
  pub(crate) const fn in_field(mut self, field: FieldKind) -> Self {
    self.field = Some(field);
    self
  }

  /// What went wrong.
  #[must_use]
  pub const fn kind(&self) -> &ErrorKind {
    &self.kind
  }

  /// Where in the input it went wrong.
  #[must_use]
  pub const fn span(&self) -> Span {
    self.span
  }

  /// Which field it went wrong in, when the parser had got far enough to know.
  #[must_use]
  pub const fn field(&self) -> Option<FieldKind> {
    self.field
  }
}

impl fmt::Display for ParseError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    if let Some(field) = self.field {
      write!(f, "{field} field at {}: ", self.span)?;
    } else {
      write!(f, "at {}: ", self.span)?;
    }

    match self.kind {
      ErrorKind::UnexpectedCharacter => f.write_str("unexpected character"),
      ErrorKind::NumberTooLarge => f.write_str("number too large"),
      ErrorKind::UnexpectedToken => f.write_str("unexpected token"),
      ErrorKind::UnexpectedEnd => f.write_str("expression ended too early"),
      ErrorKind::ValueOutOfRange { value, min, max } => {
        write!(f, "{value} is outside {min}..={max}")
      }
      ErrorKind::UnknownName => f.write_str("not a name this field knows"),
      ErrorKind::ReversedRange { start, end } => {
        write!(f, "range {start}-{end} runs backwards")
      }
      ErrorKind::ZeroStep => f.write_str("a step of zero selects nothing"),
      ErrorKind::OpenEndedStepNotSupported { dialect } => write!(
        f,
        "{dialect} needs a range or `*` before a step, not a bare value"
      ),
      ErrorKind::QuestionMarkNotSupported { dialect } => {
        write!(f, "{dialect} has no `?`")
      }
      ErrorKind::QuestionMarkNotValidHere => {
        f.write_str("`?` belongs only in day-of-month or day-of-week")
      }
      ErrorKind::QuestionMarkMustBeAlone { dialect } => write!(
        f,
        "in {dialect} `?` means the whole field is unspecified, so it cannot be \
         one item of a list"
      ),
      ErrorKind::ModifierNotSupported { dialect } => {
        write!(f, "{dialect} has no `L`, `W` or `#` predicates")
      }
      ErrorKind::ModifierNotValidHere => {
        f.write_str("this predicate does not belong in this field")
      }
      ErrorKind::ModifierMustBeAlone => {
        f.write_str("a date predicate has to be the whole field, not one item of a list")
      }
      ErrorKind::EmptyExpression => f.write_str("the expression is empty"),
      ErrorKind::WrongFieldCount {
        found,
        min,
        max,
        dialect,
      } => {
        if min == max {
          write!(f, "{dialect} takes {min} fields, not {found}")
        } else {
          write!(f, "{dialect} takes {min} or {max} fields, not {found}")
        }
      }
      ErrorKind::TrailingInput => f.write_str("the expression was already complete here"),
      ErrorKind::UnknownMacro => f.write_str("not a nickname any dialect defines"),
      ErrorKind::MacroNotSupported { dialect } => {
        write!(f, "{dialect} has no nickname macros")
      }
      ErrorKind::RebootNotSupported { dialect } => write!(f, "{dialect} has no `@reboot`"),
      ErrorKind::EveryNotSupported { dialect } => write!(f, "{dialect} has no `@every`"),
      ErrorKind::QuestionMarkRequired { dialect } => write!(
        f,
        "{dialect} needs `?` in exactly one of day-of-month and day-of-week, \
         and neither has it"
      ),
      ErrorKind::QuestionMarkInBothDayFields { dialect } => write!(
        f,
        "{dialect} needs `?` in exactly one of day-of-month and day-of-week, \
         and both have it"
      ),
      ErrorKind::EmptyDuration => f.write_str("`@every` needs a duration after it"),
      ErrorKind::MalformedDuration => f.write_str("not a duration"),
      ErrorKind::DurationMissingUnit => {
        f.write_str("a duration value needs a unit: ns, us, ms, s, m or h")
      }
      ErrorKind::UnknownDurationUnit => {
        f.write_str("not a duration unit: expected ns, us, ms, s, m or h")
      }
      ErrorKind::DurationOverflow => f.write_str("the duration is too long to represent"),
      ErrorKind::ZeroDuration => f.write_str("a period of zero never advances"),
      ErrorKind::HashedValueNotSupported { dialect } => {
        write!(f, "{dialect} has no `H`")
      }
      ErrorKind::HashedValueNeedsSeed => f.write_str(
        "`H` stands for a value chosen by hashing a seed, and this parse was given none: \
         use `parse_with`",
      ),
      ErrorKind::TimezoneNotSupported { dialect } => {
        write!(f, "{dialect} expressions carry no timezone")
      }
      ErrorKind::MalformedTimezone => f.write_str("not an IANA timezone name"),
      ErrorKind::YearBelowEpoch { year, epoch } => {
        write!(
          f,
          "{year} is before the {epoch} epoch this crate counts from"
        )
      }
      ErrorKind::YearNotRepresentable {
        year,
        max_representable,
        required_n,
      } => write!(
        f,
        "{year} is legal cron but this schedule represents only up to \
         {max_representable}; instantiate it with N = {required_n}"
      ),
    }
  }
}

impl core::error::Error for ParseError {}
