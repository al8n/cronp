#![allow(
  clippy::indexing_slicing,
  clippy::unwrap_used,
  clippy::expect_used,
  clippy::panic
)]

use super::{Years, EPOCH};
use crate::{
  dialect::Quartz,
  error::{ErrorKind, FieldKind},
  field::{parse_field, FieldSpec, ValueSink as _},
  token::Cursor,
};

// ---------------------------------------------------------------------------
// The range each N represents.
//
// The fixtures here are the whole justification for the const generic. 2050 would
// pass at every N and prove nothing; 2097, 2098 and 2226 are the values that sit
// exactly on a word boundary.
// ---------------------------------------------------------------------------

#[test]
fn one_word_holds_1970_through_2097() {
  assert_eq!(EPOCH, 1970);
  assert_eq!(Years::<1>::MAX, 2097);
  assert_eq!(Years::<2>::MAX, 2225);
  assert_eq!(Years::<3>::MAX, 2353);
}

#[test]
fn both_ends_of_the_default_range_are_accepted() {
  let mut years = Years::<1>::new();
  years.insert(1970).unwrap();
  years.insert(2097).unwrap();
  assert!(years.contains(1970));
  assert!(years.contains(2097));
  assert!(!years.contains(1971));
  assert!(!years.contains(2096));
}

#[test]
fn the_year_after_the_default_range_names_the_n_that_would_hold_it() {
  let mut years = Years::<1>::new();
  assert_eq!(
    years.insert(2098),
    Err(ErrorKind::YearNotRepresentable {
      year: 2098,
      max_representable: 2097,
      required_n: 2,
    }),
    "2098 is legal Quartz; the rejection must say which N holds it, not that the \
     year is invalid"
  );
  assert!(!years.contains(2098));
}

#[test]
fn two_words_hold_the_year_one_word_could_not() {
  let mut years = Years::<2>::new();
  years.insert(2098).unwrap();
  assert!(years.contains(2098));

  // 2097 is the last bit of the first word and 2098 the first bit of the second.
  // Setting one must not set the other: this is the assertion that catches an
  // off-by-one in the word index.
  assert!(!years.contains(2097));
  let mut other = Years::<2>::new();
  other.insert(2097).unwrap();
  assert!(other.contains(2097));
  assert!(!other.contains(2098));

  years.insert(2225).unwrap();
  assert!(years.contains(2225));
}

#[test]
fn the_year_after_the_two_word_range_names_three() {
  let mut years = Years::<2>::new();
  assert_eq!(
    years.insert(2226),
    Err(ErrorKind::YearNotRepresentable {
      year: 2226,
      max_representable: 2225,
      required_n: 3,
    })
  );
}

#[test]
fn a_year_below_the_epoch_is_a_different_failure() {
  let mut years = Years::<1>::new();
  assert_eq!(
    years.insert(1969),
    Err(ErrorKind::YearBelowEpoch {
      year: 1969,
      epoch: 1970
    }),
    "before the epoch is not the same failure as beyond this N, and widening N \
     would not help"
  );
  assert_eq!(
    years.insert(0),
    Err(ErrorKind::YearBelowEpoch {
      year: 0,
      epoch: 1970
    })
  );
  assert!(!years.contains(1969));
}

#[test]
fn a_zero_word_set_represents_nothing_without_panicking() {
  let mut years = Years::<0>::new();
  assert_eq!(Years::<0>::MAX, 1969, "one below the epoch: an empty range");
  assert_eq!(
    years.insert(1970),
    Err(ErrorKind::YearNotRepresentable {
      year: 1970,
      max_representable: 1969,
      required_n: 1,
    })
  );
  assert!(!years.contains(1970));
  assert!(years.is_empty());
}

#[test]
fn an_empty_set_contains_nothing_and_a_filled_one_is_not_empty() {
  let mut years = Years::<1>::new();
  assert!(years.is_empty());
  for year in [1970, 2000, 2097] {
    assert!(!years.contains(year));
  }
  years.insert(2000).unwrap();
  assert!(!years.is_empty());
}

#[test]
fn the_set_is_exactly_sixteen_bytes_per_word() {
  assert_eq!(core::mem::size_of::<Years<1>>(), 16);
  assert_eq!(core::mem::size_of::<Years<2>>(), 32);
  assert_eq!(core::mem::size_of::<Years<0>>(), 0);
}

// ---------------------------------------------------------------------------
// As a field sink: the year field reuses the whole grammar.
// ---------------------------------------------------------------------------

