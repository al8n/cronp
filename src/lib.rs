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

// The `alloc` and `std` features reach no code of this crate's: there are no owned
// diagnostics to render. What they do reach is every optional dependency, through weak
// dependency features, so that `--features std,jiff` is jiff in its `std` mode rather
// than jiff quietly staying `no_std` inside a `std` build. That, and the fact that
// downstream crates routinely forward a `std` feature to every dependency, is why the
// names stay. The `extern crate alloc;` that would go with them arrives with its first
// user; declaring it now would be an unused import, which `rust_2018_idioms` rightly
// rejects.

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
  Cronexpr, Dialect, DomDowRule, Quartz, QuestionMark, RangePolicy, Robfig, Vixie,
  WeekdayNumbering, WildcardWitness, YearField,
};
pub use error::{ErrorKind, FieldKind, ParseError, Span};
pub use modifier::{DayOfMonthModifier, DayOfWeekModifier};
#[cfg(feature = "tz-static")]
pub use schedule::UnknownTimeZone;
pub use schedule::{Calendar, Schedule, ZonedSchedule};
pub use years::{EPOCH, Years};
