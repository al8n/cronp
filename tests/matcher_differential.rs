//! The matcher against the four upstreams, over expressions × instants.
//!
//! The crate's other differential compares *parses* — the fused parser against the token
//! stream parser it replaced — and that is the gate a whole class of defect walked past.
//! A signal can be computed from the wrong thing and still parse identically on both
//! sides of an internal oracle; four such signals shipped, and what would have caught
//! every one of them is asking an implementation that is not this one whether the
//! schedule fires on a given day. That is what this file does.
//!
//! Nothing here reaches inside the crate. Every answer comes through `Schedule::matches`,
//! which is the whole point: the thing users depend on is now a function, so it can be
//! held against somebody else's.
//!
//! # Which upstream answers for which dialect, and where each one stops
//!
//! A disagreement is only evidence if the two sides are supposed to agree. Each pairing
//! below states why the upstream defines the dialect it is paired with, and every case
//! the pairing cannot carry is excluded by name, with the mechanism that excludes it.
//! An expression either side refuses to parse is skipped rather than excluded: a parse
//! difference is not a match difference, and the crate gates parses elsewhere.
//!
//! - **`Vixie` × `cronexpr` 1.6.0.** cronexpr *is* Vixie plus two constructs: five
//!   fields, Sunday as `0` with `7` also Sunday, the union rule, and the same wildcard —
//!   `input.starts_with('*')`, which is what vixie's `entry.c` tests on the field's first
//!   character. Instants are taken at second zero only, because cronexpr's `matches`
//!   never looks at the seconds component of the timestamp it is given.
//! - **`Cronexpr` × `cronexpr` 1.6.0.** The same, with `L`, `nW`, `nL` and `n#m` in play.
//!   `L-n` is skipped by cronexpr's own refusal of it, which is the one grammar
//!   difference this crate documents.
//! - **`Robfig` × `cron` 0.17.0.** Six fields, `?` as another spelling of `*`, a step
//!   after a bare value. `cron` has no union rule at all — `includes` ANDs every field —
//!   so it answers for a robfig expression exactly where robfig's `starBit` makes robfig
//!   intersect too. Where neither day field carries the bit, robfig unions and `cron`
//!   does not; those rows are marked `cron: false`. It also numbers Sunday `1`, so it is
//!   asked only about expressions whose day-of-week field carries no digit — see
//!   [`weekday_is_not_a_digit`].
//! - **`Robfig` × `croner` 3.0.1.** croner implements the union rule with `star_dom` and
//!   `star_dow`, and computes each as `field == "*"` after replacing `?` with `*`. That
//!   is robfig's answer wherever a `*` is a whole day field or absent from it, and not
//!   where a `*` shares a field with anything — `10,*` and `*/1` — so those rows are
//!   marked `croner: false`.
//! - **`Vixie` × `saffron` 0.1.0.** A five-field Vixie parser with the same union rule,
//!   keyed on the day field having parsed to its "all" shape. That is Vixie's answer for
//!   a field that is exactly `*` or has no `*` in it, and not for `*/2`, which saffron
//!   unions and Vixie intersects. It numbers Sunday `1` as `cron` does, so the same
//!   day-of-week restriction applies. Second zero only, for the same reason as cronexpr.
//! - **`Quartz` × `croner` 3.0.1**, in `alternative_weekdays` mode with seconds required
//!   and `dom_and_dow` set: Sunday as `1`, and both day fields required to match. Quartz's
//!   `?` is croner's `*` after the replacement above, so "both must match" is what
//!   reading the one specified field amounts to — which is Quartz's rule.
//! - **The nicknames × `cron` 0.17.0 and `croner` 3.0.1.** cronexpr's FAQ declines every
//!   nickname and saffron has none, so these two are the upstreams for `@weekly` and its
//!   siblings. Both expand them as vixie does and both intersect the day fields.
//!   Neither takes `@midnight` and `cron` does not take `@annually`; those two reach an
//!   oracle through [`the_nickname_synonyms_carry_their_oracle`] instead.
//!
//! # Why every exclusion above asserts itself
//!
//! Each exclusion is premised on how a *particular released version* of an upstream
//! behaves, and every one of those premises can stop being true. An exclusion whose
//! reason has expired does not announce itself: it goes on skipping the same rows, so the
//! crate's behaviour there is never checked again and the suite stays green while the
//! coverage quietly shrinks. A mechanism written in a doc comment is not a mechanism that
//! fails.
//!
//! So the exclusions below the pairings are executable. Each one asserts the upstream
//! behaviour that justifies it and fails when upstream stops reproducing it, at which
//! point the exclusion is deleted and the rows it was holding back come back:
//!
//! - [`the_december_exclusion_still_has_a_defect_to_exclude`] for [`cronexpr_answers`],
//! - [`cron_and_saffron_still_number_sunday_one`] for [`weekday_is_not_a_digit`],
//! - [`every_corpus_exclusion_is_still_live`] for the per-case `cron`, `croner` and
//!   `saffron` flags,
//! - [`the_upstream_refusals_are_the_declared_ones`] for the silent skip in [`compare`],
//!   which is an exclusion too — an unnamed one, and the only kind that can grow
//!   without anybody writing it down.
//!
//! The four dev-dependencies are pinned to exact versions in `Cargo.toml` for the same
//! reason: `Cargo.lock` is `.gitignore`d, so a caret range would let a clean checkout
//! resolve a different upstream and keep these exclusions anyway.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use chrono::{TimeZone, Utc};
use croner::parser::{CronParser, Seconds};

use cronp::{CivilDateTime, Cronexpr, Quartz, Robfig, Schedule, Vixie, days_in_month};

/// One expression, and which upstreams are a faithful oracle for it.
struct Case {
  /// The expression, in the five-field shape `Vixie`, `Cronexpr` and `saffron` take.
  ///
  /// The `Robfig` pairings prepend a `0` seconds field to it, so the same day, month and
  /// weekday shapes are put to every dialect rather than each getting its own list.
  expression: &'static str,
  /// Whether `cron` 0.17.0 answers for this expression under `Robfig`.
  cron: bool,
  /// Whether `croner` 3.0.1 answers for this expression under `Robfig`.
  croner: bool,
  /// Whether `saffron` 0.1.0 answers for this expression under `Vixie`.
  saffron: bool,
  /// Why an upstream above is excluded, where one is.
  why: &'static str,
}

/// The default: every upstream answers for this expression.
const fn all(expression: &'static str) -> Case {
  Case {
    expression,
    cron: true,
    croner: true,
    saffron: true,
    why: "",
  }
}