fn parse_year<const N: usize>(input: &str) -> Result<(Years<N>, bool), ErrorKind> {
  let mut cursor = Cursor::new(input);
  let mut years = Years::<N>::new();
  let spec = FieldSpec::year::<Quartz>().expect("Quartz has a year field");
  match parse_field::<Quartz, _>(&mut cursor, spec, &mut years) {
    Ok(outcome) => Ok((years, outcome.restricted)),
    Err(error) => {
      assert_eq!(error.field(), Some(FieldKind::Year));
      Err(*error.kind())
    }
  }
}

#[test]
fn the_year_field_takes_lists_ranges_and_steps() {
  let (years, restricted) = parse_year::<1>("2020-2024").unwrap();
  assert!(restricted);
  for year in 2020..=2024 {
    assert!(years.contains(year), "{year}");
  }
  assert!(!years.contains(2019));
  assert!(!years.contains(2025));

  let (years, _) = parse_year::<1>("2020,2030,2040").unwrap();
  assert!(years.contains(2020) && years.contains(2030) && years.contains(2040));
  assert!(!years.contains(2025));

  let (years, _) = parse_year::<1>("2020-2030/5").unwrap();
  assert!(years.contains(2020) && years.contains(2025) && years.contains(2030));
  assert!(!years.contains(2021));
}

#[test]
fn a_written_year_beyond_this_n_is_rejected_by_name() {
  assert_eq!(
    parse_year::<1>("2098"),
    Err(ErrorKind::YearNotRepresentable {
      year: 2098,
      max_representable: 2097,
      required_n: 2,
    })
  );
  let (years, _) = parse_year::<2>("2098").unwrap();
  assert!(years.contains(2098));
}

#[test]
fn a_year_outside_the_dialect_is_a_range_error_not_a_width_error() {
  // Quartz declares 1970..=2099. 2100 is not legal Quartz at any N, so widening the
  // schedule would not help and the message must not suggest that it would.
  assert_eq!(
    parse_year::<2>("2100"),
    Err(ErrorKind::ValueOutOfRange {
      value: 2100,
      min: 1970,
      max: 2099,
    })
  );
}

#[test]
fn a_star_year_expands_only_as_far_as_the_set_can_hold() {
  // Quartz's year field admits 1970..=2099, but `Years<1>` stops at 2097. Expanding
  // `*` into the sink must not fail: the user placed no restriction, and refusing the
  // commonest expression in the dialect because of the width of the value it is
  // stored in would be the parser's problem, not the user's.
  let (years, restricted) = parse_year::<1>("*").expect("`*` must not overflow the set");
  assert!(!restricted, "`*` restricts nothing");
  assert!(years.contains(1970));
  assert!(years.contains(2097));

  let (years, _) = parse_year::<2>("*").unwrap();
  assert!(
    years.contains(2099),
    "at N = 2 the whole dialect range fits, so `*` reaches all of it"
  );
  assert!(
    !years.contains(2100),
    "`*` still stops at the dialect's own ceiling"
  );
}

#[test]
fn the_sinks_ceiling_is_what_narrows_the_wildcard() {
  // The mechanism, asserted directly: a mask never narrows anything, and a year set
  // narrows only when its own capacity is the smaller of the two.
  use crate::field::Mask;

  let mask = Mask::default();
  assert_eq!(mask.wildcard_ceiling(59), 59);

  let narrow = Years::<1>::new();
  assert_eq!(narrow.wildcard_ceiling(2099), 2097);

  let wide = Years::<2>::new();
  assert_eq!(wide.wildcard_ceiling(2099), 2099);
}

#[test]
fn the_rejection_message_names_the_n_that_would_hold_the_year() {
  // The message is the whole point of the distinct variant, so it is asserted rather
  // than left to a reader of the enum.
  let mut cursor = Cursor::new("2098");
  let mut years = Years::<1>::new();
  let spec = FieldSpec::year::<Quartz>().unwrap();
  let error =
    parse_field::<Quartz, _>(&mut cursor, spec, &mut years).expect_err("2098 is beyond Years<1>");
  let mut rendered = std::string::String::new();
  core::fmt::Write::write_fmt(&mut rendered, format_args!("{error}")).unwrap();
  assert_eq!(
    rendered,
    "year field at 0..4: 2098 is legal cron but this schedule represents only up to \
     2097; instantiate it with N = 2"
  );
}
