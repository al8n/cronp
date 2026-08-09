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
  pub const fn new(start: usize, end: usize) -> Self {
    Self { start, end }
  }

  /// The first byte of the offending text.
  #[must_use]
  pub const fn start(self) -> usize {
    self.start
  }

  /// One past the last byte of the offending text.
  ///
  /// Equal to [`Self::start`] when the error is that the expression ended too early,
  /// which is the one case with no text to point at.
  #[must_use]
  pub const fn end(self) -> usize {
    self.end
  }

  /// Whether the span covers no text.
  #[must_use]
  pub const fn is_empty(self) -> bool {
    self.start >= self.end
  }
}

impl From<Range<usize>> for Span {
  fn from(range: Range<usize>) -> Self {
    Self::new(range.start, range.end)
  }
}

impl From<Span> for Range<usize> {
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
  /// `L`, `W`, `LW`, `L-n` or `n#m` in a dialect that has no date predicates.
  ModifierNotSupported {
    /// The dialect that refused it.
    dialect: &'static str,
  },
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
      ErrorKind::ModifierNotSupported { dialect } => {
        write!(f, "{dialect} has no `L`, `W` or `#` predicates")
      }
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
