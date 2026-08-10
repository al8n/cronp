//! Parsing throughput.
//!
//! Two things live here. The `parse` group measures cronp alone, per dialect and per
//! shape of expression. The `compare/*` groups measure cronp against `saffron`, `cron`
//! and `croner` on the six shapes the README's benchmark table reports — one group per
//! shape, one function per parser inside it, so criterion renders them side by side.
//!
//! Parse only, for all four: text in, a schedule out, dropped inside the timed region.
//! No matching and no next-occurrence computation. Every parser here does its whole job
//! in one call — none of them hands back a lazily-evaluated handle — so the cells are
//! like for like.

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use cronp::{Quartz, Robfig, Schedule, Vixie};

// The six expressions behind the README's comparison table. They are the table's
// identity: the published numbers describe these strings and nothing else, so a new
// shape belongs in a new row rather than inside one of these.
const FIVE_FIELD: &str = "30 2 * * 1-5";
const FIVE_FIELD_DENSE: &str = "0,15,30,45 0-23/2 1-15 JAN-JUN MON-FRI";
const SIX_FIELD_SECONDS: &str = "0 30 2 * * 1-5";
const SEVEN_FIELD_QUARTZ: &str = "0 15 10 ? * MON-FRI 2020-2030";
const NICKNAME: &str = "@daily";
const REJECTED: &str = "0 0 * * 99";

// Criterion group ids, reused verbatim as the row labels in the acceptance check below,
// so a failure names something that can be handed straight back to `cargo bench --`.
const ROW_FIVE_FIELD: &str = "compare/5-field";
const ROW_FIVE_FIELD_DENSE: &str = "compare/5-field lists, steps, names";
const ROW_SIX_FIELD_SECONDS: &str = "compare/6-field leading seconds";
const ROW_SEVEN_FIELD_QUARTZ: &str = "compare/7-field Quartz with year";
const ROW_NICKNAME: &str = "compare/nickname";
const ROW_REJECTED: &str = "compare/rejected";

/// What the README's comparison table says about one cell.
#[derive(Clone, Copy)]
enum Claim {
  /// The row times this parser building a schedule.
  Parses,
  /// The rejection row, where the parser is timed failing instead.
  Rejects,
  /// The cell is blank, and the table's stated reason is that this parser does not take
  /// the shape at all. That sentence is a claim about the other crates like any other,
  /// so it is checked like any other.
  Absent,
}

impl Claim {
  /// Whether the claim requires the parser to have built something.
  const fn wants_success(self) -> bool {
    matches!(self, Self::Parses)
  }

  /// What the row does with the cell, for the failure message.
  const fn treatment(self) -> &'static str {
    match self {
      Self::Parses => "times as a parse",
      Self::Rejects => "times as a rejection",
      Self::Absent => "leaves blank on the grounds that the parser cannot take that shape",
    }
  }
}

/// One cell of the README's comparison table.
struct Cell {
  /// The row, labelled as its criterion group is.
  row: &'static str,
  /// The expression that row is measured on.
  expr: &'static str,
  /// The parser named in the column.
  parser: &'static str,
  /// What the table says about this cell.
  claim: Claim,
  /// Whether the parser accepted the expression on this run.
  observed: bool,
}

fn parses(row: &'static str, expr: &'static str, parser: &'static str, observed: bool) -> Cell {
  Cell {
    row,
    expr,
    parser,
    claim: Claim::Parses,
    observed,
  }
}

fn rejects(row: &'static str, expr: &'static str, parser: &'static str, observed: bool) -> Cell {
  Cell {
    row,
    expr,
    parser,
    claim: Claim::Rejects,
    observed,
  }
}

fn absent(row: &'static str, expr: &'static str, parser: &'static str, observed: bool) -> Cell {
  Cell {
    row,
    expr,
    parser,
    claim: Claim::Absent,
    observed,
  }
}