/// Expressions chosen for the places two matchers can disagree.
///
/// The day fields carry most of them, because the day fields are where the dialects
/// carry a rule rather than a set: every shape the wildcard witness can take appears
/// here, in both day fields, alone and beside another item.
const CASES: &[Case] = &[
  // ----- no rule to apply: at most one day field is restricted -----
  all("* * * * *"),
  all("0 0 * * *"),
  all("30 2 * * *"),
  all("0 0 1 * *"),
  all("0 0 15 * *"),
  all("0 0 31 * *"),
  all("0 0 29 2 *"),
  all("0 0 1,15 * *"),
  all("0 0 1-7 * *"),
  all("0 0 */2 * *"),
  all("0 0 * * 0"),
  all("0 0 * * 7"),
  all("0 0 * * SUN"),
  all("0 0 * * MON-FRI"),
  all("0 0 * * 1,3,5"),
  all("0 0 * JAN,JUL *"),
  all("0 0 * FEB *"),
  all("0 0 * * */2"),
  all("*/15 * * * *"),
  all("0-30/10 1-5 * * *"),
  all("59 23 * * *"),
  all("0 0,12 1 */2 *"),
  // ----- both day fields restricted, and neither carries a wildcard: the union -----
  Case {
    expression: "0 0 1 * MON",
    cron: false,
    croner: true,
    saffron: true,
    why: "robfig unions here and `cron`'s `includes` ANDs every field",
  },
  Case {
    expression: "0 0 1,15 * MON",
    cron: false,
    croner: true,
    saffron: true,
    why: "as above",
  },
  Case {
    expression: "0 0 15 * SAT,SUN",
    cron: false,
    croner: true,
    saffron: true,
    why: "as above",
  },
  Case {
    expression: "0 0 1-7 * MON",
    cron: false,
    croner: true,
    saffron: true,
    why: "the first-Monday idiom, which the union rule makes something else entirely",
  },
  // ----- a wildcard beside another item, in each day field and at each end -----
  Case {
    expression: "0 0 *,10 * MON",
    cron: true,
    croner: false,
    saffron: false,
    why: "croner refuses `*,10`; saffron refuses it too",
  },
  Case {
    expression: "0 0 10,* * MON",
    cron: true,
    croner: false,
    saffron: false,
    why: "croner refuses `10,*`; saffron parses it as the 10th alone, so it is neither \
          the same set nor the same rule",
  },
  Case {
    expression: "0 0 1 * *,3",
    cron: true,
    croner: false,
    saffron: false,
    why: "as `*,10`, in the other day field",
  },
  Case {
    expression: "0 0 1 * 3,*",
    cron: true,
    croner: false,
    saffron: false,
    why: "as `10,*`, in the other day field. `Vixie` unions this and `Robfig` intersects \
          it, which is the pair of answers the two witnesses were separated for",
  },
  // ----- a stepped star, which the two witnesses answer in opposite directions -----
  Case {
    expression: "0 0 */2 * MON",
    cron: false,
    croner: true,
    saffron: false,
    why: "robfig clears its star bit for a stride above one and unions, which croner \
          agrees with and `cron` cannot; saffron unions it too, which is what `Vixie` \
          does not do",
  },
  Case {
    expression: "0 0 * * */2",
    cron: true,
    croner: true,
    saffron: true,
    why: "",
  },
  Case {
    expression: "0 0 */1 * MON",
    cron: true,
    croner: false,
    saffron: false,
    why: "robfig keeps its star bit for a stride of one and intersects; croner's \
          `field == \"*\"` and saffron's parsed shape both make `*/1` something else",
  },
  // ----- `?`, which only the Go dialect has here -----
  Case {
    expression: "0 0 ? * MON",
    cron: true,
    croner: true,
    saffron: true,
    why: "",
  },
  Case {
    expression: "0 0 ?,1 * MON",
    cron: true,
    croner: true,
    saffron: true,
    why: "",
  },
  Case {
    expression: "0 0 1 * ?",
    cron: true,
    croner: true,
    saffron: true,
    why: "",
  },
  // ----- the date predicates, which reach the `Cronexpr` pairing only -----
  all("0 0 L * *"),
  all("0 0 L * MON"),
  all("0 0 15W * *"),
  all("0 0 1W * *"),
  all("0 0 31W * *"),
  all("0 0 * * 5L"),
  all("0 0 * * 6#3"),
  all("0 0 * * 1#1"),
  all("0 0 * * 0L"),
  // ----- a step after a bare value: `Vixie` refuses it, the other three take it -----
  all("0 0 5/10 * *"),
  all("0 0 * * 2/2"),
];

/// Six-field expressions for the `Robfig` pairings, where the seconds field is real.
///
/// The corpus above is five-field and reaches `Robfig` with a `0` in front of it, so on
/// its own it would never put a value other than zero in the seconds field.
const SECONDS_CASES: &[&str] = &[
  "* * * * * *",
  "30 * * * * *",
  "*/15 * * * * *",
  "0,30 0 0 * * *",
  "59 59 23 * * *",
];

/// The nicknames, which `cron` and `croner` are the upstreams for.
const NICKNAMES: &[&str] = &[
  "@yearly",
  "@annually",
  "@monthly",
  "@weekly",
  "@daily",
  "@midnight",
  "@hourly",
];

/// Quartz expressions: six or seven fields, with exactly one `?` day field.
const QUARTZ_CASES: &[&str] = &[
  "0 0 0 * * ?",
  "0 0 0 ? * *",
  "0 30 2 ? * MON-FRI",
  "0 0 0 1 * ?",
  "0 0 0 15 * ?",
  "0 0 0 L * ?",
  "0 0 0 LW * ?",
  "0 0 0 15W * ?",
  "0 0 0 ? * 6#3",
  "0 0 0 ? * 6L",
  "0 0 0 ? * SUN",
  "0 0 0 ? * SAT",
  "0 0 0 29 2 ?",
  "0 */15 * ? * *",
  "30 0 0 ? * *",
  "0 0 0 1 1 ? 2026",
  "0 0 0 ? * MON 2026-2028",
  // A wildcard sharing the year field with another item. This is the row the repair is
  // about: the union is every year, so the schedule is the one with no year field at
  // all, and an upstream that is not this crate says whether that is so. It used to be
  // refused outright at `Schedule<Quartz, 1>` because the parser materialised
  // `1970..=2099` to back a restriction the expression never placed.
  "0 0 0 ? * MON 2026,*",
  "0 0 0 ? * MON *,2026",
  "0 0 0 1 1 ? *,*",
];

/// The instants every pairing is run over, all at second zero.
///
/// A whole turn of the calendar, so every weekday falls on every day-of-month position a
/// year allows and every month length appears; February in two leap years, for `29 2 *`
/// and for a `L` that moves; and a Monday and a Wednesday through the clock, for the
/// fields a date sweep at midnight would never vary.
fn instants() -> Vec<CivilDateTime> {
  let mut out = Vec::new();
  let mut push = |year, month, day, hour, minute, second| {
    out.push(CivilDateTime::new(year, month, day, hour, minute, second).expect("a real date"));
  };

  for month in 1..=12u8 {
    for day in 1..=days_in_month(2026, month) {
      push(2026, month, day, 0, 0, 0);
    }
  }
  for year in [2024u16, 2028] {
    for day in 1..=29u8 {
      push(year, 2, day, 0, 0, 0);
    }
  }
  for day in [10u8, 12] {
    for hour in [0u8, 1, 2, 23] {
      for minute in [0u8, 1, 30, 59] {
        push(2026, 8, day, hour, minute, 0);
      }
    }
  }
  out
}

/// The instants for a pairing whose upstream reads the seconds component.
fn second_instants() -> Vec<CivilDateTime> {
  [0u8, 1, 30, 59]
    .iter()
    .map(|&second| CivilDateTime::new(2026, 8, 12, 0, 0, second).expect("a real date"))
    .collect()
}

