#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::panic)]

use std::{vec, vec::Vec};

use super::{
  Dialect, DomDowRule, Quartz, QuestionMark, RangePolicy, Robfig, Vixie, WeekdayNumbering,
  YearField, DIALECT_COUNT,
};

/// Everything a dialect declares, read off the trait in one place.
#[derive(Debug)]
struct DialectFacts {
  name: &'static str,
  min_fields: u8,
  max_fields: u8,
  weekday: WeekdayNumbering,
  dom_dow: DomDowRule,
  question_mark: QuestionMark,
  ranges: RangePolicy,
  year: YearField,
  modifiers: bool,
  macros: bool,
  reboot: bool,
  every: bool,
  open_ended_step: bool,
}

/// One row per dialect, read off the trait rather than restated.
///
/// `index` is the dialect's ordinal from the sealed trait. It is what lets the coverage
/// test below say *every* dialect rather than *these* dialects: an implementor whose
/// ordinal has no row here leaves an empty slot, and an implementor added without
/// raising `DIALECT_COUNT` fails the const assertion in `dialect.rs`.
fn facts<D: Dialect>() -> (usize, DialectFacts) {
  (
    D::INDEX,
    DialectFacts {
      name: D::NAME,
      min_fields: D::MIN_FIELDS,
      max_fields: D::MAX_FIELDS,
      weekday: D::WEEKDAY,
      dom_dow: D::DOM_DOW,
      question_mark: D::QUESTION_MARK,
      ranges: D::RANGES,
      year: D::YEAR,
      modifiers: D::MODIFIERS,
      macros: D::MACROS,
      reboot: D::REBOOT,
      every: D::EVERY,
      open_ended_step: D::OPEN_ENDED_STEP,
    },
  )
}

fn all() -> Vec<(usize, DialectFacts)> {
  vec![facts::<Vixie>(), facts::<Quartz>(), facts::<Robfig>()]
}

/// The facts of one dialect, looked up by name.
///
/// Assertions go through this rather than through `Vixie::MODIFIERS` and friends so
/// that they are comparisons of runtime values. Reading an associated constant directly
/// inside `assert!` folds the whole assertion to a constant, which clippy rejects and
/// which would in any case be checking the compiler rather than the dialect.
fn fact(name: &str) -> DialectFacts {
  all()
    .into_iter()
    .find(|(_, f)| f.name == name)
    .map(|(_, f)| f)
    .unwrap_or_else(|| panic!("no dialect named {name}"))
}

#[test]
fn every_dialect_has_a_row() {
  let rows = all();
  assert_eq!(
    rows.len(),
    DIALECT_COUNT,
    "the tests below say 'every dialect'; there are {DIALECT_COUNT} of them and \
     {} rows here",
    rows.len()
  );

  let mut slots: Vec<Option<&'static str>> = vec![None; DIALECT_COUNT];
  for (index, fact) in &rows {
    assert!(
      slots[*index].is_none(),
      "two dialects claim ordinal {index}"
    );
    slots[*index] = Some(fact.name);
  }
  assert!(
    slots.iter().all(Option::is_some),
    "a dialect ordinal has no row: {slots:?}"
  );
}

// ---------------------------------------------------------------------------
// Incompatibility 1: field count.
// ---------------------------------------------------------------------------

#[test]
fn field_counts_differ_across_dialects() {
  let (vixie, quartz, robfig) = (fact("Vixie"), fact("Quartz"), fact("Robfig"));

  assert_eq!((vixie.min_fields, vixie.max_fields), (5, 5));
  assert_eq!((robfig.min_fields, robfig.max_fields), (6, 6));
  assert_eq!(
    (quartz.min_fields, quartz.max_fields),
    (6, 7),
    "Quartz's year field is optional, so it accepts both widths"
  );

  assert_eq!(vixie.year, YearField::Absent);
  assert_eq!(robfig.year, YearField::Absent);
  assert_eq!(
    quartz.year,
    YearField::Optional {
      min: 1970,
      max: 2099
    }
  );

  // All three of the spec's field counts are represented; if a dialect is ever
  // added or reshaped, this is the assertion that notices.
  let widths: Vec<(u8, u8)> = all()
    .iter()
    .map(|(_, f)| (f.min_fields, f.max_fields))
    .collect();
  assert!(widths.iter().any(|w| w.0 == 5));
  assert!(widths.iter().any(|w| w.0 == 6 && w.1 == 6));
  assert!(widths.iter().any(|w| w.1 == 7));
}

