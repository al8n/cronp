//! `@every <duration>` — the one place a cron expression denotes a length of time.
//!
//! The duration is not cron syntax and is deliberately not part of the token grammar.
//! `1h30m` would lex as a number, an unknown letter, a number and another unknown
//! letter, and teaching the cron lexer about `ns`, `ms` and `µs` would make it worse at
//! the job it exists for. It gets its own scanner over the text after the macro, and the
//! byte offsets it reports are offsets into the whole expression.

use core::time::Duration;

use crate::error::{ErrorKind, ParseError, Span};

#[cfg(test)]
mod tests;

/// A billion.
const NANOS_PER_SECOND: u128 = 1_000_000_000;

/// The largest scale a fractional part is accumulated at.
///
/// Ten to the eighteenth: past that the digits cannot change a nanosecond count, and
/// letting the scale keep growing would eventually overflow the accumulator.
const MAX_FRACTION_SCALE: u128 = 1_000_000_000_000_000_000;

/// Parses a Go-style duration, with `base` the offset of `text` in the whole expression.
pub(crate) fn parse(text: &str, base: usize) -> Result<Duration, ParseError> {
  let bytes = text.as_bytes();
  let whole = Span::new(base, base.saturating_add(bytes.len()));

  if bytes.is_empty() {
    return Err(ParseError::new(ErrorKind::EmptyDuration, whole));
  }

  let mut index = 0usize;
  let mut total: u128 = 0;
  let mut components = 0usize;

  while index < bytes.len() {
    let component = index;

    let (value, digits) = scan_digits(bytes, index);
    index = digits;
    let had_integer = index > component;

    let mut fraction: u128 = 0;
    let mut scale: u128 = 1;
    let mut had_fraction = false;
    if bytes.get(index) == Some(&b'.') {
      index = index.saturating_add(1);
      while let Some(digit) = bytes.get(index).and_then(digit_of) {
        had_fraction = true;
        if scale < MAX_FRACTION_SCALE {
          fraction = fraction.saturating_mul(10).saturating_add(digit);
          scale = scale.saturating_mul(10);
        }
        index = index.saturating_add(1);
      }
    }

    if !had_integer && !had_fraction {
      return Err(ParseError::new(
        ErrorKind::MalformedDuration,
        Span::new(base.saturating_add(component), whole.end()),
      ));
    }

    let unit_start = index;
    while let Some(byte) = bytes.get(index) {
      if byte.is_ascii_digit() || *byte == b'.' {
        break;
      }
      index = index.saturating_add(1);
    }
    let unit_span = Span::new(base.saturating_add(unit_start), base.saturating_add(index));
    if index == unit_start {
      return Err(ParseError::new(ErrorKind::DurationMissingUnit, unit_span));
    }
    let Some(unit) = text.get(unit_start..index) else {
      return Err(ParseError::new(ErrorKind::MalformedDuration, unit_span));
    };
    let Some(nanos_per_unit) = unit_nanos(unit) else {
      return Err(ParseError::new(ErrorKind::UnknownDurationUnit, unit_span));
    };

    let component_span = Span::new(base.saturating_add(component), base.saturating_add(index));
    let overflow = || ParseError::new(ErrorKind::DurationOverflow, component_span);

    let integral = value.checked_mul(nanos_per_unit).ok_or_else(overflow)?;
    let fractional = fraction
      .checked_mul(nanos_per_unit)
      .ok_or_else(overflow)?
      .checked_div(scale)
      .ok_or_else(overflow)?;
    total = total
      .checked_add(integral)
      .and_then(|sum| sum.checked_add(fractional))
      .ok_or_else(overflow)?;

    components = components.saturating_add(1);
  }

  if components == 0 {
    return Err(ParseError::new(ErrorKind::EmptyDuration, whole));
  }
  if total == 0 {
    return Err(ParseError::new(ErrorKind::ZeroDuration, whole));
  }

  let seconds = total / NANOS_PER_SECOND;
  let Ok(seconds) = u64::try_from(seconds) else {
    return Err(ParseError::new(ErrorKind::DurationOverflow, whole));
  };
  let nanos = (total % NANOS_PER_SECOND) as u32;
  Ok(Duration::new(seconds, nanos))
}

/// Reads a run of decimal digits, returning its value and the index after it.
fn scan_digits(bytes: &[u8], from: usize) -> (u128, usize) {
  let mut index = from;
  let mut value: u128 = 0;
  while let Some(digit) = bytes.get(index).and_then(digit_of) {
    // Saturating rather than checked: a digit run long enough to saturate is far past
    // any duration `Duration::new` could hold, and the overflow is caught below when
    // the value is scaled by its unit.
    value = value.saturating_mul(10).saturating_add(digit);
    index = index.saturating_add(1);
  }
  (value, index)
}

fn digit_of(byte: &u8) -> Option<u128> {
  byte.is_ascii_digit().then(|| u128::from(byte - b'0'))
}

/// How many nanoseconds one of the unit is.
fn unit_nanos(unit: &str) -> Option<u128> {
  Some(match unit {
    "ns" => 1,
    // Go writes microseconds three ways: ASCII, MICRO SIGN and GREEK SMALL LETTER MU.
    "us" | "\u{b5}s" | "\u{3bc}s" => 1_000,
    "ms" => 1_000_000,
    "s" => NANOS_PER_SECOND,
    "m" => 60 * NANOS_PER_SECOND,
    "h" => 3_600 * NANOS_PER_SECOND,
    _ => return None,
  })
}