/// Something that answers "does this fire then" for one expression.
type Matcher = Box<dyn Fn(CivilDateTime) -> bool>;

/// Runs one pairing over one corpus, and returns how many expression × instant pairs were
/// compared.
///
/// An expression either side refuses to parse is skipped: a parse difference is not a
/// match difference. `answers` is the pairing's own boundary — the expression × instant
/// pairs its upstream is not an oracle for, each with the mechanism that puts it there.
fn compare(
  label: &str,
  expressions: &[String],
  when: &[CivilDateTime],
  ours: impl Fn(&str) -> Option<Matcher>,
  theirs: impl Fn(&str) -> Option<Matcher>,
  answers: impl Fn(&str, CivilDateTime) -> bool,
) -> usize {
  let mut compared = 0usize;
  for expression in expressions {
    let (Some(ours), Some(theirs)) = (ours(expression), theirs(expression)) else {
      continue;
    };
    for &instant in when {
      if !answers(expression, instant) {
        continue;
      }
      assert_eq!(
        ours(instant),
        theirs(instant),
        "{label}: {expression:?} at {instant} — cronp says {}, the upstream says {}",
        ours(instant),
        theirs(instant)
      );
      compared = compared.saturating_add(1);
    }
  }
  println!("{label}: {compared} expression x instant pairs");
  compared
}

/// Every pair: the pairing has no boundary beyond the corpus tags.
fn every_pair(_expression: &str, _when: CivilDateTime) -> bool {
  true
}

/// The last whitespace-separated field, which in every shape here is the day of the week.
fn day_of_week_field(expression: &str) -> &str {
  expression.split_whitespace().next_back().unwrap_or("")
}

/// Whether `cron` 0.17.0 and `saffron` 0.1.0 answer for this expression at all.
///
/// Both of them number the days of the week from **Sunday as one**, which is Quartz's
/// numbering and neither Vixie's nor the Go dialect's: `cron`'s `DAY_OF_WEEK_MAP` reads
/// `sun` as `1` and `sat` as `7` with an `inclusive_min` of 1, and saffron answers `1`
/// with Sunday and `7` with Saturday. Both reject `0` outright. So a digit in the
/// day-of-week field names a different day there than here — `1,3,5` is Sunday, Tuesday
/// and Thursday to them and Monday, Wednesday and Friday here, and `7` is Saturday to
/// them and Sunday here, where `crontab(5)` puts it.
///
/// Names do not vary between dialects, and `*` and `?` name no day at all. Those are
/// what these two can be asked about, and the numbering is oracled by `cronexpr` and
/// `croner`, which both number Sunday zero.
///
/// [`cron_and_saffron_still_number_sunday_one`] asserts all four of those numberings, so
/// this exclusion fails rather than silently outliving its reason.
fn weekday_is_not_a_digit(expression: &str, _when: CivilDateTime) -> bool {
  !day_of_week_field(expression)
    .bytes()
    .any(|byte| byte.is_ascii_digit())
}

/// Whether `cronexpr` 1.6.0 answers for this expression at this instant.
///
/// Its last-of-month predicates ask whether `(value + 1.day()).month() > value.month()`,
/// and in December the next day's month is January: `1 > 12` is false, so `L` and `nL`
/// fire on no day of December at all there. Every other month is unaffected, and so is
/// every other predicate — `nW` and `n#m` compare days within the month.
///
/// Only the two day fields are read for the `L`. A month field carries one too —
/// `JUL` — and a month name is not a last-of-month predicate: looking for the letter
/// anywhere in the expression took December away from `0 0 * JAN,JUL *`, which is a row
/// cronexpr answers perfectly well. The corpus this runs over is five-field throughout,
/// so the two day fields are the third and the fifth.
///
/// [`the_december_exclusion_still_has_a_defect_to_exclude`] asserts that defect against
/// the pinned cronexpr, so a repaired upstream fails the suite here rather than leaving
/// December permanently unchecked for these rows. It also asserts that the exclusion is
/// no wider than the defect, which is what found the `JUL` above.
fn cronexpr_answers(expression: &str, when: CivilDateTime) -> bool {
  let mut fields = expression.split_whitespace();
  let day_of_month = fields.nth(2).unwrap_or("");
  let day_of_week = fields.nth(1).unwrap_or("");
  let last_of_something = [day_of_month, day_of_week]
    .iter()
    .any(|field| field.contains('L') && !field.contains("L-"));
  when.month() != 12 || !last_of_something
}

// ---------------------------------------------------------------------------
// The upstreams, each behind the same shape.
// ---------------------------------------------------------------------------

fn as_cronexpr(expression: &str) -> Option<Matcher> {
  // A timezone is required unless the options say otherwise; UTC keeps the crontab's
  // civil calendar and the timestamp's the same one.
  let crontab = cronexpr::parse_crontab(&format!("{expression} UTC")).ok()?;
  Some(Box::new(move |when: CivilDateTime| {
    crontab.matches(iso(when).as_str()).unwrap_or(false)
  }))
}

fn as_cron(expression: &str) -> Option<Matcher> {
  let schedule: cron::Schedule = expression.parse().ok()?;
  Some(Box::new(move |when: CivilDateTime| {
    schedule.includes(utc(when))
  }))
}

fn as_croner(expression: &str, parser: CronParser) -> Option<Matcher> {
  let cron = parser.parse(expression).ok()?;
  Some(Box::new(move |when: CivilDateTime| {
    cron.is_time_matching(&utc(when)).unwrap_or(false)
  }))
}

fn as_saffron(expression: &str) -> Option<Matcher> {
  let cron = expression.parse::<saffron::Cron>().ok()?;
  Some(Box::new(move |when: CivilDateTime| {
    cron.contains(utc(when))
  }))
}

fn as_cronp<D: cronp::Dialect + 'static, const N: usize>(expression: &str) -> Option<Matcher> {
  let schedule = Schedule::<D, N>::parse(expression).ok()?;
  Some(Box::new(move |when: CivilDateTime| schedule.matches(when)))
}

fn iso(when: CivilDateTime) -> String {
  format!("{when}Z")
}

fn utc(when: CivilDateTime) -> chrono::DateTime<Utc> {
  Utc
    .with_ymd_and_hms(
      i32::from(when.year()),
      u32::from(when.month()),
      u32::from(when.day()),
      u32::from(when.hour()),
      u32::from(when.minute()),
      u32::from(when.second()),
    )
    .single()
    .expect("UTC has no ambiguous instants")
}

/// The five-field corpus, for the dialects that take five fields.
fn five_field(keep: impl Fn(&Case) -> bool) -> Vec<String> {
  CASES
    .iter()
    .filter(|case| keep(case))
    .map(|case| String::from(case.expression))
    .collect()
}

/// The same corpus with a `0` seconds field in front, for the dialects that take six.
fn six_field(keep: impl Fn(&Case) -> bool) -> Vec<String> {
  CASES
    .iter()
    .filter(|case| keep(case))
    .map(|case| format!("0 {}", case.expression))
    .chain(SECONDS_CASES.iter().map(|&case| String::from(case)))
    .collect()
}

// ---------------------------------------------------------------------------
// The pairings.
// ---------------------------------------------------------------------------

