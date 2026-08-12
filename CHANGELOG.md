# 0.3.0 (August 12th, 2026)

- **Performance.** Parsing the plainest expressions is back to its pre-`0.3.0` cost. Giving
  every value sink a fallible `insert` had made the failure path exist even for the
  `Mask`-backed sink, whose every value is by construction a bit it already has; the
  unreachable path kept an error slot live across the loop that records a run of values and
  stopped that loop inlining. The cost was **per call rather than per unit of work**, so it
  fell on short expressions and not on dense ones: `30 2 * * 1-5` had become 1.42x slower and
  `0 30 2 * * 1-5` 1.30x, while lists, steps, names, seven-field Quartz, nicknames and the
  rejection path were unaffected. A sink now declares its own `Failure`, the `Mask`-backed one
  declares `Infallible`, and the single genuinely fallible path is outlined cold. Measured
  against the same baseline, the two rows are now 1.06x and 1.02x, which is inside the spread
  of the unchanged third-party parsers measured beside them.
- **`ZonedSchedule` names a timezone beside the schedule, and resolving it is the caller's
  step.** `parse` and `parse_with` accept an expression with a trailing zone name;
  `timezone_name`, `schedule`, `name` and `into_parts` read it back without resolving
  anything. `resolve` and `resolve_in` produce a `jiff` timezone, and `validate_in` checks a
  name against a list the caller supplies and returns `UnknownTimeZone` rather than a
  resolution error, so a build that carries no timezone database can still reject a name it
  does not know.
- `parse_with` takes a seed. The hashing a parse does is seedable, so a caller exposed to
  untrusted expressions can choose its own seed rather than inherit a fixed one.
- `canonical_bounds` reports a field's own inclusive bounds, which is what a caller needs to
  say why a value was refused without reimplementing the dialect's table.

- **Fixed.** A field whose union is every value no longer counts as a restriction, whatever
  is written beside its wildcard. Whether a field narrows anything was computed from the
  *syntax* — "exactly one item, and it was bare" — for a property that is *semantic*, so
  `*,2025` was a restriction and `*`, which names the same years, was not. In the year field
  that difference was fatal rather than cosmetic: the parser wrote out `1970..=2099` to back
  a restriction the expression never placed, and then refused the whole expression because
  2098 does not fit `Years<1>`. `0 0 0 ? * * *,2025`, `0 0 0 ? * * 2025,*` and
  `0 0 0 ? * * *,*` are legal Quartz that place no year restriction, and every one of them
  failed to parse under the default `Schedule<Quartz>` for a storage reason. They parse now,
  and they store nothing.
- No expression changes which instants it matches. A field that used to be "restricted" with
  its whole domain written out is now unrestricted with nothing written down, and the two
  admit the same values by construction; the year is the one field whose sink could be
  narrower than its dialect, which is why it was the one field that could observe the
  difference. `Calendar::years()` is empty for `2025,*` as it already was for `*`, and two
  spellings of one schedule — `0 0 *,10 * MON` and `0 0 * * MON` — now compare equal.
  `WildcardWitness` is untouched: `10,*` and `*,10` still land on opposite sides of Vixie's
  union rule, which is the one question the restriction flag was never able to answer.
- **Fixed.** The same repair, second half: a *written* year the instantiation cannot hold
  no longer refuses an unconstrained union either. `0 0 0 ? * * *,2098` and
  `0 0 0 ? * * 2098,*` parse under the default `Schedule<Quartz>`, and admit 2098 — 2098 is
  legal Quartz, the union containing `*` is every year, and a field that stores nothing has
  no storage to be too narrow. The first round refused these on the ground that "every item
  is checked on its own, exactly as `*,2100` is", which sorted two failures by what they
  look like rather than by where they come from.
- **Validity and representability are now separated, and the boundary is the sink.** A
  failure the *grammar* raises is a fault in the expression — outside the dialect's bounds,
  a backwards range, bytes that are not a token — and is refused per item, wildcard or no
  wildcard: `*,2100` and `*,2030-2020` are as illegal as they ever were, at every `N`. A
  failure a `ValueSink` returns is a fault in nothing: the value is legal cron and this
  instantiation is too narrow, so it is held until the field is classified and discarded if
  the field turns out to constrain nothing. A sink's `insert` reports a failure of its own
  declared kind rather than a bare `ErrorKind`, so the two cannot be confused at a call site,
  and one function is the sink's only caller. That kind is per sink: a sink whose storage can
  refuse names what it refuses, and one that cannot names `Infallible` — see the performance
  entry above, which is why the distinction is in the type rather than in a comment.
- A field that *is* a restriction reports exactly what it reported before, with the same
  value and the same span: `0 0 0 ? * * 2098` and `0 0 0 ? * * 1970-2099` still name
  `YearNotRepresentable` and the `N` that would hold them. What did change is precedence
  within one field — a fault in the expression now outranks a storage limit wherever the
  two meet, so `2098,2100` reports the year Quartz does not have rather than advising a
  wider `N` for an expression that is wrong anyway. Both orders of that pair now agree.
- Every tier documented as needing no host is now built for one that has none. The `no-std`
  job carries a cell per tier — `core-only`, `alloc`, `jiff`, `alloc+jiff` and `tz-static`,
  plus `tz-static` on Cortex-M0 with the `portable-atomic` choice a binary has to make — and
  each is `cargo check --lib`, so no dev-dependency is in the graph. `tz-static` shipped
  documented as bare-metal and no-alloc with no such cell at all: the only no_std command
  built `--no-default-features`, and the tier's own tests are host integration tests whose
  graph enables `jiff/default` through `cronexpr`, supplying the exact std and alloc the
  tier promises to avoid. A regression that made `tz-static` require `std` left every test
  green.
