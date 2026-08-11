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