/// Checks every cell of the comparison table before anything is timed.
///
/// A parse and a rejection are not the same measurement: a parser that rejects stops at
/// the first error and finishes far sooner than one that builds a schedule. Without this
/// check, a dependency bump that changed what one of these crates accepts would quietly
/// turn a cell into an error-path timing while the README went on reporting it as a
/// parse. Registered ahead of every benchmark group so it runs before the first sample.
///
/// The blank cells are checked too. "A library appears in a row only where it accepts
/// that shape" is the sentence that licenses the ragged columns, and it stops being true
/// the moment one of these parsers grows a shape it used to refuse.
fn acceptance_preflight(_: &mut Criterion) {
  let cells = [
    parses(
      ROW_FIVE_FIELD,
      FIVE_FIELD,
      "cronp",
      Schedule::<Vixie>::parse(FIVE_FIELD).is_ok(),
    ),
    parses(
      ROW_FIVE_FIELD,
      FIVE_FIELD,
      "saffron",
      FIVE_FIELD.parse::<saffron::Cron>().is_ok(),
    ),
    parses(
      ROW_FIVE_FIELD,
      FIVE_FIELD,
      "croner",
      FIVE_FIELD.parse::<croner::Cron>().is_ok(),
    ),
    parses(
      ROW_FIVE_FIELD_DENSE,
      FIVE_FIELD_DENSE,
      "cronp",
      Schedule::<Vixie>::parse(FIVE_FIELD_DENSE).is_ok(),
    ),
    parses(
      ROW_FIVE_FIELD_DENSE,
      FIVE_FIELD_DENSE,
      "saffron",
      FIVE_FIELD_DENSE.parse::<saffron::Cron>().is_ok(),
    ),
    parses(
      ROW_FIVE_FIELD_DENSE,
      FIVE_FIELD_DENSE,
      "croner",
      FIVE_FIELD_DENSE.parse::<croner::Cron>().is_ok(),
    ),
    parses(
      ROW_SIX_FIELD_SECONDS,
      SIX_FIELD_SECONDS,
      "cronp",
      Schedule::<Robfig>::parse(SIX_FIELD_SECONDS).is_ok(),
    ),
    parses(
      ROW_SIX_FIELD_SECONDS,
      SIX_FIELD_SECONDS,
      "cron",
      SIX_FIELD_SECONDS.parse::<cron::Schedule>().is_ok(),
    ),
    parses(
      ROW_SIX_FIELD_SECONDS,
      SIX_FIELD_SECONDS,
      "croner",
      SIX_FIELD_SECONDS.parse::<croner::Cron>().is_ok(),
    ),
    parses(
      ROW_SEVEN_FIELD_QUARTZ,
      SEVEN_FIELD_QUARTZ,
      "cronp",
      Schedule::<Quartz>::parse(SEVEN_FIELD_QUARTZ).is_ok(),
    ),
    parses(
      ROW_SEVEN_FIELD_QUARTZ,
      SEVEN_FIELD_QUARTZ,
      "cron",
      SEVEN_FIELD_QUARTZ.parse::<cron::Schedule>().is_ok(),
    ),
    parses(
      ROW_SEVEN_FIELD_QUARTZ,
      SEVEN_FIELD_QUARTZ,
      "croner",
      SEVEN_FIELD_QUARTZ.parse::<croner::Cron>().is_ok(),
    ),
    parses(
      ROW_NICKNAME,
      NICKNAME,
      "cronp",
      Schedule::<Vixie>::parse(NICKNAME).is_ok(),
    ),
    parses(
      ROW_NICKNAME,
      NICKNAME,
      "cron",
      NICKNAME.parse::<cron::Schedule>().is_ok(),
    ),
    parses(
      ROW_NICKNAME,
      NICKNAME,
      "croner",
      NICKNAME.parse::<croner::Cron>().is_ok(),
    ),
    rejects(
      ROW_REJECTED,
      REJECTED,
      "cronp",
      Schedule::<Vixie>::parse(REJECTED).is_ok(),
    ),
    rejects(
      ROW_REJECTED,
      REJECTED,
      "saffron",
      REJECTED.parse::<saffron::Cron>().is_ok(),
    ),
    rejects(
      ROW_REJECTED,
      REJECTED,
      "cron",
      REJECTED.parse::<cron::Schedule>().is_ok(),
    ),
    rejects(
      ROW_REJECTED,
      REJECTED,
      "croner",
      REJECTED.parse::<croner::Cron>().is_ok(),
    ),
    absent(
      ROW_FIVE_FIELD,
      FIVE_FIELD,
      "cron",
      FIVE_FIELD.parse::<cron::Schedule>().is_ok(),
    ),
    absent(
      ROW_FIVE_FIELD_DENSE,
      FIVE_FIELD_DENSE,
      "cron",
      FIVE_FIELD_DENSE.parse::<cron::Schedule>().is_ok(),
    ),
    absent(
      ROW_SIX_FIELD_SECONDS,
      SIX_FIELD_SECONDS,
      "saffron",
      SIX_FIELD_SECONDS.parse::<saffron::Cron>().is_ok(),
    ),
    absent(
      ROW_SEVEN_FIELD_QUARTZ,
      SEVEN_FIELD_QUARTZ,
      "saffron",
      SEVEN_FIELD_QUARTZ.parse::<saffron::Cron>().is_ok(),
    ),
    absent(
      ROW_NICKNAME,
      NICKNAME,
      "saffron",
      NICKNAME.parse::<saffron::Cron>().is_ok(),
    ),
  ];

  let wrong = cells
    .iter()
    .filter(|cell| cell.claim.wants_success() != cell.observed)
    .map(|cell| {
      let happened = if cell.observed {
        "accepted"
      } else {
        "rejected"
      };
      format!(
        "  {} {} `{}`, which row `{}` {}",
        cell.parser,
        happened,
        cell.expr,
        cell.row,
        cell.claim.treatment()
      )
    })
    .collect::<Vec<_>>();

  assert!(
    wrong.is_empty(),
    "these comparison rows no longer measure what the README's table reports:\n{}",
    wrong.join("\n")
  );
}

