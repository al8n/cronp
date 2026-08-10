#![doc = include_str!("../README.md")]
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
