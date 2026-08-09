#![allow(
  clippy::indexing_slicing,
  clippy::unwrap_used,
  clippy::expect_used,
  clippy::panic
)]

use super::{EPOCH, Years};
use crate::{
  dialect::Quartz,
  error::{ErrorKind, FieldKind},
  field::{FieldSpec, parse_field},
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
fn an_unrestricted_year_field_writes_no_years_at_all() {
  // Quartz's year field admits 1970..=2099 and `Years<1>` stops at 2097, so any
  // attempt to write `*` down at N = 1 must either fail or lose two years. It does
  // neither, because it writes nothing: `*` means "no constraint", not "the set from
  // 1970 to whatever this sink happens to hold". With no bits there is nothing for a
  // storage ceiling to truncate and the width problem cannot arise.
  let (years, restricted) = parse_year::<1>("*").expect("`*` must not overflow the set");
  assert!(!restricted, "`*` restricts nothing");
  assert!(years.is_empty(), "and so writes nothing");

  let (years, restricted) = parse_year::<1>("*/1").expect("`*/1` is `*`");
  assert!(!restricted, "a stride of one narrows nothing");
  assert!(years.is_empty());

  // The same holds where the set is wide enough to have held the range, so the
  // behaviour is one rule rather than a special case for the narrow instantiation.
  let (years, restricted) = parse_year::<2>("*").unwrap();
  assert!(!restricted);
  assert!(years.is_empty());
}

#[test]
fn a_wildcard_beside_another_item_is_written_out_against_the_dialect() {
  // The moment a second item appears the field is a restriction, so the wildcard has
  // to be materialised — and against the *dialect's* ceiling, not the sink's. Quartz
  // reaches 2099, `Years<1>` reaches 2097, so this is the representability case and
  // must name the N rather than quietly dropping two years.
  for text in ["*,2025", "*/1,2025", "2025,*", "*,*"] {
    assert_eq!(
      parse_year::<1>(text),
      Err(ErrorKind::YearNotRepresentable {
        year: 2098,
        max_representable: 2097,
        required_n: 2,
      }),
      "{text}"
    );
  }

  // At N = 2 the whole dialect range fits, so the wildcard really does reach 2099.
  let (years, restricted) = parse_year::<2>("*,2025").expect("N = 2 holds 1970..=2099");
  assert!(restricted, "a list is a restriction");
  assert!(years.contains(1970));
  assert!(years.contains(2025));
  assert!(years.contains(2099));
  assert!(!years.contains(2100), "the dialect's ceiling still applies");
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

#[test]
fn a_stepped_year_wildcard_does_not_silently_truncate() {
  // A step above one is a real restriction, so the set is observable and has to be
  // built against the *dialect's* ceiling rather than the storage one. Quartz reaches
  // 2099 and `Years<1>` reaches 2097, and that gap is exactly the situation the
  // explicit-year path already reports.
  assert_eq!(
    parse_year::<1>("*/2"),
    Err(ErrorKind::YearNotRepresentable {
      year: 2098,
      max_representable: 2097,
      required_n: 2,
    }),
    "silently stopping at 2097 would make the schedule lie about which years it fires in"
  );

  let (years, restricted) = parse_year::<2>("*/2").expect("N = 2 reaches past 2099");
  assert!(restricted);
  assert!(years.contains(2098));
  assert!(
    !years.contains(2099),
    "2099 is not on a stride of two from 1970"
  );
}