// ---------------------------------------------------------------------------
// Incompatibility 2: day-of-week numbering.
//
// The interesting fixture is not "Wednesday". It is the digit that names a
// different day under each numbering.
// ---------------------------------------------------------------------------

#[test]
fn the_same_digit_means_different_days() {
  // Canonical numbering is 0 = Sunday .. 6 = Saturday.
  const SUNDAY: u8 = 0;
  const MONDAY: u8 = 1;
  const SATURDAY: u8 = 6;

  assert_eq!(Vixie::WEEKDAY.canonical(7), Some(SUNDAY));
  assert_eq!(
    Quartz::WEEKDAY.canonical(7),
    Some(SATURDAY),
    "7 is Sunday in Vixie and Saturday in Quartz — the same digit, six days apart"
  );

  assert_eq!(Vixie::WEEKDAY.canonical(1), Some(MONDAY));
  assert_eq!(
    Quartz::WEEKDAY.canonical(1),
    Some(SUNDAY),
    "1 is Monday in Vixie and Sunday in Quartz"
  );

  // Every digit 1..=6 disagrees by exactly one day; 0 and 7 are where the two
  // numberings disagree about what is even legal.
  for digit in 1..=6 {
    let vixie = Vixie::WEEKDAY.canonical(digit).unwrap();
    let quartz = Quartz::WEEKDAY.canonical(digit).unwrap();
    assert_eq!(vixie, quartz + 1, "digit {digit}");
  }

  assert_eq!(Vixie::WEEKDAY.canonical(0), Some(SUNDAY));
  assert_eq!(
    Quartz::WEEKDAY.canonical(0),
    None,
    "Quartz numbers 1..=7; 0 is not a weekday at all"
  );
  assert_eq!(
    Vixie::WEEKDAY.canonical(8),
    None,
    "Vixie accepts 7 as a second spelling of Sunday, and nothing above it"
  );
  assert_eq!(Quartz::WEEKDAY.canonical(8), None);

  assert_eq!(
    fact("Robfig").weekday,
    WeekdayNumbering::ZeroSunday,
    "the Go dialect follows Vixie's numbering even though it adds a seconds field"
  );
}

#[test]
fn names_mean_the_same_day_in_every_dialect() {
  // The digits disagree; the names do not. A dialect that changed this would be
  // unreadable, and the test says so rather than leaving it implied.
  for (_, fact) in all() {
    assert_eq!(
      WeekdayNumbering::canonical_name("SUN"),
      Some(0),
      "{}",
      fact.name
    );
    assert_eq!(
      WeekdayNumbering::canonical_name("sat"),
      Some(6),
      "{}",
      fact.name
    );
  }
  assert_eq!(WeekdayNumbering::canonical_name("MON"), Some(1));
  assert_eq!(WeekdayNumbering::canonical_name("JAN"), None);
}

// ---------------------------------------------------------------------------
// Incompatibility 3: day-of-month against day-of-week.
// ---------------------------------------------------------------------------

