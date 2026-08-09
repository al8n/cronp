#![allow(
  clippy::indexing_slicing,
  clippy::unwrap_used,
  clippy::expect_used,
  clippy::panic
)]

use core::time::Duration;

use super::parse;
use crate::error::ErrorKind;

fn ok(text: &str) -> Duration {
  parse(text, 0).unwrap_or_else(|e| panic!("{text:?} should parse: {e}"))
}

fn err(text: &str) -> ErrorKind {
  match parse(text, 0) {
    Ok(d) => panic!("{text:?} should not parse; got {d:?}"),
    Err(e) => *e.kind(),
  }
}

#[test]
fn every_unit_is_understood() {
  assert_eq!(ok("1ns"), Duration::from_nanos(1));
  assert_eq!(ok("1us"), Duration::from_micros(1));
  assert_eq!(ok("1\u{b5}s"), Duration::from_micros(1), "MICRO SIGN");
  assert_eq!(ok("1\u{3bc}s"), Duration::from_micros(1), "GREEK MU");
  assert_eq!(ok("1ms"), Duration::from_millis(1));
  assert_eq!(ok("1s"), Duration::from_secs(1));
  assert_eq!(ok("1m"), Duration::from_secs(60));
  assert_eq!(ok("1h"), Duration::from_secs(3600));
}

#[test]
fn components_add_up() {
  assert_eq!(ok("1h30m"), Duration::from_secs(5400));
  assert_eq!(ok("2h45m30s"), Duration::from_secs(2 * 3600 + 45 * 60 + 30));
  assert_eq!(ok("1m1s1ms1us1ns"), {
    Duration::from_secs(61) + Duration::from_nanos(1_001_001)
  });
  assert_eq!(
    ok("30m1h"),
    Duration::from_secs(5400),
    "Go does not require the components to descend, and neither does this"
  );
}

#[test]
fn fractions_scale_without_floating_point() {
  assert_eq!(ok("1.5h"), Duration::from_secs(5400));
  assert_eq!(ok("0.5s"), Duration::from_millis(500));
  assert_eq!(ok(".5s"), Duration::from_millis(500), "a bare fraction");
  assert_eq!(ok("1.000000001s"), Duration::new(1, 1));
  // Half a nanosecond truncates rather than rounding, as Go does — and a duration
  // that truncates to nothing *is* nothing, so it meets the zero rule rather than
  // becoming a period of zero nanoseconds that would fire without advancing.
  assert_eq!(err("0.0000000005s"), ErrorKind::ZeroDuration);
  assert_eq!(ok("1.0000000005s"), Duration::new(1, 0));
}

#[test]
fn a_large_but_representable_duration_survives() {
  // Just under a hundred thousand hours, which is far past anything a scheduler
  // wants and well inside what `Duration` holds.
  assert_eq!(ok("99999h"), Duration::from_secs(99_999 * 3600));
}

#[test]
fn malformed_durations_are_rejected_by_cause() {
  assert_eq!(err(""), ErrorKind::EmptyDuration);
  assert_eq!(err("1"), ErrorKind::DurationMissingUnit);
  assert_eq!(err("1h30"), ErrorKind::DurationMissingUnit);
  assert_eq!(err("1x"), ErrorKind::UnknownDurationUnit);
  assert_eq!(err("1hh"), ErrorKind::UnknownDurationUnit);
  assert_eq!(err("h"), ErrorKind::MalformedDuration);
  assert_eq!(
    err("-1h"),
    ErrorKind::MalformedDuration,
    "a negative period is not a schedule"
  );
  assert_eq!(err("0s"), ErrorKind::ZeroDuration);
  assert_eq!(err("0h0m0s"), ErrorKind::ZeroDuration);
  assert_eq!(
    err("99999999999999999999999999h"),
    ErrorKind::DurationOverflow
  );
}

#[test]
fn spans_are_offsets_into_the_whole_expression() {
  // `@every 1x` — the macro and its space are seven bytes, so the unit is at 8..9.
  let error = parse("1x", 7).expect_err("x is not a unit");
  assert_eq!(*error.kind(), ErrorKind::UnknownDurationUnit);
  assert_eq!(error.span().start(), 8);
  assert_eq!(error.span().end(), 9);

  let error = parse("", 7).expect_err("nothing is not a duration");
  assert_eq!(error.span().start(), 7);
  assert_eq!(error.span().end(), 7);
}

#[test]
fn no_input_makes_the_scanner_panic() {
  // A sweep over every prefix of a well-formed duration plus a corpus of junk. The
  // scanner must return, never panic, whatever it is handed.
  let corpus = [
    "1h30m45s",
    "..",
    "1..2s",
    "999999999999999999999999999999ns",
    "\u{b5}",
    "s1",
    "1e9s",
    "1 h",
    "\t",
    "1h ",
    "+1h",
  ];
  for text in corpus {
    for end in 0..=text.len() {
      if !text.is_char_boundary(end) {
        continue;
      }
      let prefix = text.get(..end).expect("a boundary");
      let _ = parse(prefix, 0);
    }
  }
}
