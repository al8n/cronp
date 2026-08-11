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
//! - **`Vixie` × `cronexpr` 1.6.** cronexpr *is* Vixie plus two constructs: five fields,
//!   Sunday as `0` with `7` also Sunday, the union rule, and the same wildcard —
//!   `input.starts_with('*')`, which is what vixie's `entry.c` tests on the field's first
//!   character. Instants are taken at second zero only, because cronexpr's `matches`
//!   never looks at the seconds component of the timestamp it is given.
//! - **`Cronexpr` × `cronexpr` 1.6.** The same, with `L`, `nW`, `nL` and `n#m` in play.
//!   `L-n` is skipped by cronexpr's own refusal of it, which is the one grammar
//!   difference this crate documents.
//! - **`Robfig` × `cron` 0.17.** Six fields, `?` as another spelling of `*`, a step after
//!   a bare value. `cron` has no union rule at all — `includes` ANDs every field — so it
//!   answers for a robfig expression exactly where robfig's `starBit` makes robfig
//!   intersect too. Where neither day field carries the bit, robfig unions and `cron`
//!   does not; those rows are marked `cron: false`. It also numbers Sunday `1`, so it is
//!   asked only about expressions whose day-of-week field carries no digit — see
//!   [`weekday_is_not_a_digit`].
//! - **`Robfig` × `croner` 3.** croner implements the union rule with `star_dom` and
//!   `star_dow`, and computes each as `field == "*"` after replacing `?` with `*`. That
//!   is robfig's answer wherever a `*` is a whole day field or absent from it, and not
//!   where a `*` shares a field with anything — `10,*` and `*/1` — so those rows are
//!   marked `croner: false`.
//! - **`Vixie` × `saffron` 0.1.** A five-field Vixie parser with the same union rule,
//!   keyed on the day field having parsed to its "all" shape. That is Vixie's answer for
//!   a field that is exactly `*` or has no `*` in it, and not for `*/2`, which saffron
//!   unions and Vixie intersects. It numbers Sunday `1` as `cron` does, so the same
//!   day-of-week restriction applies. Second zero only, for the same reason as cronexpr.
//! - **`Quartz` × `croner` 3**, in `alternative_weekdays` mode with seconds required and
//!   `dom_and_dow` set: Sunday as `1`, and both day fields required to match. Quartz's
//!   `?` is croner's `*` after the replacement above, so "both must match" is what
//!   reading the one specified field amounts to — which is Quartz's rule.
//! - **The nicknames × `cron` 0.17 and `croner` 3.** cronexpr's FAQ declines every
//!   nickname and saffron has none, so these two are the upstreams for `@weekly` and its
//!   siblings. Both expand them as vixie does and both intersect the day fields.

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
  /// Whether `cron` 0.17 answers for this expression under `Robfig`.
  cron: bool,
  /// Whether `croner` 3 answers for this expression under `Robfig`.
  croner: bool,
  /// Whether `saffron` 0.1 answers for this expression under `Vixie`.
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

/// Whether `cron` 0.17 and `saffron` 0.1 answer for this expression at all.
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
fn weekday_is_not_a_digit(expression: &str, _when: CivilDateTime) -> bool {
  !day_of_week_field(expression)
    .bytes()
    .any(|byte| byte.is_ascii_digit())
}

/// Whether `cronexpr` 1.6 answers for this expression at this instant.
///
/// Its last-of-month predicates ask whether `(value + 1.day()).month() > value.month()`,
/// and in December the next day's month is January: `1 > 12` is false, so `L` and `nL`
/// fire on no day of December at all there. Every other month is unaffected, and so is
/// every other predicate — `nW` and `n#m` compare days within the month.
fn cronexpr_answers(expression: &str, when: CivilDateTime) -> bool {
  let last_of_something = expression
    .split_whitespace()
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
