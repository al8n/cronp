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
//!
//! ```
//! use cronp::{Quartz, Schedule, Vixie};
//!
//! // Five fields, the shape `crontab(5)` takes.
//! let nightly: Schedule<Vixie> = Schedule::parse("30 2 * * *")?;
//!
//! // Six or seven, a leading seconds field, and `?` in one of the two day fields.
//! let quartz = Schedule::<Quartz>::parse("0 15 10 ? * MON-FRI")?;
//!
//! // Five fields is not Quartz, and the error says so rather than failing on a field.
//! assert!(Schedule::<Quartz>::parse("30 2 * * *").is_err());
//! # let _ = (nightly, quartz);
//! # Ok::<(), cronp::ParseError>(())
//! ```
//!
//! The year range is a parameter of the type rather than a number this crate chose.
//! `Schedule<D>` is `Schedule<D, 1>`, which represents `1970..=2097`; a caller who needs
//! a year beyond that asks for it, and the rejection says which `N` would hold it.
//!
//! ```
//! use cronp::{ErrorKind, Quartz, Schedule};
//!
//! let error = Schedule::<Quartz>::parse("0 0 0 ? * * 2098").unwrap_err();
//! assert_eq!(
//!   *error.kind(),
//!   ErrorKind::YearNotRepresentable {
//!     year: 2098,
//!     max_representable: 2097,
//!     required_n: 2,
//!   },
//! );
//!
//! assert!(Schedule::<Quartz, 2>::parse("0 0 0 ? * * 2098").is_ok());
//! ```

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

// The `alloc` and `std` features are declared but reach nothing yet: this tier has no
// owned diagnostics to render. They stay in the manifest because they are the axis the
// crate is built around and because downstream crates routinely forward a `std` feature
// to every dependency, so removing the names would break those builds for no gain. The
// `extern crate alloc;` that goes with them arrives with its first user; declaring it
// now would be an unused import, which `rust_2018_idioms` rightly rejects.

// Unit tests run inside a test binary that links `std` no matter what this crate
// declares. Naming that here keeps `std::vec::Vec` available to test modules without
// giving the library itself any route to it.
#[cfg(test)]
extern crate std;

mod date;
mod dialect;
mod error;
mod every;
mod field;
mod modifier;
mod schedule;
mod token;
mod years;

pub use date::{CivilDateTime, DateComponent, DateError, Weekday, days_in_month, is_leap_year};
pub use dialect::{
  Dialect, DomDowRule, Quartz, QuestionMark, RangePolicy, Robfig, Vixie, WeekdayNumbering,
  YearField,
};
pub use error::{ErrorKind, FieldKind, ParseError, Span};
pub use modifier::{DayOfMonthModifier, DayOfWeekModifier};
pub use schedule::{Calendar, Schedule};
pub use years::{EPOCH, Years};