#[test]
#[cfg_attr(
  miri,
  ignore = "tens of thousands of matches through four upstream parsers, jiff and chrono; \
            the divergence this looks for is a property of the logic, not of the machine \
            it is interpreted on, and every crate involved is `forbid(unsafe_code)` here"
)]
fn vixie_agrees_with_cronexpr() {
  let compared = compare(
    "Vixie × cronexpr",
    &five_field(|_| true),
    &instants(),
    as_cronp::<Vixie, 1>,
    as_cronexpr,
    cronexpr_answers,
  );
  assert!(compared > 10_000, "only {compared} pairs compared");
}

#[test]
#[cfg_attr(
  miri,
  ignore = "tens of thousands of matches through four upstream parsers, jiff and chrono; \
            the divergence this looks for is a property of the logic, not of the machine \
            it is interpreted on, and every crate involved is `forbid(unsafe_code)` here"
)]
fn the_cronexpr_dialect_agrees_with_cronexpr() {
  let compared = compare(
    "Cronexpr × cronexpr",
    &five_field(|_| true),
    &instants(),
    as_cronp::<Cronexpr, 1>,
    as_cronexpr,
    cronexpr_answers,
  );
  assert!(compared > 10_000, "only {compared} pairs compared");
}

#[test]
#[cfg_attr(
  miri,
  ignore = "tens of thousands of matches through four upstream parsers, jiff and chrono; \
            the divergence this looks for is a property of the logic, not of the machine \
            it is interpreted on, and every crate involved is `forbid(unsafe_code)` here"
)]
fn robfig_agrees_with_cron() {
  let mut compared = compare(
    "Robfig × cron",
    &six_field(|case| case.cron),
    &instants(),
    as_cronp::<Robfig, 1>,
    as_cron,
    weekday_is_not_a_digit,
  );
  compared = compared.saturating_add(compare(
    "Robfig × cron, over the seconds",
    &six_field(|case| case.cron),
    &second_instants(),
    as_cronp::<Robfig, 1>,
    as_cron,
    weekday_is_not_a_digit,
  ));
  assert!(compared > 10_000, "only {compared} pairs compared");
}

#[test]
#[cfg_attr(
  miri,
  ignore = "tens of thousands of matches through four upstream parsers, jiff and chrono; \
            the divergence this looks for is a property of the logic, not of the machine \
            it is interpreted on, and every crate involved is `forbid(unsafe_code)` here"
)]
fn robfig_agrees_with_croner() {
  let parser = || CronParser::builder().seconds(Seconds::Required).build();
  let mut compared = compare(
    "Robfig × croner",
    &six_field(|case| case.croner),
    &instants(),
    as_cronp::<Robfig, 1>,
    |expression| as_croner(expression, parser()),
    every_pair,
  );
  compared = compared.saturating_add(compare(
    "Robfig × croner, over the seconds",
    &six_field(|case| case.croner),
    &second_instants(),
    as_cronp::<Robfig, 1>,
    |expression| as_croner(expression, parser()),
    every_pair,
  ));
  assert!(compared > 10_000, "only {compared} pairs compared");
}

#[test]
#[cfg_attr(
  miri,
  ignore = "tens of thousands of matches through four upstream parsers, jiff and chrono; \
            the divergence this looks for is a property of the logic, not of the machine \
            it is interpreted on, and every crate involved is `forbid(unsafe_code)` here"
)]
fn vixie_agrees_with_saffron() {
  let compared = compare(
    "Vixie × saffron",
    &five_field(|case| case.saffron),
    &instants(),
    as_cronp::<Vixie, 1>,
    as_saffron,
    weekday_is_not_a_digit,
  );
  assert!(compared > 10_000, "only {compared} pairs compared");
}

#[test]
#[cfg_attr(
  miri,
  ignore = "tens of thousands of matches through four upstream parsers, jiff and chrono; \
            the divergence this looks for is a property of the logic, not of the machine \
            it is interpreted on, and every crate involved is `forbid(unsafe_code)` here"
)]
fn quartz_agrees_with_croner() {
  // Quartz's numbering, Quartz's seconds field, and both day fields required to match —
  // which, with the `?` field admitting everything, is Quartz's rule.
  let parser = || {
    CronParser::builder()
      .seconds(Seconds::Required)
      .dom_and_dow(true)
      .alternative_weekdays(true)
      .build()
  };
  let expressions: Vec<String> = QUARTZ_CASES
    .iter()
    .map(|&case| String::from(case))
    .collect();
  let mut compared = compare(
    "Quartz × croner",
    &expressions,
    &instants(),
    as_cronp::<Quartz, 1>,
    |expression| as_croner(expression, parser()),
    every_pair,
  );
  compared = compared.saturating_add(compare(
    "Quartz × croner, over the seconds",
    &expressions,
    &second_instants(),
    as_cronp::<Quartz, 1>,
    |expression| as_croner(expression, parser()),
    every_pair,
  ));
  assert!(compared > 5_000, "only {compared} pairs compared");
}

#[test]
#[cfg_attr(
  miri,
  ignore = "tens of thousands of matches through four upstream parsers, jiff and chrono; \
            the divergence this looks for is a property of the logic, not of the machine \
            it is interpreted on, and every crate involved is `forbid(unsafe_code)` here"
)]
fn the_nicknames_agree_with_cron_and_croner() {
  let nicknames: Vec<String> = NICKNAMES.iter().map(|&name| String::from(name)).collect();

  let against_cron = compare(
    "Vixie nicknames × cron",
    &nicknames,
    &instants(),
    as_cronp::<Vixie, 1>,
    as_cron,
    every_pair,
  );
  let against_croner = compare(
    "Vixie nicknames × croner",
    &nicknames,
    &instants(),
    as_cronp::<Vixie, 1>,
    |expression| as_croner(expression, CronParser::new()),
    every_pair,
  );
  // The Go dialect has the same nicknames and, upstream, the same expansions.
  let robfig = compare(
    "Robfig nicknames × cron",
    &nicknames,
    &instants(),
    as_cronp::<Robfig, 1>,
    as_cron,
    every_pair,
  );

  assert!(
    against_cron > 2_000 && against_croner > 2_000 && robfig > 2_000,
    "only {against_cron}, {against_croner} and {robfig} pairs compared"
  );
}

// ---------------------------------------------------------------------------
// The measured divergences, each pinned against a live upstream.
// ---------------------------------------------------------------------------

