<div align="center">
<h1>cronp</h1>
</div>
<div align="center">

A blazing fast no-std, no-alloc cron expression parser, with multiple dialects support.

[<img alt="github" src="https://img.shields.io/badge/github-al8n/cronp-8da0cb?style=for-the-badge&logo=Github" height="22">][Github-url]
<img alt="LoC" src="https://img.shields.io/endpoint?url=https%3A%2F%2Fgist.githubusercontent.com%2Fal8n%2F327b2a8aef9003246e45c6e47fe63937%2Fraw%2Fcronp" height="22">
[<img alt="Build" src="https://img.shields.io/github/actions/workflow/status/al8n/cronp/ci.yml?logo=Github-Actions&style=for-the-badge" height="22">][CI-url]
[<img alt="codecov" src="https://img.shields.io/codecov/c/gh/al8n/cronp?style=for-the-badge&token=6R3QFWRWHL&logo=codecov" height="22">][codecov-url]

[<img alt="docs.rs" src="https://img.shields.io/badge/docs.rs-cronp-66c2a5?style=for-the-badge&labelColor=555555&logo=data:image/svg+xml;base64,PHN2ZyByb2xlPSJpbWciIHhtbG5zPSJodHRwOi8vd3d3LnczLm9yZy8yMDAwL3N2ZyIgdmlld0JveD0iMCAwIDUxMiA1MTIiPjxwYXRoIGZpbGw9IiNmNWY1ZjUiIGQ9Ik00ODguNiAyNTAuMkwzOTIgMjE0VjEwNS41YzAtMTUtOS4zLTI4LjQtMjMuNC0zMy43bC0xMDAtMzcuNWMtOC4xLTMuMS0xNy4xLTMuMS0yNS4zIDBsLTEwMCAzNy41Yy0xNC4xIDUuMy0yMy40IDE4LjctMjMuNCAzMy43VjIxNGwtOTYuNiAzNi4yQzkuMyAyNTUuNSAwIDI2OC45IDAgMjgzLjlWMzk0YzAgMTMuNiA3LjcgMjYuMSAxOS45IDMyLjJsMTAwIDUwYzEwLjEgNS4xIDIyLjEgNS4xIDMyLjIgMGwxMDMuOS01MiAxMDMuOSA1MmMxMC4xIDUuMSAyMi4xIDUuMSAzMi4yIDBsMTAwLTUwYzEyLjItNi4xIDE5LjktMTguNiAxOS45LTMyLjJWMjgzLjljMC0xNS05LjMtMjguNC0yMy40LTMzLjd6TTM1OCAyMTQuOGwtODUgMzEuOXYtNjguMmw4NS0zN3Y3My4zek0xNTQgMTA0LjFsMTAyLTM4LjIgMTAyIDM4LjJ2LjZsLTEwMiA0MS40LTEwMi00MS40di0uNnptODQgMjkxLjFsLTg1IDQyLjV2LTc5LjFsODUtMzguOHY3NS40em0wLTExMmwtMTAyIDQxLjQtMTAyLTQxLjR2LS42bDEwMi0zOC4yIDEwMiAzOC4ydi42em0yNDAgMTEybC04NSA0Mi41di03OS4xbDg1LTM4Ljh2NzUuNHptMC0xMTJsLTEwMiA0MS40LTEwMi00MS40di0uNmwxMDItMzguMiAxMDIgMzguMnYuNnoiPjwvcGF0aD48L3N2Zz4K" height="20">][doc-url]
[<img alt="crates.io" src="https://img.shields.io/crates/v/cronp?style=for-the-badge&logo=data:image/svg+xml;base64,PD94bWwgdmVyc2lvbj0iMS4wIiBlbmNvZGluZz0iaXNvLTg4NTktMSI/Pg0KPCEtLSBHZW5lcmF0b3I6IEFkb2JlIElsbHVzdHJhdG9yIDE5LjAuMCwgU1ZHIEV4cG9ydCBQbHVnLUluIC4gU1ZHIFZlcnNpb246IDYuMDAgQnVpbGQgMCkgIC0tPg0KPHN2ZyB2ZXJzaW9uPSIxLjEiIGlkPSJMYXllcl8xIiB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHhtbG5zOnhsaW5rPSJodHRwOi8vd3d3LnczLm9yZy8xOTk5L3hsaW5rIiB4PSIwcHgiIHk9IjBweCINCgkgdmlld0JveD0iMCAwIDUxMiA1MTIiIHhtbDpzcGFjZT0icHJlc2VydmUiPg0KPGc+DQoJPGc+DQoJCTxwYXRoIGQ9Ik0yNTYsMEwzMS41MjgsMTEyLjIzNnYyODcuNTI4TDI1Niw1MTJsMjI0LjQ3Mi0xMTIuMjM2VjExMi4yMzZMMjU2LDB6IE0yMzQuMjc3LDQ1Mi41NjRMNzQuOTc0LDM3Mi45MTNWMTYwLjgxDQoJCQlsMTU5LjMwMyw3OS42NTFWNDUyLjU2NHogTTEwMS44MjYsMTI1LjY2MkwyNTYsNDguNTc2bDE1NC4xNzQsNzcuMDg3TDI1NiwyMDIuNzQ5TDEwMS44MjYsMTI1LjY2MnogTTQzNy4wMjYsMzcyLjkxMw0KCQkJbC0xNTkuMzAzLDc5LjY1MVYyNDAuNDYxbDE1OS4zMDMtNzkuNjUxVjM3Mi45MTN6IiBmaWxsPSIjRkZGIi8+DQoJPC9nPg0KPC9nPg0KPGc+DQo8L2c+DQo8Zz4NCjwvZz4NCjxnPg0KPC9nPg0KPGc+DQo8L2c+DQo8Zz4NCjwvZz4NCjxnPg0KPC9nPg0KPGc+DQo8L2c+DQo8Zz4NCjwvZz4NCjxnPg0KPC9nPg0KPGc+DQo8L2c+DQo8Zz4NCjwvZz4NCjxnPg0KPC9nPg0KPGc+DQo8L2c+DQo8Zz4NCjwvZz4NCjxnPg0KPC9nPg0KPC9zdmc+DQo=" height="22">][crates-url]
[<img alt="crates.io" src="https://img.shields.io/crates/d/cronp?color=critical&logo=data:image/svg+xml;base64,PD94bWwgdmVyc2lvbj0iMS4wIiBzdGFuZGFsb25lPSJubyI/PjwhRE9DVFlQRSBzdmcgUFVCTElDICItLy9XM0MvL0RURCBTVkcgMS4xLy9FTiIgImh0dHA6Ly93d3cudzMub3JnL0dyYXBoaWNzL1NWRy8xLjEvRFREL3N2ZzExLmR0ZCI+PHN2ZyB0PSIxNjQ1MTE3MzMyOTU5IiBjbGFzcz0iaWNvbiIgdmlld0JveD0iMCAwIDEwMjQgMTAyNCIgdmVyc2lvbj0iMS4xIiB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHAtaWQ9IjM0MjEiIGRhdGEtc3BtLWFuY2hvci1pZD0iYTMxM3guNzc4MTA2OS4wLmkzIiB3aWR0aD0iNDgiIGhlaWdodD0iNDgiIHhtbG5zOnhsaW5rPSJodHRwOi8vd3d3LnczLm9yZy8xOTk5L3hsaW5rIj48ZGVmcz48c3R5bGUgdHlwZT0idGV4dC9jc3MiPjwvc3R5bGU+PC9kZWZzPjxwYXRoIGQ9Ik00NjkuMzEyIDU3MC4yNHYtMjU2aDg1LjM3NnYyNTZoMTI4TDUxMiA3NTYuMjg4IDM0MS4zMTIgNTcwLjI0aDEyOHpNMTAyNCA2NDAuMTI4QzEwMjQgNzgyLjkxMiA5MTkuODcyIDg5NiA3ODcuNjQ4IDg5NmgtNTEyQzEyMy45MDQgODk2IDAgNzYxLjYgMCA1OTcuNTA0IDAgNDUxLjk2OCA5NC42NTYgMzMxLjUyIDIyNi40MzIgMzAyLjk3NiAyODQuMTYgMTk1LjQ1NiAzOTEuODA4IDEyOCA1MTIgMTI4YzE1Mi4zMiAwIDI4Mi4xMTIgMTA4LjQxNiAzMjMuMzkyIDI2MS4xMkM5NDEuODg4IDQxMy40NCAxMDI0IDUxOS4wNCAxMDI0IDY0MC4xOTJ6IG0tMjU5LjItMjA1LjMxMmMtMjQuNDQ4LTEyOS4wMjQtMTI4Ljg5Ni0yMjIuNzItMjUyLjgtMjIyLjcyLTk3LjI4IDAtMTgzLjA0IDU3LjM0NC0yMjQuNjQgMTQ3LjQ1NmwtOS4yOCAyMC4yMjQtMjAuOTI4IDIuOTQ0Yy0xMDMuMzYgMTQuNC0xNzguMzY4IDEwNC4zMi0xNzguMzY4IDIxNC43MiAwIDExNy45NTIgODguODMyIDIxNC40IDE5Ni45MjggMjE0LjRoNTEyYzg4LjMyIDAgMTU3LjUwNC03NS4xMzYgMTU3LjUwNC0xNzEuNzEyIDAtODguMDY0LTY1LjkyLTE2NC45MjgtMTQ0Ljk2LTE3MS43NzZsLTI5LjUwNC0yLjU2LTUuODg4LTMwLjk3NnoiIGZpbGw9IiNmZmZmZmYiIHAtaWQ9IjM0MjIiIGRhdGEtc3BtLWFuY2hvci1pZD0iYTMxM3guNzc4MTA2OS4wLmkwIiBjbGFzcz0iIj48L3BhdGg+PC9zdmc+&style=for-the-badge" height="22">][crates-url]
<img alt="license" src="https://img.shields.io/badge/License-Apache%202.0/MIT-blue.svg?style=for-the-badge&fontColor=white&logoColor=f5c076&logo=data:image/svg+xml;base64,PCFET0NUWVBFIHN2ZyBQVUJMSUMgIi0vL1czQy8vRFREIFNWRyAxLjEvL0VOIiAiaHR0cDovL3d3dy53My5vcmcvR3JhcGhpY3MvU1ZHLzEuMS9EVEQvc3ZnMTEuZHRkIj4KDTwhLS0gVXBsb2FkZWQgdG86IFNWRyBSZXBvLCB3d3cuc3ZncmVwby5jb20sIFRyYW5zZm9ybWVkIGJ5OiBTVkcgUmVwbyBNaXhlciBUb29scyAtLT4KPHN2ZyBmaWxsPSIjZmZmZmZmIiBoZWlnaHQ9IjgwMHB4IiB3aWR0aD0iODAwcHgiIHZlcnNpb249IjEuMSIgaWQ9IkNhcGFfMSIgeG1sbnM9Imh0dHA6Ly93d3cudzMub3JnLzIwMDAvc3ZnIiB4bWxuczp4bGluaz0iaHR0cDovL3d3dy53My5vcmcvMTk5OS94bGluayIgdmlld0JveD0iMCAwIDI3Ni43MTUgMjc2LjcxNSIgeG1sOnNwYWNlPSJwcmVzZXJ2ZSIgc3Ryb2tlPSIjZmZmZmZmIj4KDTxnIGlkPSJTVkdSZXBvX2JnQ2FycmllciIgc3Ryb2tlLXdpZHRoPSIwIi8+Cg08ZyBpZD0iU1ZHUmVwb190cmFjZXJDYXJyaWVyIiBzdHJva2UtbGluZWNhcD0icm91bmQiIHN0cm9rZS1saW5lam9pbj0icm91bmQiLz4KDTxnIGlkPSJTVkdSZXBvX2ljb25DYXJyaWVyIj4gPGc+IDxwYXRoIGQ9Ik0xMzguMzU3LDBDNjIuMDY2LDAsMCw2Mi4wNjYsMCwxMzguMzU3czYyLjA2NiwxMzguMzU3LDEzOC4zNTcsMTM4LjM1N3MxMzguMzU3LTYyLjA2NiwxMzguMzU3LTEzOC4zNTcgUzIxNC42NDgsMCwxMzguMzU3LDB6IE0xMzguMzU3LDI1OC43MTVDNzEuOTkyLDI1OC43MTUsMTgsMjA0LjcyMywxOCwxMzguMzU3UzcxLjk5MiwxOCwxMzguMzU3LDE4IHMxMjAuMzU3LDUzLjk5MiwxMjAuMzU3LDEyMC4zNTdTMjA0LjcyMywyNTguNzE1LDEzOC4zNTcsMjU4LjcxNXoiLz4gPHBhdGggZD0iTTE5NC43OTgsMTYwLjkwM2MtNC4xODgtMi42NzctOS43NTMtMS40NTQtMTIuNDMyLDIuNzMyYy04LjY5NCwxMy41OTMtMjMuNTAzLDIxLjcwOC0zOS42MTQsMjEuNzA4IGMtMjUuOTA4LDAtNDYuOTg1LTIxLjA3OC00Ni45ODUtNDYuOTg2czIxLjA3Ny00Ni45ODYsNDYuOTg1LTQ2Ljk4NmMxNS42MzMsMCwzMC4yLDcuNzQ3LDM4Ljk2OCwyMC43MjMgYzIuNzgyLDQuMTE3LDguMzc1LDUuMjAxLDEyLjQ5NiwyLjQxOGM0LjExOC0yLjc4Miw1LjIwMS04LjM3NywyLjQxOC0xMi40OTZjLTEyLjExOC0xNy45MzctMzIuMjYyLTI4LjY0NS01My44ODItMjguNjQ1IGMtMzUuODMzLDAtNjQuOTg1LDI5LjE1Mi02NC45ODUsNjQuOTg2czI5LjE1Miw2NC45ODYsNjQuOTg1LDY0Ljk4NmMyMi4yODEsMCw0Mi43NTktMTEuMjE4LDU0Ljc3OC0zMC4wMDkgQzIwMC4yMDgsMTY5LjE0NywxOTguOTg1LDE2My41ODIsMTk0Ljc5OCwxNjAuOTAzeiIvPiA8L2c+IDwvZz4KDTwvc3ZnPg==" height="22">