#[test]
fn dom_against_dow_is_a_semantic_difference() {
  let (vixie, quartz, robfig) = (fact("Vixie"), fact("Quartz"), fact("Robfig"));

  assert_eq!(
    vixie.dom_dow,
    DomDowRule::Union,
    "a historical quirk, not a bug: when both are restricted Vixie fires on either"
  );
  assert_eq!(robfig.dom_dow, DomDowRule::Union);
  assert_eq!(
    quartz.dom_dow,
    DomDowRule::Exclusive,
    "Quartz refuses the question rather than answering it, and demands `?`"
  );

  // The three dialects mean three different things by `?`, and the difference is not
  // cosmetic: Quartz's `?` is what tells the Exclusive rule which day field to ignore,
  // so it has to be the whole field. The Go dialect's is another spelling of `*` and
  // may be one item of a list, exactly as `cron` 0.17 reads it.
  assert_eq!(vixie.question_mark, QuestionMark::Forbidden);
  assert_eq!(robfig.question_mark, QuestionMark::Wildcard);
  assert_eq!(quartz.question_mark, QuestionMark::NoSpecificValue);

  assert!(!vixie.question_mark.is_supported());
  assert!(robfig.question_mark.is_supported() && quartz.question_mark.is_supported());

  assert_eq!(
    (
      vixie.question_mark.must_be_alone(),
      quartz.question_mark.must_be_alone(),
      robfig.question_mark.must_be_alone()
    ),
    (false, true, false),
    "only a `?` that means `no specific value` has to stand alone"
  );
}

// ---------------------------------------------------------------------------
// The rest of the surface each dialect gates.
// ---------------------------------------------------------------------------

#[test]
fn each_dialect_gates_its_own_extensions() {
  let (vixie, quartz, robfig) = (fact("Vixie"), fact("Quartz"), fact("Robfig"));

  assert_eq!(
    (vixie.modifiers, quartz.modifiers, robfig.modifiers),
    (false, true, false),
    "L, W, LW, #n and L-n are Quartz's alone"
  );
  assert_eq!(
    (vixie.every, quartz.every, robfig.every),
    (false, false, true),
    "@every is the Go dialect's alone"
  );
  assert_eq!(
    (vixie.reboot, quartz.reboot, robfig.reboot),
    (true, false, false),
    "@reboot is Vixie's alone, and it is legal Vixie"
  );
  assert_eq!(
    (vixie.macros, quartz.macros, robfig.macros),
    (true, false, true),
    "Quartz has no nickname macros"
  );
  assert_eq!(
    (
      vixie.open_ended_step,
      quartz.open_ended_step,
      robfig.open_ended_step
    ),
    (false, true, true),
    "`5/15` means `5-max/15` in Quartz and Go; Vixie requires a range or `*`"
  );
}

#[test]
fn a_backwards_range_is_a_dialect_difference() {
  let (vixie, quartz, robfig) = (fact("Vixie"), fact("Quartz"), fact("Robfig"));

  assert_eq!(
    (vixie.ranges, quartz.ranges, robfig.ranges),
    (
      RangePolicy::Ascending,
      RangePolicy::Wrapping,
      RangePolicy::Ascending
    ),
    "Quartz documents NOV-FEB and FRI-MON; `cron` 0.17 guards its range expansion \
     with start <= end and the Go dialect does the same"
  );
}

#[test]
fn no_wrapping_dialect_numbers_sunday_from_zero() {
  // A coupling worth pinning rather than discovering. A wrapping range folds through
  // the count of values the field admits, and a ZeroSunday day-of-week field admits
  // eight raw digits for seven days because it takes both 0 and 7 as Sunday. Wrapping
  // over that count would stride wrongly across the seam. No dialect does both today,
  // and adding one is a decision rather than an accident.
  for (_, f) in all() {
    assert!(
      f.ranges == RangePolicy::Ascending || f.weekday == WeekdayNumbering::OneSunday,
      "{} both wraps ranges and numbers Sunday from zero",
      f.name
    );
  }
}

#[test]
fn names_are_distinct() {
  let mut names: Vec<&str> = all().iter().map(|(_, f)| f.name).collect();
  names.sort_unstable();
  let before = names.len();
  names.dedup();
  assert_eq!(names.len(), before, "two dialects share a name");
}

#[test]
fn dialects_are_zero_sized() {
  // The dialect is a type, not a runtime tag: it must cost a schedule nothing.
  fn size_of_dialect<D: Dialect>() -> usize {
    core::mem::size_of::<D>()
  }
  assert_eq!(size_of_dialect::<Vixie>(), 0);
  assert_eq!(size_of_dialect::<Quartz>(), 0);
  assert_eq!(size_of_dialect::<Robfig>(), 0);
}