- `tests/no_std.rs` fails when a feature is added to `Cargo.toml` without either a cell in
  that job or an explicit statement that its tier needs a host, and when anything under
  `src/` reaches `alloc` outside the `alloc` feature — the half of the no-alloc claim a
  bare-metal build cannot make, since `alloc` is in those targets' sysroot and an rlib links
  no allocator.
- **Fixed.** `ZonedSchedule::parse` reports a fault in the cron expression before one in the
  trailing timezone. Where the run count was exactly one more than the dialect takes, the
  last field was validated as a timezone *before* the expression in front of it was parsed,
  so `ZonedSchedule::<Cronexpr>::parse("99 0 * * * @")` answered `MalformedTimezone` while
  the minute was out of range. That is the last thing wrong rather than the first, it
  contradicts the documented "the first thing wrong with the expression, exactly as
  `Schedule::parse` reports it", and a caller branching on `MalformedTimezone` could not tell
  it from an expression whose only fault was the timezone. The prefix is parsed first now and
  `MalformedTimezone` is raised only once everything before it has parsed; an expression
  whose only fault is the trailing field is unaffected.
- **The whole precedence family is written down and checked.** Which of several coexisting
  failures a caller hears about is a decision taken in thirty places across `src/`, and until
  now it was stated in two prose sentences that four public entry points relied on and that
  neither mentioned the exceptions. `schedule/tests/precedence.rs` is the census: every site,
  the rule it follows, why that rule is right there, and — where two failures really can hold
  at once — an expression carrying both together with the same pair minus the reported one,
  so that a coexistence claim is measured rather than asserted. Sites where two failures
  *cannot* coexist are rows with reasons rather than omissions, and the row count is pinned
  so that one going missing fails the suite.
- **Docs.** `ZonedSchedule::parse`'s `# Errors` says where `MalformedTimezone` sits in that
  order, and that `TimezoneNotSupported` is reported whatever the text says because no edit
  to the text would help.
- **Docs.** The Cortex-M0 boundary belongs to the `jiff` dependency, so it applies to
  `jiff`, `tz-static` and `tz` alike; the README attributed it to `tz-static` alone.

- **Breaking.** `DomDowRule::Union` carries a `WildcardWitness`. Which items of a day
  field count as the wildcard that turns the union rule's "or" into "and" is a dialect
  decision, and the dialects disagree in both directions: `Vixie` and `Cronexpr` count the
  field's first item and count `*/2`, `Robfig` counts any item that narrows nothing
  wherever it sits and does not count `*/2`. The witness lives inside the variant so that
  a dialect whose rule is `Exclusive` has none to state.
- **Breaking.** `Calendar::day_of_month_starts_with_star` and
  `Calendar::day_of_week_starts_with_star` are removed. They reported the field's first
  byte, which is only one of the two dialect answers, and they were inputs to a rule the
  caller had to apply. The rule is applied inside the crate now.
- `Robfig` applies the union rule as `github.com/robfig/cron` does. `0 0 0 ? * MON`,
  `0 0 0 10,* * MON` and `0 0 0 */2 * MON` each changed answer, the first two towards
  intersection and the third towards union.
- `@weekly`, `@monthly`, `@yearly`, `@daily`, `@hourly` and `@midnight` fire on the days
  their upstreams fire on. A nickname's open day field now carries the wildcard, as
  vixie's `entry.c` and robfig's `all()` both do, so `@weekly` intersects and fires on
  Sundays where it used to union and fire on every day of the week. `@weekly` and
  `0 0 * * 0` are now the same calendar.
- **Added.** `Schedule::matches(CivilDateTime) -> bool` and `Calendar::matches`, which
  answer whether the schedule fires at a civil instant: every field, both date
  predicates, and the dialect's day-of-month against day-of-week rule, decided inside the
  crate. `@every` and `@reboot` match no instant, which is what those two variants mean
  rather than a limitation. No time zone, no next-occurrence, no new dependency, and it
  needs no `alloc`.
- **Breaking.** `Calendar::admits_day_of_month` and `Calendar::admits_weekday` are
  removed. They are the two terms of the day decision, and a caller cannot combine them
  correctly: the rule keys on how each field was written, and a field carrying `L` or
  `15W` has an empty bitset, so `0 0 L * *` answered `false` for every day of every
  month. `matches` is the answer. The other per-field predicates — `admits_second`,
  `admits_minute`, `admits_hour`, `admits_month`, `admits_year` — stay, because they
  combine with nothing but "and".
- **Breaking.** `Calendar::day_of_month_restricted`, `Calendar::day_of_week_restricted`
  and `Calendar::year_restricted` are removed. The first two documented themselves as
  "the question the Vixie union rule keys off", which they never answered — `*,10` and
  `10,*` share their value and take opposite branches of that rule. `years()` is empty
  exactly when the year field placed no restriction, so the third said nothing new.
- **Breaking.** `Calendar::dom_dow_rule` is removed. It returned `D::DOM_DOW`, which a
  caller can name directly, and the rule it described is applied by `matches`.
- **Added.** `tests/matcher_differential.rs` drives `Schedule::matches` against
  `cronexpr` 1.6.0, `cron` 0.17.0, `croner` 3.0.1 and `saffron` 0.1.0 over a corpus of
  expressions and a year of instants — the gate this class of defect never had, since the
  crate's other differential can only compare parses. Those four dev-dependencies are
  pinned to exact versions: the differential's exclusions describe how a particular
  release of each behaves, `Cargo.lock` is `.gitignore`d, and a caret range would let a
  clean checkout resolve a different upstream and keep the exclusions anyway. Every
  exclusion asserts the upstream behaviour it is premised on and fails when upstream stops
  reproducing it, so an exclusion cannot outlive its reason.

