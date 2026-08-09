//! A no-std, no-alloc cron expression parser.
//!
//! # Tiers
//!
//! The default tier parses a cron expression, represents it in a fixed-size value
//! that never allocates, and answers whether a caller-supplied instant matches it.
//! It has no clock and does no calendar arithmetic.
//!
//! The `jiff` feature adds the calendar tier: next and previous occurrence. It stays
//! `no_std` and allocation-free; `jiff` is a *calendar* axis, not a *std* axis.
//!
//! # The dialect is a type
//!
//! Cron dialects disagree about the number of fields, about which digit means which
//! weekday, and about what it means to restrict the day of the month and the day of the
//! week at once. Those are semantic differences, so the dialect is a type parameter and
//! not a runtime tag: a `Schedule<Vixie>` cannot be passed where a `Schedule<Quartz>` is
//! expected.
//!
//! This does *not* mean the expression is checked at compile time. The input is a runtime
//! `&str`, so syntax errors are parse-time errors.

#![no_std]
#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(
  clippy::indexing_slicing,
  clippy::unwrap_used,
  clippy::expect_used,
  clippy::panic
)]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![cfg_attr(docsrs, allow(unused_attributes))]

#[cfg(feature = "alloc")]
extern crate alloc;

// Unit tests run inside a test binary that links `std` no matter what this crate
// declares. Naming that here keeps `std::vec::Vec` available to test modules without
// giving the library itself any route to it.
#[cfg(test)]
extern crate std;

mod dialect;
// TEMPORARY: the token enum has no consumer until the field parser lands in this same
// pull request. Removed there; a `dead_code` allowance must not outlive PR 1.
#[allow(dead_code)]
mod token;

pub use dialect::{
  Dialect, DomDowRule, Quartz, QuestionMark, Robfig, Vixie, WeekdayNumbering, YearField,
};