/// The seven rows a census measured, with the upstream that decides each one.
///
/// These are not extra coverage — every one of them is inside a pairing above. They are
/// here because a corpus row that stops being exercised fails nothing, and each of these
/// was a shipped defect. Naming them keeps the answer pinned to the expression that had
/// it wrong, so a regression says which one.
///
/// The upstream's answer is computed, not written down. A pinned expectation is a claim
/// about an upstream that nothing rechecks; a live one moves if the upstream does, which
/// is a fact worth learning.
#[test]
#[cfg_attr(
  miri,
  ignore = "tens of thousands of matches through four upstream parsers, jiff and chrono; \
            the divergence this looks for is a property of the logic, not of the machine \
            it is interpreted on, and every crate involved is `forbid(unsafe_code)` here"
)]
fn the_measured_divergences_are_closed() {
  /// A day the row turns on, and what must happen on it.
  struct Pin {
    /// What this row is.
    label: &'static str,
    /// The expression, in the dialect named by `label`.
    expression: &'static str,
    /// The day, and the matcher's answer for it.
    days: &'static [((u16, u8, u8), bool)],
  }

  // 2026-08-02 is a Sunday, 08-03 and 08-10 Mondays, 08-11 a Tuesday, 08-12 a Wednesday,
  // 03-31 a Tuesday and the last day of March.
  const PINS: &[Pin] = &[
    Pin {
      label: "Vixie @weekly fires on Sundays, not on every day",
      expression: "@weekly",
      days: &[
        ((2026, 8, 2), true),
        ((2026, 8, 10), false),
        ((2026, 8, 12), false),
      ],
    },
    Pin {
      label: "Vixie @monthly fires on the first",
      expression: "@monthly",
      days: &[
        ((2026, 8, 1), true),
        ((2026, 8, 2), false),
        ((2026, 9, 1), true),
      ],
    },
    Pin {
      label: "Vixie @yearly fires on the first of January",
      expression: "@yearly",
      days: &[
        ((2026, 1, 1), true),
        ((2026, 2, 1), false),
        ((2026, 8, 2), false),
      ],
    },
    Pin {
      label: "Robfig `?` is a wildcard, so the day fields intersect",
      expression: "0 0 0 ? * MON",
      days: &[
        ((2026, 8, 10), true),
        ((2026, 8, 11), false),
        ((2026, 8, 12), false),
      ],
    },
    Pin {
      label: "Robfig ORs its star bit across a list, so `10,*` intersects",
      expression: "0 0 0 10,* * MON",
      days: &[
        ((2026, 8, 10), true),
        ((2026, 8, 11), false),
        ((2026, 8, 12), false),
      ],
    },
    Pin {
      label: "Robfig clears its star bit above stride one, so `*/2` unions",
      expression: "0 0 0 */2 * MON",
      days: &[
        ((2026, 8, 10), true),
        ((2026, 8, 11), true),
        ((2026, 8, 12), false),
        ((2026, 3, 31), true),
      ],
    },
    Pin {
      label: "Cronexpr `L` is the last day of the month, not no day at all",
      expression: "0 0 L * *",
      days: &[
        ((2026, 3, 31), true),
        ((2026, 3, 30), false),
        ((2026, 2, 28), true),
        ((2024, 2, 29), true),
        ((2024, 2, 28), false),
      ],
    },
  ];

  let at = |(year, month, day): (u16, u8, u8)| {
    CivilDateTime::new(year, month, day, 0, 0, 0).expect("a real date")
  };

  // The matcher's answer.
  for pin in PINS {
    let ours: Matcher = if pin.label.starts_with("Vixie") {
      as_cronp::<Vixie, 1>(pin.expression)
    } else if pin.label.starts_with("Robfig") {
      as_cronp::<Robfig, 1>(pin.expression)
    } else {
      as_cronp::<Cronexpr, 1>(pin.expression)
    }
    .unwrap_or_else(|| panic!("{}: {:?} must parse", pin.label, pin.expression));

    for &(day, expected) in pin.days {
      assert_eq!(
        ours(at(day)),
        expected,
        "{}: {:?} on {:?}",
        pin.label,
        pin.expression,
        day
      );
    }
  }

  // And the same answers out of an upstream that is not this crate. Which upstream is
  // the one the pairings above justify for that dialect and that expression.
  let upstreams: &[(&str, Matcher)] = &[
    (
      "@weekly",
      as_cron("@weekly").expect("cron takes the nicknames"),
    ),
    (
      "@monthly",
      as_croner("@monthly", CronParser::new()).expect("croner takes the nicknames"),
    ),
    (
      "@yearly",
      as_cron("@yearly").expect("cron takes the nicknames"),
    ),
    (
      "0 0 0 ? * MON",
      as_cron("0 0 0 ? * MON").expect("cron reads `?` as a star and intersects"),
    ),
    (
      "0 0 0 10,* * MON",
      as_cron("0 0 0 10,* * MON").expect("robfig intersects here, and so does cron"),
    ),
    (
      // robfig unions a stepped star and `cron` cannot; croner is the upstream that
      // reproduces `if step > 1 { extra = 0 }`.
      "0 0 */2 * MON",
      as_croner("0 0 */2 * MON", CronParser::new()).expect("croner unions a stepped star"),
    ),
    (
      "0 0 L * *",
      as_cronexpr("0 0 L * *").expect("cronexpr has `L`"),
    ),
  ];

  for (pin, (expression, upstream)) in PINS.iter().zip(upstreams) {
    for &(day, expected) in pin.days {
      assert_eq!(
        upstream(at(day)),
        expected,
        "{}: the upstream reading of {expression:?} on {day:?} is not what this pins",
        pin.label
      );
    }
  }
}

#[test]
fn the_corpus_reaches_every_pairing() {
  // Each pairing must keep a corpus rather than filtering itself down to nothing, and
  // each exclusion must carry its reason.
  let cron = CASES.iter().filter(|case| case.cron).count();
  let croner = CASES.iter().filter(|case| case.croner).count();
  let saffron = CASES.iter().filter(|case| case.saffron).count();
  assert!(
    CASES.len() >= 40 && cron >= 30 && croner >= 30 && saffron >= 30,
    "the corpus shrank: {} cases, {cron} for cron, {croner} for croner, {saffron} for \
     saffron",
    CASES.len()
  );

  for case in CASES {
    let excluded = !case.cron || !case.croner || !case.saffron;
    assert_eq!(
      excluded,
      !case.why.is_empty(),
      "{:?}: an exclusion needs a reason and a reason needs an exclusion",
      case.expression
    );
  }

  let instants = instants();
  assert!(
    instants.len() >= 400,
    "the instant sweep shrank to {}",
    instants.len()
  );
}

// ---------------------------------------------------------------------------
// The exclusions, each asserting the upstream behaviour that justifies it.
//
// Everything below this line exists so that an exclusion cannot outlive its reason. Each
// test states the repair in its failure message: what to delete, and which rows come back
// when it is deleted.
// ---------------------------------------------------------------------------