[<img alt="Discord" src="https://img.shields.io/discord/835936528140206122?style=for-the-badge&logo=discord&logoColor=white&label=Discord&color=7289da" height="22">][discord]

</div>

## Installation

```toml
[dependencies]
cronp = "0.3"
```

## What it does

Parses a cron expression into a fixed-size value that never allocates, and answers
whether a caller-supplied instant matches it. No clock, no calendar arithmetic, and
nothing behind a feature on the default path.

The dialect is a **type parameter**, not a runtime tag, because the dialects disagree
about what a stored schedule *means* and not merely about what text they accept:

```rust
use cronp::{CivilDateTime, DateError, ParseError, Quartz, Schedule, Vixie};

fn example() -> Result<(), ParseError> {
    // Five fields, the shape crontab(5) takes.
    let nightly: Schedule<Vixie> = Schedule::parse("30 2 * * 1-5")?;

    // Six or seven, a leading seconds field, and `?` in one of the two day fields.
    let quartz = Schedule::<Quartz>::parse("0 15 10 ? * MON-FRI")?;

    // Five fields is not Quartz, and the error says so.
    assert!(Schedule::<Quartz>::parse("30 2 * * 1-5").is_err());

    // And the question a schedule exists to answer. 2026-08-12 is a Wednesday.
    let when = CivilDateTime::new(2026, 8, 12, 2, 30, 0).map_err(as_parse_error)?;
    assert!(nightly.matches(when));

    let _ = quartz;
    Ok(())
}

// The example returns one error type; a date that does not exist is a different failure
// from an expression that does not parse.
fn as_parse_error(_: DateError) -> ParseError {
    unreachable!("2026-08-12T02:30:00 exists")
}
```