fn parsing(c: &mut Criterion) {
  let mut group = c.benchmark_group("parse");

  group.bench_function("vixie/five fields", |b| {
    b.iter(|| Schedule::<Vixie>::parse(black_box("30 2 * * 1-5")));
  });
  group.bench_function("vixie/nickname", |b| {
    b.iter(|| Schedule::<Vixie>::parse(black_box("@daily")));
  });
  group.bench_function("vixie/lists and steps", |b| {
    b.iter(|| Schedule::<Vixie>::parse(black_box("0,15,30,45 0-23/2 1-15 JAN-JUN MON-FRI")));
  });
  group.bench_function("quartz/seven fields", |b| {
    b.iter(|| Schedule::<Quartz>::parse(black_box("0 15 10 ? * MON-FRI 2020-2030")));
  });
  group.bench_function("quartz/date predicate", |b| {
    b.iter(|| Schedule::<Quartz>::parse(black_box("0 15 10 LW * ?")));
  });
  group.bench_function("robfig/every", |b| {
    b.iter(|| Schedule::<Robfig>::parse(black_box("@every 1h30m45s")));
  });
  group.bench_function("vixie/rejected", |b| {
    b.iter(|| Schedule::<Vixie>::parse(black_box("0 0 * * 99")));
  });

  group.finish();
}

/// The six rows of the README's comparison table.
///
/// A parser appears in a row only where it accepts that shape, which is why the columns
/// are ragged: `saffron` takes five fields and nothing else, and `cron` will not take a
/// bare five-field expression. Timing a rejection against a parse would compare an error
/// return with a built schedule, so those cells are absent rather than zero.
fn comparison(c: &mut Criterion) {
  let mut five_field = c.benchmark_group(ROW_FIVE_FIELD);
  five_field.bench_function("cronp", |b| {
    b.iter(|| Schedule::<Vixie>::parse(black_box(FIVE_FIELD)));
  });
  five_field.bench_function("saffron", |b| {
    b.iter(|| black_box(FIVE_FIELD).parse::<saffron::Cron>());
  });
  five_field.bench_function("croner", |b| {
    b.iter(|| black_box(FIVE_FIELD).parse::<croner::Cron>());
  });
  five_field.finish();

  let mut dense = c.benchmark_group(ROW_FIVE_FIELD_DENSE);
  dense.bench_function("cronp", |b| {
    b.iter(|| Schedule::<Vixie>::parse(black_box(FIVE_FIELD_DENSE)));
  });
  dense.bench_function("saffron", |b| {
    b.iter(|| black_box(FIVE_FIELD_DENSE).parse::<saffron::Cron>());
  });
  dense.bench_function("croner", |b| {
    b.iter(|| black_box(FIVE_FIELD_DENSE).parse::<croner::Cron>());
  });
  dense.finish();

  let mut seconds = c.benchmark_group(ROW_SIX_FIELD_SECONDS);
  seconds.bench_function("cronp", |b| {
    b.iter(|| Schedule::<Robfig>::parse(black_box(SIX_FIELD_SECONDS)));
  });
  seconds.bench_function("cron", |b| {
    b.iter(|| black_box(SIX_FIELD_SECONDS).parse::<cron::Schedule>());
  });
  seconds.bench_function("croner", |b| {
    b.iter(|| black_box(SIX_FIELD_SECONDS).parse::<croner::Cron>());
  });
  seconds.finish();

  let mut quartz = c.benchmark_group(ROW_SEVEN_FIELD_QUARTZ);
  quartz.bench_function("cronp", |b| {
    b.iter(|| Schedule::<Quartz>::parse(black_box(SEVEN_FIELD_QUARTZ)));
  });
  quartz.bench_function("cron", |b| {
    b.iter(|| black_box(SEVEN_FIELD_QUARTZ).parse::<cron::Schedule>());
  });
  quartz.bench_function("croner", |b| {
    b.iter(|| black_box(SEVEN_FIELD_QUARTZ).parse::<croner::Cron>());
  });
  quartz.finish();

  let mut nickname = c.benchmark_group(ROW_NICKNAME);
  nickname.bench_function("cronp", |b| {
    b.iter(|| Schedule::<Vixie>::parse(black_box(NICKNAME)));
  });
  nickname.bench_function("cron", |b| {
    b.iter(|| black_box(NICKNAME).parse::<cron::Schedule>());
  });
  nickname.bench_function("croner", |b| {
    b.iter(|| black_box(NICKNAME).parse::<croner::Cron>());
  });
  nickname.finish();

  // Not comparable with the rows above: every parser here stops at its first error, so
  // this measures four rejection paths against each other and nothing else.
  let mut rejected = c.benchmark_group(ROW_REJECTED);
  rejected.bench_function("cronp", |b| {
    b.iter(|| Schedule::<Vixie>::parse(black_box(REJECTED)));
  });
  rejected.bench_function("saffron", |b| {
    b.iter(|| black_box(REJECTED).parse::<saffron::Cron>());
  });
  rejected.bench_function("cron", |b| {
    b.iter(|| black_box(REJECTED).parse::<cron::Schedule>());
  });
  rejected.bench_function("croner", |b| {
    b.iter(|| black_box(REJECTED).parse::<croner::Cron>());
  });
  rejected.finish();
}

criterion_group!(benches, acceptance_preflight, parsing, comparison);
criterion_main!(benches);