/// [`cronexpr_answers`] has a defect to exclude.
///
/// The exclusion is premised on cronexpr 1.6.0 answering "no" for `L` and `nL` on every
/// day of December, and nothing else rechecks that. If cronexpr repairs it, the exclusion
/// keeps taking December away from those rows for good and this crate's answer there is
/// never compared with anything again — a green suite covering strictly less than it
/// claims. So the defect is asserted, and its repair is a red test.
#[test]
#[cfg_attr(
  miri,
  ignore = "several hundred matches through cronexpr and jiff; the upstream behaviour \
            this pins is a property of that crate's logic, not of the machine it is \
            interpreted on"
)]
fn the_december_exclusion_still_has_a_defect_to_exclude() {
  let december = |day: u8| CivilDateTime::new(2026, 12, day, 0, 0, 0).expect("a real date");
  let november = |day: u8| CivilDateTime::new(2026, 11, day, 0, 0, 0).expect("a real date");

  // The mechanism, named. 2026-12-31 and 2026-11-30 are both the last day of their month,
  // and cronexpr answers them differently: only the December one runs into
  // `(value + 1.day()).month() > value.month()` with January on the left.
  let ours = as_cronp::<Cronexpr, 1>("0 0 L * *").expect("the `Cronexpr` dialect has `L`");
  let theirs = as_cronexpr("0 0 L * *").expect("cronexpr has `L`");
  assert!(
    ours(december(31)) && ours(november(30)),
    "this crate stopped firing `0 0 L * *` on the last day of the month, which is a \
     defect in this crate and not in the exclusion below"
  );
  assert!(
    theirs(november(30)),
    "cronexpr 1.6.0 no longer fires `0 0 L * *` on 2026-11-30 either, so `L` is broken \
     there in some wider way than the December case this excludes; re-read the pairing \
     before touching the exclusion"
  );
  assert!(
    !theirs(december(31)),
    "cronexpr 1.6.0 now fires `0 0 L * *` on 2026-12-31: the December `L`/`nL` defect is \
     repaired and the exclusion is a blind spot. Delete `cronexpr_answers`, hand \
     `every_pair` to the two cronexpr pairings, and delete this test — December comes \
     back for `0 0 L * *`, `0 0 L * MON`, `0 0 * * 5L` and `0 0 * * 0L`"
  );

  // And the exclusion is not wider than the defect: `nW` and `n#m` compare days inside
  // the month, so December is not in dispute for them and the predicate must not be
  // taking it away. `JAN,JUL` is here because the predicate did take it away — the
  // letter it looks for is in a month name.
  for expression in ["0 0 15W * *", "0 0 * * 6#3", "0 0 * JAN,JUL *"] {
    assert!(
      (1..=31u8).all(|day| cronexpr_answers(expression, december(day))),
      "{expression:?} is excluded from December, but only an `L` or an `nL` in a day \
       field runs into cronexpr's month comparison"
    );
  }

  // Every corpus row the predicate does take December away from must be a row where
  // December is genuinely in dispute. One where cronexpr agrees with this crate all
  // through December is a row the exclusion is only hiding.
  let all_of_december: Vec<CivilDateTime> = (1..=31u8).map(december).collect();
  let mut excluded = 0usize;
  for case in CASES {
    if all_of_december
      .iter()
      .all(|&when| cronexpr_answers(case.expression, when))
    {
      continue;
    }
    excluded = excluded.saturating_add(1);

    let ours = as_cronp::<Cronexpr, 1>(case.expression).unwrap_or_else(|| {
      panic!(
        "{:?} is excluded from December but does not parse here",
        case.expression
      )
    });
    let theirs = as_cronexpr(case.expression).unwrap_or_else(|| {
      panic!(
        "{:?} is excluded from December but cronexpr refuses it",
        case.expression
      )
    });
    assert!(
      all_of_december
        .iter()
        .any(|&when| ours(when) != theirs(when)),
      "{:?} is excluded from December and cronexpr agrees with this crate on every day \
       of it, so the exclusion buys nothing: give the row back to the pairing",
      case.expression
    );
  }
  assert!(
    excluded > 0,
    "no corpus row reaches the December exclusion any more — either the corpus lost its \
     `L` rows or `cronexpr_answers` stopped excluding, and the predicate is dead either \
     way"
  );
}

/// [`weekday_is_not_a_digit`] still has two upstreams that number the week differently.
///
/// That predicate withholds every expression with a digit in its day-of-week field from
/// `cron` and `saffron`, which is a large exclusion: it is what keeps `0 0 * * 7`,
/// `0 0 * * 1,3,5` and `0 0 * * 2/2` away from two of the five upstreams. It is worth
/// exactly as much as the claim underneath it, so the claim is asserted — on both
/// sides, because the exclusion also says `croner` and `cronexpr` *can* be asked.
#[test]
fn cron_and_saffron_still_number_sunday_one() {
  // 2026-08-02 is a Sunday, 08-03 a Monday and 08-08 a Saturday.
  let at = |day: u8| CivilDateTime::new(2026, 8, day, 0, 0, 0).expect("a real date");
  let (sunday, monday, saturday) = (at(2), at(3), at(8));

  for (label, one, seven, zero) in [
    (
      "cron 0.17.0",
      as_cron("0 0 0 * * 1"),
      as_cron("0 0 0 * * 7"),
      as_cron("0 0 0 * * 0"),
    ),
    (
      "saffron 0.1.0",
      as_saffron("0 0 * * 1"),
      as_saffron("0 0 * * 7"),
      as_saffron("0 0 * * 0"),
    ),
  ] {
    let one = one.unwrap_or_else(|| panic!("{label} takes a `1` in the day-of-week field"));
    assert!(
      one(sunday) && !one(monday),
      "{label} no longer reads `1` as Sunday. It shares this crate's numbering now, so \
       `weekday_is_not_a_digit` is withholding rows for a reason that has expired: give \
       that upstream the digits back"
    );
    let seven = seven.unwrap_or_else(|| panic!("{label} takes a `7` in the day-of-week field"));
    assert!(
      seven(saturday),
      "{label} no longer reads `7` as Saturday, so its numbering has moved and \
       `weekday_is_not_a_digit` needs re-deriving"
    );
    assert!(
      zero.is_none(),
      "{label} now accepts a `0` in the day-of-week field, which it rejected when the \
       exclusion was written; re-derive the numbering before trusting it"
    );
  }

  // This crate's numbering, which is the other half of the divergence.
  let ours = as_cronp::<Vixie, 1>("0 0 * * 1").expect("the `Vixie` dialect takes a digit");
  assert!(
    ours(monday) && !ours(sunday),
    "this crate stopped reading `1` as Monday, which is `crontab(5)`'s numbering"
  );

  // And the two upstreams the predicate hands the digits to instead. If either of these
  // moved, the digits would have no oracle at all and the exclusion would need widening
  // rather than deleting.
  let croner = as_croner(
    "0 0 0 * * 0",
    CronParser::builder().seconds(Seconds::Required).build(),
  )
  .expect("croner takes a `0` in the day-of-week field");
  assert!(
    croner(sunday) && !croner(monday),
    "croner 3.0.1 no longer numbers Sunday zero, so it is no longer the oracle for a \
     digit in the day-of-week field"
  );
  let cronexpr = as_cronexpr("0 0 * * 0").expect("cronexpr takes a `0` in the day-of-week field");
  assert!(
    cronexpr(sunday) && !cronexpr(monday),
    "cronexpr 1.6.0 no longer numbers Sunday zero, so it is no longer the oracle for a \
     digit in the day-of-week field"
  );
}

