//! Parsing throughput, per dialect and per shape of expression.

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use cronp::{Quartz, Robfig, Schedule, Vixie};

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

criterion_group!(benches, parsing);
criterion_main!(benches);
