//! The year field, which is the only one that is not a bitset of sixty-odd values.
//!
//! Years are `[u128; N]` over a 1970 epoch, and `N` is a parameter rather than a number
//! this crate picked. One word was tempting on the ground that no schedule needs more
//! than 128 years, which is true of *use* and false of the *declared* range: Vixie's
//! reference year field is `1970..=2100` and Quartz's is `1970..=2099`, so one word is
//! three short of the first and two short of the second.
//!
//! Making it a parameter moves the range from something the implementation quietly
//! decided into something the type states. A caller who needs 2098 asks for it.

use crate::{
  error::ErrorKind,
  field::{Unrepresentable, ValueSink},
};

#[cfg(test)]
mod tests;

/// The first year any `Years<N>` can represent.
pub const EPOCH: u16 = 1970;

/// How many years one `u128` holds.
const YEARS_PER_WORD: usize = 128;

/// A set of years, `[u128; N]` over the [`EPOCH`].
///
/// | `N` | range      | size     |
/// |-----|------------|----------|
/// | 1   | `1970..=2097` | 16 bytes |
/// | 2   | `1970..=2225` | 32 bytes |
///
/// A year beyond the range gets [`ErrorKind::YearNotRepresentable`], which names the `N`
/// that would hold it — never a generic invalid-year. A user who meets a flat rejection
/// of legal cron concludes the parser is wrong, and they are half right.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Years<const N: usize = 1> {
  words: [u128; N],
}

impl<const N: usize> Default for Years<N> {
  #[inline(always)]
  fn default() -> Self {
    Self::new()
  }
}

impl<const N: usize> Years<N> {
  /// The first year this set can represent.
  pub const EPOCH: u16 = EPOCH;

  /// The last year this set can represent.
  ///
  /// `Years::<0>::MAX` is one *below* the epoch, which is how an empty range spells
  /// itself: there is no year at or below it that the set can hold either.
  pub const MAX: u16 = {
    let span = YEARS_PER_WORD.saturating_mul(N);
    let last = (EPOCH as usize).saturating_add(span).saturating_sub(1);
    // `Ord::min` is not callable in a const on the MSRV, so the clamp is written out.
    if last > u16::MAX as usize {
      u16::MAX
    } else {
      last as u16
    }
  };

  /// An empty set.
  #[must_use]
  #[inline(always)]
  pub const fn new() -> Self {
    Self { words: [0; N] }
  }

  /// Adds a year.
  ///
  /// Two failures, deliberately distinct. A year below [`EPOCH`] is
  /// [`ErrorKind::YearBelowEpoch`] and no `N` would help. A year above [`Self::MAX`] is
  /// [`ErrorKind::YearNotRepresentable`] and names the `N` that would.
  pub fn insert(&mut self, year: u16) -> Result<(), ErrorKind> {
    let Some(offset) = year.checked_sub(EPOCH) else {
      return Err(ErrorKind::YearBelowEpoch { year, epoch: EPOCH });
    };
    let offset = usize::from(offset);
    let word = offset / YEARS_PER_WORD;
    let bit = offset % YEARS_PER_WORD;

    match self.words.get_mut(word) {
      Some(slot) => {
        *slot |= 1u128 << bit;
        Ok(())
      }
      None => Err(ErrorKind::YearNotRepresentable {
        year,
        max_representable: Self::MAX,
        required_n: word.saturating_add(1),
      }),
    }
  }

  /// Whether the set holds this year.
  ///
  /// A year outside the representable range is simply absent; asking is not an error.
  #[must_use]
  pub fn contains(&self, year: u16) -> bool {
    let Some(offset) = year.checked_sub(EPOCH) else {
      return false;
    };
    let offset = offset as usize;
    match self.words.get(offset / YEARS_PER_WORD) {
      Some(word) => (word >> (offset % YEARS_PER_WORD)) & 1 == 1,
      None => false,
    }
  }

  /// Whether the set holds no years at all.
  #[must_use]
  #[inline(always)]
  pub fn is_empty(&self) -> bool {
    self.words.iter().all(|word| *word == 0)
  }
}

impl<const N: usize> ValueSink for Years<N> {
  /// This set really can be too narrow for a legal year — `N` is the caller's choice —
  /// so its failure is the real [`Unrepresentable`], and the deferral in `parse_field`
  /// stays live for year fields. That is the asymmetry with `Mask`, whose 64 bits cover
  /// everything its fields can produce and whose failure is therefore `Infallible`.
  type Failure = Unrepresentable;

  /// Both failures this can report are about *this* set's width and not about the
  /// expression: the year arrived having already been checked against the dialect's own
  /// bounds. [`ErrorKind::YearBelowEpoch`] is unreachable while every dialect's floor is
  /// at or above [`EPOCH`], and it is an [`Unrepresentable`] anyway — a dialect that
  /// declared an earlier floor would get the same deferral as a year past the ceiling,
  /// without that being a second decision.
  fn insert(&mut self, value: u32) -> Result<(), Unrepresentable> {
    // No dialect declares a year field wider than `u16`, so the fallback is
    // unreachable today; it exists so that the sink cannot be made to panic by a
    // dialect added later.
    let Ok(year) = u16::try_from(value) else {
      return Err(Unrepresentable::new(ErrorKind::ValueOutOfRange {
        value,
        min: EPOCH as u32,
        max: Self::MAX as u32,
      }));
    };
    Years::insert(self, year).map_err(Unrepresentable::new)
  }

  #[inline(always)]
  fn clear(&mut self) {
    self.words = [0; N];
  }
}