/// Every `cron: false`, `croner: false` and `saffron: false` in [`CASES`] still excludes
/// something.
///
/// A per-case exclusion says the upstream is not a faithful oracle for that expression.
/// The only observable content of that claim is that the upstream refuses the expression
/// or answers it differently from this crate somewhere. When neither is true any more the
/// upstream has converged on this crate's reading, and the flag is holding a row out of
/// a pairing that would now pass — so it is checked rather than described, and the list
/// is read off [`CASES`] rather than written down twice.
#[test]
#[cfg_attr(
  miri,
  ignore = "thousands of matches through three upstream parsers and chrono; the \
            divergences this looks for are properties of those crates' logic, not of the \
            machine they are interpreted on"
)]
fn every_corpus_exclusion_is_still_live() {
  let when = instants();

  for case in CASES {
    let six = format!("0 {}", case.expression);
    let per_upstream: [(&str, bool, Option<Matcher>, Option<Matcher>); 3] = [
      (
        "cron",
        case.cron,
        as_cronp::<Robfig, 1>(&six),
        as_cron(&six),
      ),
      (
        "croner",
        case.croner,
        as_cronp::<Robfig, 1>(&six),
        as_croner(
          &six,
          CronParser::builder().seconds(Seconds::Required).build(),
        ),
      ),
      (
        "saffron",
        case.saffron,
        as_cronp::<Vixie, 1>(case.expression),
        as_saffron(case.expression),
      ),
    ];

    for (upstream, answers, ours, theirs) in per_upstream {
      if answers {
        continue;
      }
      // A refusal is the strongest form of "not an oracle here", so there is nothing
      // further to check while it lasts. When it ends the upstream starts parsing the
      // expression and control falls through to the divergence check below, which is
      // where a converged reading is caught.
      let Some(theirs) = theirs else {
        continue;
      };
      let ours = ours.unwrap_or_else(|| {
        panic!(
          "{:?} is excluded from {upstream} but this crate does not parse it either, so \
           the row is compared with nothing at all",
          case.expression
        )
      });
      assert!(
        when.iter().any(|&instant| ours(instant) != theirs(instant)),
        "{:?} is excluded from {upstream} — {:?} — but {upstream} now parses it and \
         agrees with this crate on all {} instants. The exclusion has outlived its \
         reason: set that flag to `true`",
        case.expression,
        case.why,
        when.len()
      );
    }
  }
}

/// The expressions a pairing silently drops because the upstream will not parse them.
///
/// [`compare`] skips those, which is right — a parse difference is not a match
/// difference — but a silent skip is an exclusion nobody wrote down, and it is the only
/// kind that can grow on its own. `@midnight` reaches this list from every pairing that
/// offers it, which is to say no upstream here answers for it at all; that is the sort
/// of fact a skip hides and a list states.
///
/// Each entry is `(pairing, expression, why)`, listed in corpus order within a pairing so
/// that a failure reads as a diff against the corpus. `why` is about the upstream, so a
/// version bump is what changes this table.
const UPSTREAM_REFUSALS: &[(&str, &str, &str)] = &[
  (
    "Robfig × cron",
    "0 0 0 * * 0",
    "`cron` numbers Sunday 1 and rejects a 0 in the day-of-week field outright",
  ),
  (
    "Robfig × croner",
    "0 0 0 ?,1 * MON",
    "croner takes `?` as a whole day field and not as one item of a list",
  ),
  (
    "Vixie × saffron",
    "0 0 * * 0",
    "saffron numbers Sunday 1 and rejects a 0 in the day-of-week field, as `cron` does",
  ),
  (
    "Quartz × croner",
    "0 0 0 LW * ?",
    "croner has `L` and `W` but not the two together",
  ),
  (
    "Quartz × croner",
    "0 0 0 ? * MON 2026,*",
    "croner takes a `*` only as a whole field, in the year as in the day fields — the \
     same `field == \"*\"` test that makes it refuse `*,10`. No upstream here has a year \
     field and a `*` inside a list, so `the_wildcard_year_carries_its_oracle` is what \
     holds these three to one",
  ),
  (
    "Quartz × croner",
    "0 0 0 ? * MON *,2026",
    "as above, with the wildcard first",
  ),
  (
    "Quartz × croner",
    "0 0 0 1 1 ? *,*",
    "as above, with nothing but wildcards",
  ),
  (
    "Vixie nicknames × cron",
    "@annually",
    "`cron`'s nickname table has `@yearly` and not its synonym",
  ),
  (
    "Vixie nicknames × cron",
    "@midnight",
    "`cron`'s nickname table has `@daily` and not its synonym",
  ),
  (
    "Vixie nicknames × croner",
    "@midnight",
    "croner's nickname table has `@daily` and not its synonym",
  ),
  (
    "Robfig nicknames × cron",
    "@annually",
    "as the `Vixie` pairing above: the same upstream and the same table",
  ),
  (
    "Robfig nicknames × cron",
    "@midnight",
    "as the `Vixie` pairing above: the same upstream and the same table",
  ),
];

/// One side of a pairing: an expression in, a [`Matcher`] out, and `None` where that side
/// refuses to parse it.
type Side = Box<dyn Fn(&str) -> Option<Matcher>>;

/// One pairing, in the shape [`the_upstream_refusals_are_the_declared_ones`] walks it.
struct Pairing {
  /// The label [`compare`] prints for it.
  label: &'static str,
  /// The corpus it is run over, already filtered by the corpus tags.
  expressions: Vec<String>,
  /// This crate, in the pairing's dialect.
  ours: Side,
  /// The upstream.
  theirs: Side,
}

/// The nine pairings above, as data. The labels and corpora are the ones the pairing
/// tests use, so a corpus that changes there changes here.
fn pairings() -> Vec<Pairing> {
  let nicknames: Vec<String> = NICKNAMES.iter().map(|&name| String::from(name)).collect();
  let quartz: Vec<String> = QUARTZ_CASES
    .iter()
    .map(|&case| String::from(case))
    .collect();
  let pairing = |label, expressions, ours, theirs| Pairing {
    label,
    expressions,
    ours,
    theirs,
  };

  vec![
    pairing(
      "Vixie × cronexpr",
      five_field(|_| true),
      Box::new(as_cronp::<Vixie, 1>),
      Box::new(as_cronexpr),
    ),
    pairing(
      "Cronexpr × cronexpr",
      five_field(|_| true),
      Box::new(as_cronp::<Cronexpr, 1>),
      Box::new(as_cronexpr),
    ),
    pairing(
      "Robfig × cron",
      six_field(|case| case.cron),
      Box::new(as_cronp::<Robfig, 1>),
      Box::new(as_cron),
    ),
    pairing(
      "Robfig × croner",
      six_field(|case| case.croner),
      Box::new(as_cronp::<Robfig, 1>),
      Box::new(|expression| {
        as_croner(
          expression,
          CronParser::builder().seconds(Seconds::Required).build(),
        )
      }),
    ),
    pairing(
      "Vixie × saffron",
      five_field(|case| case.saffron),
      Box::new(as_cronp::<Vixie, 1>),
      Box::new(as_saffron),
    ),
    pairing(
      "Quartz × croner",
      quartz,
      Box::new(as_cronp::<Quartz, 1>),
      Box::new(|expression| {
        as_croner(
          expression,
          CronParser::builder()
            .seconds(Seconds::Required)
            .dom_and_dow(true)
            .alternative_weekdays(true)
            .build(),
        )
      }),
    ),
    pairing(
      "Vixie nicknames × cron",
      nicknames.clone(),
      Box::new(as_cronp::<Vixie, 1>),
      Box::new(as_cron),
    ),
    pairing(
      "Vixie nicknames × croner",
      nicknames.clone(),
      Box::new(as_cronp::<Vixie, 1>),
      Box::new(|expression| as_croner(expression, CronParser::new())),
    ),
    pairing(
      "Robfig nicknames × cron",
      nicknames,
      Box::new(as_cronp::<Robfig, 1>),
      Box::new(as_cron),
    ),
  ]
}