### Dialects

| | fields | Sunday is | both day fields restricted | extras |
|---|---|---|---|---|
| `Vixie` | 5 | `0`, and `7` too | union | nicknames, `@reboot` |
| `Quartz` | 6 or 7 | `1` | rejected; one must be `?` | `L`, `LW`, `L-n`, `nW`, `n#m` |
| `Robfig` | 6 | `0` | union | nicknames, `@every <duration>` |
| `Cronexpr` | 5 | `0`, and `7` too | union | trailing IANA timezone, `H`, Quartz's predicates |

The same digit means different days in the first two, so a crontab written with digits
is not portable between them and one written with names is.

#### The union rule reads the items, not the set

Where both day fields are restricted, Vixie fires on **either** — unless a day field
carries the *wildcard*, in which case the two are combined with "and" instead. That is not
a question about the set of days a field denotes, so no accessor over the stored days
could answer it: `*,10` and `10,*` are one set written two ways and behave differently. It
is [the documented cron bug](https://crontab.guru/cron-bug.html).

`Schedule::matches` applies the rule; there is no accessor to apply it with, because the
witness is not a property of the days a field denotes and no question about them could
carry it. Which items count is `WildcardWitness`, and it sits inside `DomDowRule::Union`
because a dialect that refuses two restricted day fields never asks. The dialects
disagree in both directions, so it is a dialect's declaration rather than a syntactic
constant:

| | `*,10` | `10,*` | `*/2` | `?` |
|---|---|---|---|---|
| `Vixie`, `Cronexpr` — `LeadingStar`, the field's first item | wildcard | — | wildcard | n/a |
| `Robfig` — `AnyUnconstrained`, any item that narrows nothing | wildcard | wildcard | — | wildcard |

#### `H`, and where the seed comes from

`Cronexpr` reads `H` as a value chosen by hashing a caller-supplied seed into the field's
own values, so `H 0 * * *` fires at the same caller-specific minute every hour and
different callers spread across the range. The seed arrives at runtime and so cannot be a
dialect constant: `Schedule::parse_with(input, seed)` is the entry point that carries one,
and plain `parse` reports that it is missing.

The fold is over the values a field *has*, not over the ways they can be written, and
day-of-week is where those two counts differ: `0` and `7` are both Sunday, so eight digits
name seven days. `cronexpr` itself folds over the eight, which gives Sunday two of its
buckets and twice its share of the work; this crate folds over the seven, so every day
gets one. It is the one place a seed picks a different value here than there.

#### Timezones

An expression in a dialect that declares `Dialect::TIMEZONE` may end with an IANA name.
`ZonedSchedule` is the type that parses one — a sibling of `Schedule` rather than a
lifetime bolted onto it, because a schedule *with* a timezone denotes different instants
from one without. What the crate can do with the name afterwards is a question about
features, not about dialects; see the table below.

### The year range is on the type

`Schedule<D>` is `Schedule<D, 1>`, whose years are one `u128` over a 1970 epoch:
`1970..=2097`. That is short of both reference implementations on purpose — the range is
something the type states rather than something this crate quietly decided — and a year
beyond it is rejected by name:

```text
year field at 12..16: 2098 is legal cron but this schedule represents only up to 2097;
instantiate it with N = 2
```

## Benchmarks

Parse only — text in, a schedule out, dropped inside the timed region. No matching, no
next-occurrence computation. Criterion, `aarch64-apple-darwin` (M4 Pro), stable 1.97.1,
against `saffron` 0.1, `cron` 0.17, `croner` 3 and `cronexpr` 1.6.

A library appears in a row only where it **accepts** that shape. `saffron` is five-field
only, and `cron` does not take a bare five-field expression, so neither is in every row —
timing a parse against a rejection would compare a built schedule with an error return.
`cronexpr` wants five fields plus a timezone and has no seconds field, no year field and
no nickname macros, so it is absent from the 6-field, 7-field and nickname rows.

`cronexpr` requires the timezone by default; its cells here use `parse_crontab_with` in
`FallbackTimezoneOption::UTC` mode, called with no timezone field in the input — the same
five fields every other column parses. With nothing after the day-of-week field, cronexpr
never scans or resolves a timezone at all; it falls straight through to the constant
`jiff::tz::TimeZone::UTC`. That keeps its cells comparable to three parsers that do no
timezone work of any kind, but it is not how cronexpr is meant to be called — its whole
reason for existing is the timezone written into the expression, and this table does not
charge it for that. `cargo bench` also reports `cronexpr/timezone cost (informational)`,
outside this table: the same expression once with no timezone and once with an explicit
`Asia/Shanghai` through cronexpr's default, required-timezone mode. Resolving the name
costs about 38 ns more on this machine, a further ~5% on top of the 732 ns already below.

Bold is the fastest cell in the row, not cronp's column.

| | cronp | saffron | cron | croner | cronexpr† |
|---|---:|---:|---:|---:|---:|
| 5-field `30 2 * * 1-5` | **37.6 ns** | 38.4 ns | — | 8.61 µs | 719 ns |
| 5-field lists, steps, names | **100.8 ns** | 146.5 ns | — | 9.27 µs | 754 ns |
| 6-field `0 30 2 * * 1-5` | **44.9 ns** | — | 569 ns | 8.77 µs | — |
| 7-field Quartz with year | **96.0 ns** | — | 802 ns | 1.48 µs | — |
| nickname `@daily` | **9.7 ns** | — | 99.4 ns | 8.58 µs | — |
| rejected `0 0 * * 99` | **36.2 ns** | 37.8 ns | 523 ns | 1.01 µs | 757 ns |

† No timezone resolved; see above.

## Features

Two groups, and they are kept apart on purpose. **Propagation** says what tier the build
is, and every optional dependency learns it: `alloc` and `std` reach `jiff` through weak
dependency features, so `--features std,jiff` is jiff in its `std` mode rather than jiff
quietly staying `no_std` inside a `std` build. Neither pulls the dependency in on its own.
**Selection** says which capability is compiled — with `jiff` the odd one out, because it
names the dependency the other two are built on and delivers no capability by itself.

| feature | effect |
|---|---|
| *(default)* | `no_std`, no `alloc`: parse, represent, match. A timezone in the expression is retained as a borrowed `&str` and resolved by nobody. |
| `alloc`, `std` | the tier of the build, propagated into every optional dependency. No owned diagnostics reach them yet. |
| `jiff` | pulls the `jiff` dependency and nothing else: no API of this crate appears or changes, and no `civil::DateTime` conversion exists in any build. It is the base the two rows below add a capability to, and on its own it selects none. Still `no_std`. |
| `tz-static` | resolve a timezone against a table the **application** names at compile time, through `ZonedSchedule::resolve_in`. Still `no_std`, still no `alloc`: jiff's `static` feature requires neither. |
| `tz` | resolve **any** IANA name at runtime, through `ZonedSchedule::resolve`. Needs `std` and an allocator, and pulls jiff's bundled/system tzdb. |

Every row above that says `no_std` is built for a bare-metal target in CI, in a cell of the
`no-std` job named for it, and `tests/no_std.rs` fails if a new feature arrives without
either a cell or a statement that its tier needs a host. Being exercised on a host says
nothing about a row: the tier tests in `tests/public_api.rs` run in a graph where the
`cronexpr` dev-dependency enables `jiff/default`, which is the std and alloc these rows
disclaim.

`tz-static` and `tz` are different capabilities rather than two sizes of one. The static
tier resolves exactly what was compiled in and refuses everything else; the runtime tier
needs no registration at all. An application that knows its timezones can have the first
on bare metal.

One boundary on that last sentence, because it is a build-level requirement rather than
something this crate can satisfy for you. Every tier that pulls the `jiff` dependency
reaches `portable-atomic` through it, and `portable-atomic` needs atomic compare-and-swap:
that is `jiff`, `tz-static` and `tz` alike, not `tz-static` alone. Targets that have it —
`thumbv7em-none-eabi` and the rest of Cortex-M3 and up — build as they stand. On a target
without it, such as `thumbv6m-none-eabi` (Cortex-M0), `portable-atomic` requires the
**binary** to choose either its `critical-section` feature or `unsafe-assume-single-core`;
that is a leaf-crate decision by design, and a library must not make it on your behalf.
With the choice made, `tz-static` builds there too — the `no-std` job's Cortex-M0 cell
passes `--cfg portable_atomic_unsafe_assume_single_core`, the second of those two choices,
and goes red on `portable-atomic`'s own `compile_error!` without it. The default tier
reaches none of this and builds on both targets untouched.

### What is not here

Next-occurrence, iteration, and any timezone-aware matching. This crate parses,
represents, and answers whether a schedule fires at a civil instant;
`ZonedSchedule::matches` and "when does it fire next" are separate features and are not
in it.

`Schedule::matches` is deliberately the *only* way to ask. The per-field `admits_*`
predicates that remain — seconds, minutes, hours, months, years — are the ones that
combine with nothing but "and", so a caller can read them without a rule. The two day
fields have no such accessor: combining them takes the dialect's rule, that rule keys on
how each field was *written*, and a field carrying `L` or `15W` has an empty bitset
anyway. Exporting the terms of that decision and leaving the caller to assemble them is
how four wrong answers shipped, and it is what
`tests/matcher_differential.rs` — the matcher against `cronexpr`, `cron`, `croner` and
`saffron` over a corpus of expressions and a year of instants — now stands in the way
of.

#### License

`cronp` is under the terms of both the MIT license and the
Apache License (Version 2.0).

See [LICENSE-APACHE](LICENSE-APACHE), [LICENSE-MIT](LICENSE-MIT) for details.

Copyright (c) 2026 Al Liu.

[Github-url]: https://github.com/al8n/cronp/
[CI-url]: https://github.com/al8n/cronp/actions/workflows/ci.yml
[doc-url]: https://docs.rs/cronp
[crates-url]: https://crates.io/crates/cronp
[codecov-url]: https://app.codecov.io/gh/al8n/cronp/
[discord]: https://discord.gg/ysTvDvcusA
