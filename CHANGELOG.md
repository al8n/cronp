# UNRELEASED

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
  `cronexpr` 1.6, `cron` 0.17, `croner` 3 and `saffron` 0.1 over a corpus of expressions
  and a year of instants — the gate this class of defect never had, since the crate's
  other differential can only compare parses.