/// The upstreams refuse what [`UPSTREAM_REFUSALS`] says they refuse, and nothing else.
///
/// Failing in either direction is the point. A refusal that appears means a pairing has
/// quietly stopped comparing a row; a refusal that disappears means the upstream grew a
/// construct and the row is being compared again, which is coverage worth knowing about.
#[test]
fn the_upstream_refusals_are_the_declared_ones() {
  for pairing in pairings() {
    let declared: Vec<&str> = UPSTREAM_REFUSALS
      .iter()
      .filter(|&&(label, ..)| label == pairing.label)
      .map(|&(_, expression, _)| expression)
      .collect();
    // Only the expressions this crate parses: where both sides refuse, the skip is this
    // crate's grammar and not the upstream's, and the dialects' grammars are gated by the
    // crate's own tests.
    let found: Vec<&str> = pairing
      .expressions
      .iter()
      .filter(|expression| (pairing.ours)(expression).is_some())
      .filter(|expression| (pairing.theirs)(expression).is_none())
      .map(String::as_str)
      .collect();
    assert_eq!(
      found, declared,
      "{}: the upstream refusals moved. `UPSTREAM_REFUSALS` is what the pairing is not \
       comparing, and it is out of date",
      pairing.label
    );
  }

  // An entry naming a pairing that no longer exists would sit here excluding nothing, and
  // the equality above cannot see it: there is no pairing with that label to compare it
  // against. An entry whose expression has left the corpus is caught above instead, as a
  // `declared` with no `found` beside it.
  let labels: Vec<&str> = pairings()
    .into_iter()
    .map(|pairing| pairing.label)
    .collect();
  for &(label, expression, why) in UPSTREAM_REFUSALS {
    assert!(
      labels.contains(&label),
      "{label:?} is not a pairing any more, so {expression:?} is excluded from nothing"
    );
    assert!(
      !why.is_empty(),
      "{label} {expression:?}: a refusal needs a reason"
    );
  }
}

/// `@midnight` and `@annually` reach an oracle through the nickname they are a synonym
/// of.
///
/// [`UPSTREAM_REFUSALS`] records that no upstream here parses `@midnight`, and that
/// `cron` does not parse `@annually` — so the nickname pairings compare neither, in the
/// dialect where `cron` is the only upstream. Vixie's `entry.c` makes each the same entry
/// as the nickname beside it in its table, and that is a claim this crate can be held to:
/// if `@midnight` is `@daily` and `@daily` is oracled, `@midnight` is oracled too.
///
/// Written as an identity over the whole sweep rather than as a spot check, because the
/// two nicknames differing on a single instant is exactly the defect this would be for.
/// A year field whose union is every year reaches an oracle through the `*` it equals.
///
/// [`UPSTREAM_REFUSALS`] records that croner — the only upstream here with a year field
/// at all — takes a `*` only as a whole field, so `2026,*` reaches no upstream directly.
/// It does reach one by identity: a list containing a bare wildcard admits every year,
/// which is what a `*` year field admits and what an absent one admits, and croner
/// answers for both of those. If `? * MON *` is oracled and `? * MON 2026,*` is the same
/// schedule, then `? * MON 2026,*` is oracled too.
///
/// This is the row the repair is about, so the identity is asserted over the whole sweep
/// rather than spot-checked, and the oracled member of each group is compared with
/// croner in the same test — an identity between two wrong answers proves nothing.
#[test]
#[cfg_attr(
  miri,
  ignore = "a year of matches per spelling through croner and chrono; the identity is a \
            property of the union each spelling denotes, not of the machine it is \
            interpreted on"
)]
fn the_wildcard_year_carries_its_oracle() {
  let when = instants();
  let parser = || {
    CronParser::builder()
      .seconds(Seconds::Required)
      .dom_and_dow(true)
      .alternative_weekdays(true)
      .build()
  };

  // Each group is one schedule written several ways. The first spelling is the one
  // croner answers for; the rest are the ones it refuses.
  const GROUPS: &[&[&str]] = &[
    &[
      "0 0 0 ? * MON",
      "0 0 0 ? * MON *",
      "0 0 0 ? * MON 2026,*",
      "0 0 0 ? * MON *,2026",
      "0 0 0 ? * MON *,*",
      "0 0 0 ? * MON */1,2026",
    ],
    &[
      "0 0 0 1 1 ?",
      "0 0 0 1 1 ? *",
      "0 0 0 1 1 ? *,*",
      "0 0 0 1 1 ? 2026,*",
    ],
  ];

  for group in GROUPS {
    let (&oracled, rest) = group.split_first().expect("a group has a first spelling");
    let ours = as_cronp::<Quartz, 1>(oracled)
      .unwrap_or_else(|| panic!("{oracled:?} must parse at the default width"));
    let theirs =
      as_croner(oracled, parser()).unwrap_or_else(|| panic!("croner must answer for {oracled:?}"));

    // The oracled member, against something that is not this crate.
    for &instant in &when {
      assert_eq!(
        ours(instant),
        theirs(instant),
        "{oracled:?} at {instant}: cronp says {}, croner says {}",
        ours(instant),
        theirs(instant)
      );
    }

    // And every other spelling of the same schedule, against it. A refusal here is the
    // defect this test exists for: each of these used to be rejected outright at
    // `Schedule<Quartz, 1>` with `YearNotRepresentable` at 2098.
    for &spelling in rest {
      let same = as_cronp::<Quartz, 1>(spelling).unwrap_or_else(|| {
        panic!(
          "{spelling:?} places no year restriction, so it must parse at the default \
           width — and it must not need the oracle to be told so"
        )
      });
      for &instant in &when {
        assert_eq!(
          same(instant),
          ours(instant),
          "{spelling:?} and {oracled:?} are the same schedule and disagree at {instant}"
        );
      }
    }
  }
}

#[test]
#[cfg_attr(
  miri,
  ignore = "a year of matches per nickname; the identity is a property of the expansion, \
            not of the machine it is interpreted on"
)]
fn the_nickname_synonyms_carry_their_oracle() {
  let when = instants();
  for (synonym, oracled) in [("@midnight", "@daily"), ("@annually", "@yearly")] {
    for (dialect, ours) in [
      (
        "Vixie",
        (as_cronp::<Vixie, 1>(synonym), as_cronp::<Vixie, 1>(oracled)),
      ),
      (
        "Robfig",
        (
          as_cronp::<Robfig, 1>(synonym),
          as_cronp::<Robfig, 1>(oracled),
        ),
      ),
    ] {
      let (synonym_matches, oracled_matches) = ours;
      let synonym_matches =
        synonym_matches.unwrap_or_else(|| panic!("{dialect} takes {synonym:?}"));
      let oracled_matches =
        oracled_matches.unwrap_or_else(|| panic!("{dialect} takes {oracled:?}"));
      for &instant in &when {
        assert_eq!(
          synonym_matches(instant),
          oracled_matches(instant),
          "{dialect}: {synonym:?} and {oracled:?} are the same entry in vixie's table, \
           and they answer differently at {instant}. No upstream here parses {synonym:?}, \
           so this identity is the only thing holding it to one"
        );
      }
    }
  }
}
