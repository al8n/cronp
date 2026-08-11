//! Which of several coexisting failures a caller hears about — every site, decided.
//!
//! An expression can be wrong in more than one way at once, and every place that then has
//! to choose is a place the choice can be made badly. One of them was: the timezone path
//! validated the trailing field before parsing the expression in front of it, so
//! `ZonedSchedule::<Cronexpr>::parse("99 0 * * * @")` answered `MalformedTimezone` while
//! the minute was out of range — and a caller branching on that variant could not tell it
//! from an expression whose *only* fault was the timezone.
//!
//! That is a family, not an incident. This file is the whole of it: every site in `src/`
//! that decides between coexisting failures, each with the rule it follows, why that rule
//! is the right one there, and — where two failures really can hold at once — an
//! expression that carries both and the answer it is owed.
//!
//! # The rule
//!
//! Three whole-expression answers bracket the field reads:
//!
//!   - [`ErrorKind::TimezoneNotSupported`] is about the *type*, not the text, and is
//!     reported before the text is looked at at all.
//!   - [`ErrorKind::WrongFieldCount`] is a precondition: without the right number of runs
//!     there is no field to attribute anything to.
//!   - The day-of-month against day-of-week rule is a postcondition: it reads facts the
//!     two day fields produce, so it cannot be asked before they have parsed.
//!
//! Between those, **the leftmost failure by byte offset wins**, with two departures:
//!
//!   - A construct the dialect or the field does not have outranks a *value* fault in the
//!     same item, even when the value is written to its left. `99W` in the seconds field
//!     reports the misplaced predicate, not the range: reporting the range invites `59W`,
//!     which is still wrong.
//!   - A **storage** limit is not a fault in the expression at all, and loses to every one
//!     that is, wherever it sits. It is deferred to the end of the field and discarded
//!     outright when the union constrains nothing.
//!
//! # Where each answer points
//!
//! *Which* failure is only half of an answer. The other half is **where it points**, and
//! that half went unranged for a whole round: a row could pin `ModifierMustBeAlone` and
//! say nothing about the bytes underneath it. `0 0 0 ? * 6#3,2` reported the violation
//! over `6` — two bytes short of the three-byte predicate that caused it — and
//! `0 0 0 ? * 2,6#3` reported it over `15..15`, an empty range past the last byte of the
//! input. Both passed every assertion this file had.
//!
//! So every probe now pins its span, and two rules hold it there. They are different
//! questions and the second exists because the first is not enough.
//!
//! **The shape**, [`points`], total over [`ErrorKind`]. Three answers and no fourth:
//!
//!   - [`Points::Text`] — the failure is about bytes the caller wrote, so the span is
//!     those bytes and is **never empty**.
//!   - [`Points::Everything`] — the failure is about the expression as a whole, so the
//!     span is exactly `0..input.len()` and is empty only when the expression is.
//!   - [`Points::Nothing`] — the failure is that something is *absent*, so the span is a
//!     position and is **always empty**. There are no bytes at the end of the input to
//!     name, and naming one would be a lie.
//!
//! **Is an empty span ever a legitimate answer? Yes, and only for the third kind.** An
//! empty span is a position, not a pointer; it is the honest answer for "the expression
//! ended before this could finish" and never for a construct the caller wrote down. That
//! is why this lives here as an assertion over every row rather than as a row of its own —
//! a case would pin the two inputs someone thought of, and the rule holds of all of them.
//!
//! **The text**, [`misdescribes`], partial and exact where it applies. A shape rule sorts
//! spans into three classes and **cannot see a span of the right class over the wrong
//! bytes** — which is what every defect in this campaign was. `ModifierMustBeAlone` over
//! the `6` of `6#3` is a non-empty slice of the input. So is
//! `YearNotRepresentable { year: 2098 }` over the bytes `1970`. Both passed the shape rule
//! while pointing somewhere a caller cannot act on. So where a failure's payload names
//! something the text contains, the text is held to it: a count of fields against the runs
//! it spans, a value against the digits it spans, and a fault in an *item* against the
//! item's own boundaries. The last is the general form of both defects and it retro-catches
//! the first.
//!
//! Where no relation exists — a bad byte, an unknown name, a dialect refusal — the rows
//! below are what pin the bytes, and that residue is named rather than papered over. A
//! total relation would be a second parser deciding independently which bytes each failure
//! is about, which is an oracle and not an assertion.
//!
//! [`every_answer_spans_the_shape_its_kind_allows_and_text_its_kind_agrees_with`] runs both
//! over the differential corpus **and** over every atom in every field position, in six
//! entry points. The second half is not decoration: the corpus is the scanner's and is
//! mostly one-field text, which every dialect refuses on the field count before a field is
//! ever read — with the corpus alone, reverting either fix left that sweep green.
//!
//! # Where the contract is written down
//!
//! Twice, in prose: [`Schedule::parse`]'s `# Errors` states it, and
//! [`ZonedSchedule::parse`]'s restates it with the timezone carved out. Two more entry
//! points — both `parse_with` — inherit it by saying "As [`Self::parse`]", so one sentence
//! is answering for four calls.
//!
//! Both statements are about *which* failure, and both only promise that a span exists.
//! The span half is written down in a third place and a different shape: [`Span::end`]'s
//! own documentation, which neither `# Errors` paragraph points at. It said an empty span
//! meant the expression ended too early and that this was "the one case" — a claim that
//! was wrong about three more kinds, and wrong in the direction that made the defect above
//! read as legal. It now names all four, and this file is what holds it to them.
//!
//! Neither statement mentions the bracketing answers or the departures, and a reader who
//! takes "the first thing wrong" literally gets three of the sites below backwards — the
//! day rule, the deferred storage limit, and the misplaced predicate — and decides nothing
//! at five more, where the two candidate answers carry the same span. That gap is what
//! this file closes: a site whose behaviour drifts fails here by name, and a site nobody
//! wrote down is a row that is missing rather than a paragraph that was never written.
//!
//! # What a row costs, and what it buys
//!
//! [`Evidence::Cannot`] is a row too. A site where two failures cannot both hold is a
//! member of this list with a reason — the reason is the claim, and it is the thing that
//! stops being true when someone adds a third failure to that site. Deleting such a row
//! because "there is nothing to test there" is how the next round starts.

#![allow(
  clippy::indexing_slicing,
  clippy::unwrap_used,
  clippy::expect_used,
  clippy::panic
)]

use super::{
  a_predicate_in_a_list_is_reported_over_the_whole_predicate,
  lexical_contract::{
    a_bad_byte_does_not_outrank_a_wrong_field_count,
    the_field_count_preflight_runs_before_any_field_is_read,
  },
};
use core::mem::{Discriminant, discriminant};
use std::collections::HashSet;

use crate::{
  date::{CivilDateTime, DateComponent},
  dialect::{Cronexpr, Quartz, Robfig, Vixie},
  error::{ErrorKind, FieldKind, ParseError, Span},
  schedule::{
    Schedule, ZonedSchedule, count_fields,
    reference::{
      tests::{ATOMS, BASES},
      token::tests::differential::corpus,
    },
  },
};

// ---------------------------------------------------------------------------
// The shape of a row.
// ---------------------------------------------------------------------------

/// Which entry point a probe goes through, and at what instantiation.
///
/// Every probe is at `N = 1`. Storage width is a question about one site rather than an
/// axis over all of them, and the width property that site turns on — that widening `N`
/// changes the answer where a real fault would not — is not a choice between two failures
/// and so is held by [`a_storage_limit_is_not_a_fault_in_the_expression`] instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Entry {
  Vixie,
  Quartz1,
  Robfig,
  Cronexpr,
  /// `ZonedSchedule::<Vixie>` — a dialect that takes no timezone, which is the mistake.
  ZonedVixie,
  ZonedCronexpr,
}

impl Entry {
  /// The call, spelled as a caller would write it.
  const fn name(self) -> &'static str {
    match self {
      Self::Vixie => "Schedule::<Vixie, 1>::parse",
      Self::Quartz1 => "Schedule::<Quartz, 1>::parse",
      Self::Robfig => "Schedule::<Robfig, 1>::parse",
      Self::Cronexpr => "Schedule::<Cronexpr, 1>::parse",
      Self::ZonedVixie => "ZonedSchedule::<Vixie>::parse",
      Self::ZonedCronexpr => "ZonedSchedule::<Cronexpr>::parse",
    }
  }

  /// Parses `input`, keeping only whether it was accepted and, if not, why.
  ///
  /// The schedule is dropped: seven different types could not otherwise be compared, and
  /// what is under test here is which failure comes back.
  fn parse(self, input: &str) -> Result<(), ParseError> {
    match self {
      Self::Vixie => Schedule::<Vixie, 1>::parse(input).map(|_| ()),
      Self::Quartz1 => Schedule::<Quartz, 1>::parse(input).map(|_| ()),
      Self::Robfig => Schedule::<Robfig, 1>::parse(input).map(|_| ()),
      Self::Cronexpr => Schedule::<Cronexpr, 1>::parse(input).map(|_| ()),
      Self::ZonedVixie => ZonedSchedule::<Vixie>::parse(input).map(|_| ()),
      Self::ZonedCronexpr => ZonedSchedule::<Cronexpr>::parse(input).map(|_| ()),
    }
  }
}

/// What a caller is owed: the kind, the byte span, and the field if the parser knew one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Answer {
  kind: ErrorKind,
  start: usize,
  end: usize,
  field: Option<FieldKind>,
}

const fn at(kind: ErrorKind, start: usize, end: usize, field: FieldKind) -> Answer {
  Answer {
    kind,
    start,
    end,
    field: Some(field),
  }
}

/// An answer raised before the parser knew which field it was in.
const fn whole(kind: ErrorKind, start: usize, end: usize) -> Answer {
  Answer {
    kind,
    start,
    end,
    field: None,
  }
}

/// What an answer's span is allowed to be, decided by the kind of failure.
///
/// The second attribute of every answer. See the [module documentation](self) for why it
/// is here and not a row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Points {
  /// At bytes the caller wrote. The span is a **non-empty** slice of the input.
  Text,
  /// At the expression as a whole. The span is exactly `0..input.len()`, so it is empty
  /// only when the expression itself is.
  Everything,
  /// At the position where something the grammar required is absent. The span is
  /// **always empty**, because there is nothing there to name — it is a caret rather
  /// than a highlight.
  Nothing,
}

/// Which of the three a failure of this kind is.
///
/// Written as one total match rather than an allowlist, so a new [`ErrorKind`] cannot be
/// added without someone deciding where it points. That is the difference between a rule
/// the census carries and a rule it happens to satisfy: the variant that arrives next year
/// stops the build here until it is classified.
const fn points(kind: ErrorKind) -> Points {
  match kind {
    // Absence: the answer is a caret at the point the missing thing should have been.
    // Three of the four are at the end of the input — `Cursor::end_span`,
    // `input.len()..input.len()`, and the empty slice a `@every` with no duration
    // occupies. `DurationMissingUnit` is the one that is not, and it is the one this
    // classification found: `@every 1` carets at offset 8 and `@every 1.2.3` carets at
    // the second `.`, with bytes on both sides. Its unit run is empty by the test that
    // raises it, so the span is empty wherever it sits.
    ErrorKind::UnexpectedEnd
    | ErrorKind::EmptyExpression
    | ErrorKind::EmptyDuration
    | ErrorKind::DurationMissingUnit => Points::Nothing,

    // The whole expression. Two are the census's bracketing answers — the count
    // precondition and the day-field postcondition — and the third is about the type
    // rather than the text, which is why it covers all of it.
    ErrorKind::WrongFieldCount { .. }
    | ErrorKind::QuestionMarkRequired { .. }
    | ErrorKind::QuestionMarkInBothDayFields { .. }
    | ErrorKind::TimezoneNotSupported { .. } => Points::Everything,

    // Everything else is about text that is there: a value, a name, a construct, a
    // predicate, a nickname, a duration component, a trailing field. The span is that
    // text, and it has at least one byte in it.
    ErrorKind::UnexpectedCharacter
    | ErrorKind::NumberTooLarge
    | ErrorKind::UnexpectedToken
    | ErrorKind::ValueOutOfRange { .. }
    | ErrorKind::UnknownName
    | ErrorKind::ReversedRange { .. }
    | ErrorKind::ZeroStep
    | ErrorKind::OpenEndedStepNotSupported { .. }
    | ErrorKind::QuestionMarkNotSupported { .. }
    | ErrorKind::QuestionMarkNotValidHere
    | ErrorKind::QuestionMarkMustBeAlone { .. }
    | ErrorKind::ModifierNotSupported { .. }
    | ErrorKind::ModifierNotValidHere
    | ErrorKind::ModifierMustBeAlone
    | ErrorKind::TrailingInput
    | ErrorKind::UnknownMacro
    | ErrorKind::MacroNotSupported { .. }
    | ErrorKind::RebootNotSupported { .. }
    | ErrorKind::EveryNotSupported { .. }
    | ErrorKind::MalformedDuration
    | ErrorKind::UnknownDurationUnit
    | ErrorKind::DurationOverflow
    | ErrorKind::ZeroDuration
    | ErrorKind::HashedValueNotSupported { .. }
    | ErrorKind::HashedValueNeedsSeed
    | ErrorKind::MalformedTimezone
    | ErrorKind::YearBelowEpoch { .. }
    | ErrorKind::YearNotRepresentable { .. } => Points::Text,
  }
}

/// Holds one answer against the rule for its kind, and returns what went wrong if
/// anything did.
///
/// A `&'static str` rather than an assertion so that both callers can use it: the census
/// names the row it came from, and the corpus sweep names the expression and the dialect.
fn misplaced(kind: ErrorKind, span: Span, input: &str) -> Option<&'static str> {
  let slice = input.get(span.start()..span.end());
  match points(kind) {
    _ if slice.is_none() => Some("its span is not a slice of the input"),
    Points::Text if span.is_empty() => {
      Some("it is about text the caller wrote, and points at no bytes at all")
    }
    Points::Nothing if !span.is_empty() => {
      Some("it is about something absent, and points at bytes as though it were there")
    }
    Points::Everything if (span.start(), span.end()) != (0, input.len()) => {
      Some("it is about the whole expression, and does not cover it")
    }
    _ => None,
  }
}

/// Whether the span's *text* is consistent with what the failure says about it.
///
/// The second axis, and a different question from [`misplaced`]. That one asks which of
/// three shapes the span has; **this one asks whether the bytes are the right bytes**, and
/// nothing but this can see a span that is the right shape and the wrong text. Both spans
/// the two rounds of this campaign got wrong were non-empty slices of the input, so the
/// shape check passed them: `ModifierMustBeAlone` over the `6` of `6#3`, and
/// `YearNotRepresentable { year: 2098 }` over the bytes `1970`.
///
/// It is **partial**, and deliberately so — a total relation would need a second parser
/// that decides independently which bytes each failure is about, which is an oracle and
/// not an assertion. What is here is every relation that is *exact*:
///
///   - a failure that names a **count** of fields must span text with that many fields;
///   - a failure that names a **value** must, where the span is written in digits, span
///     those digits;
///   - a failure about an **item** must span a whole item.
///
/// The third is the general form of both defects. A predicate that has to stand alone and
/// a value the storage could not hold are both faults in an *item*: the first because the
/// item is what the caller must delete, the second because the value was generated by the
/// item and written nowhere, so no narrower text names it. "Begins and ends at an item
/// boundary" is checkable from the input alone, which is what keeps it independent of the
/// spans the parser produced.
///
/// Kinds with no relation here carry no payload naming their text — a bad byte, an unknown
/// name, a dialect refusal — and for those the census rows are what pin the bytes.
fn misdescribes(kind: ErrorKind, span: Span, input: &str) -> Option<&'static str> {
  let Some(text) = input.get(span.start()..span.end()) else {
    return Some("its span is not a slice of the input");
  };
  let digits = |s: &str| {
    (!s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
      .then(|| s.parse::<u32>().ok())
      .flatten()
  };

  match kind {
    // Exact: the span is the whole expression, so the runs in it are the runs counted.
    ErrorKind::WrongFieldCount { found, .. } if count_fields(text) != found => {
      Some("it counts fields the text it spans does not have")
    }

    // Exact wherever the value was written in digits. A value spelled as a name — `JAN`,
    // `MON` — is skipped rather than guessed at: mapping it back needs the name tables,
    // and a relation that reimplements the parser is not evidence about the parser.
    ErrorKind::ValueOutOfRange { value, .. } if digits(text).is_some_and(|d| d != value) => {
      Some("it names a value the digits it spans do not spell")
    }
    ErrorKind::ZeroStep if digits(text).is_some_and(|d| d != 0) => {
      Some("it reports a zero step over digits that are not zero")
    }
    ErrorKind::ReversedRange { start, end }
      if text.split_once('-').is_some_and(|(a, b)| {
        matches!((digits(a), digits(b)), (Some(x), Some(y))
            if x != start || y != end)
      }) =>
    {
      Some("it names endpoints the range it spans does not have")
    }

    // A fault in an item is reported over the whole item. Checked against the input's own
    // separators, so it holds whatever span the parser chose.
    ErrorKind::ModifierMustBeAlone
    | ErrorKind::QuestionMarkMustBeAlone { .. }
    | ErrorKind::YearNotRepresentable { .. }
    | ErrorKind::YearBelowEpoch { .. }
      if !is_whole_item(span, input) =>
    {
      Some("it is a fault in one item and spans less than the item")
    }

    _ => None,
  }
}

/// Whether the span stops in the *middle* of an item at either end.
///
/// Phrased as the negative on purpose. "Ends at a comma, whitespace or the end of the
/// input" is the tempting rule and it is too strong: `0 0 0 1,L% 1 ?` reports the
/// predicate over `L` and the byte after it is `%`, which is not a separator — it is a
/// lexical fault the field then ends on, and a separate answer. The span is right and that
/// rule calls it wrong.
///
/// So the question is whether the neighbouring byte **could have been part of the same
/// item**. Only alphanumerics and the three infix bytes `-`, `/` and `#` continue one, so
/// those are what a whole item may not be cut off by. That catches every span this
/// campaign got wrong — `1970` before the `-` of `1970-2099`, `*` before the `/` of `*/2`,
/// `6` before the `#` of `6#3`, `15` and `L` before the `W` of `15W` and `LW` — and admits
/// a span that a byte no item can contain happens to sit next to.
fn is_whole_item(span: Span, input: &str) -> bool {
  let continues = |byte: u8| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'/' | b'#');
  let bytes = input.as_bytes();
  let cut = |at: usize| bytes.get(at).copied().is_some_and(continues);
  !(span.start() > 0 && cut(span.start().wrapping_sub(1))) && !cut(span.end())
}

/// One expression, where it is parsed, and the failure it must come back with.
struct Probe {
  entry: Entry,
  input: &'static str,
  answer: Answer,
}

/// Two failures that hold of one expression at once, and which one is reported.
///
/// `other` is what makes the coexistence a measurement rather than a claim. It is the same
/// pair of failures with the reported one taken away — a different dialect where the
/// construct exists, or the same expression with the offending text removed — and it has
/// to come back with the failure the first probe did *not* report. If it came back `Ok`,
/// the first expression only ever had one fault and the row would be pinning nothing.
struct Pair {
  heard: Probe,
  other: Probe,
}

/// How a site's decision is held up.
enum Evidence {
  /// Two failures can hold at once. Each pair carries both and says which is reported.
  Ordered(&'static [Pair]),
  /// Two failures cannot hold at once here, and this is why.
  ///
  /// The reason is the row's whole content, and it is a claim about the code that can stop
  /// being true — usually by someone adding a third failure to the same site.
  Cannot(&'static str),
  /// Two failures can hold at once, but what this site reports is not a [`ParseError`], so
  /// the probes cannot reach it. [`Site::also`] carries the test that does.
  Elsewhere(&'static str),
}

/// One place in `src/` that decides between coexisting failures.
struct Site {
  /// Where the decision is taken.
  at: &'static str,
  /// The failures it stands between.
  between: &'static str,
  /// Which one wins, and why that is the right one here.
  decided: &'static str,
  /// How that is held up.
  evidence: Evidence,
  /// Tests that hold what the pairs cannot express.
  ///
  /// The functions themselves rather than their names, so a citation cannot outlive the
  /// test it cites: renaming or deleting one stops this file compiling. They are held for
  /// the compiler and not called from here — each is a `#[test]` the harness already runs,
  /// and two of them are Miri-ignored for being tens of thousands of parses, which calling
  /// them from a census would smuggle straight back in.
  also: &'static [fn()],
}

// ---------------------------------------------------------------------------
// The list.
// ---------------------------------------------------------------------------

const SITES: &[Site] = &[
  // ----- schedule/mod.rs: the whole expression -----
  Site {
    at: "schedule/mod.rs — `parse_seeded`, the emptiness check before everything",
    between: "`EmptyExpression` and every failure a field could report",
    decided: "Nothing else can be wrong. `EmptyExpression` fires exactly when there is no \
              non-whitespace byte after the leading space, so there is no content left for \
              a second failure to be about.",
    evidence: Evidence::Cannot(
      "an input with no non-whitespace byte has no field, no item and no token to be wrong",
    ),
    also: &[],
  },
  Site {
    at: "schedule/mod.rs — `parse_seeded`, the `@` dispatch",
    between: "the nickname grammar and the field grammar",
    decided: "A dispatch on the first byte, not a choice between failures. A lone `@` is \
              not a nickname, and `take_macro` deliberately leaves the cursor on it so it \
              reaches `parse_calendar` as an ordinary bad byte and is reported in whichever \
              field it sits in.",
    evidence: Evidence::Cannot(
      "one byte takes one branch; the nickname path and the field path never both run",
    ),
    also: &[],
  },
  Site {
    at: "schedule/mod.rs — `parse_macro`, `@every`",
    between: "`EveryNotSupported` and whatever the duration after it says",
    decided: "The dialect refusal, which is also the leftmost: the nickname is at offset 0 \
              and the duration follows it.",
    evidence: Evidence::Ordered(&[Pair {
      heard: Probe {
        entry: Entry::Vixie,
        input: "@every zzz",
        answer: whole(ErrorKind::EveryNotSupported { dialect: "Vixie" }, 0, 6),
      },
      other: Probe {
        entry: Entry::Robfig,
        input: "@every zzz",
        answer: whole(ErrorKind::MalformedDuration, 7, 10),
      },
    }]),
    also: &[],
  },
  Site {
    at: "schedule/mod.rs — `parse_macro`, what follows `@every`",
    between: "`EmptyDuration` and `UnexpectedToken`",
    decided: "Cannot both hold: the next byte is absent, or whitespace, or neither, and \
              the three partition it. One kind used to answer for the first and third at \
              once, which is how `@every1s` reported that `@every` needs a duration after \
              it while pointing at the `1` of the duration that was there. `EmptyDuration` \
              is a statement about text that is *absent* — it is one of the four failures \
              whose span is a caret — so the branch where text is present cannot borrow \
              it, and reports the token that may not be there instead.",
    evidence: Evidence::Cannot(
      "the byte after the nickname is absent, whitespace, or neither, and one byte cannot \
       be two of those",
    ),
    also: &[],
  },
  Site {
    at: "schedule/mod.rs — `parse_macro`, `@reboot`",
    between: "`RebootNotSupported` and the `TrailingInput` after a complete nickname",
    decided: "The dialect refusal, which is also the leftmost.",
    evidence: Evidence::Ordered(&[Pair {
      heard: Probe {
        entry: Entry::Quartz1,
        input: "@reboot *",
        answer: whole(ErrorKind::RebootNotSupported { dialect: "Quartz" }, 0, 7),
      },
      other: Probe {
        entry: Entry::Vixie,
        input: "@reboot *",
        answer: whole(ErrorKind::TrailingInput, 8, 9),
      },
    }]),
    also: &[],
  },
  Site {
    at: "schedule/mod.rs — `parse_macro`, the nickname table",
    between: "`UnknownMacro` and `MacroNotSupported`",
    decided: "`UnknownMacro`, and the two carry the same span so nothing about byte order \
              settles it. It is the same reasoning the timezone finding turned on: \
              `MacroNotSupported` names a dialect and invites the caller to change it, and \
              no dialect defines `@nonsense`, so that would be the wrong remediation. The \
              dialect refusal is kept for a name some dialect does define.",
    evidence: Evidence::Ordered(&[Pair {
      heard: Probe {
        entry: Entry::Quartz1,
        input: "@nonsense",
        answer: whole(ErrorKind::UnknownMacro, 0, 9),
      },
      other: Probe {
        entry: Entry::Quartz1,
        input: "@daily",
        answer: whole(ErrorKind::MacroNotSupported { dialect: "Quartz" }, 0, 6),
      },
    }]),
    also: &[],
  },
  Site {
    at: "schedule/mod.rs — `parse_calendar`, the field-count preflight",
    between: "`WrongFieldCount` and every failure a field could report",
    decided: "The count, always. It is a precondition rather than a competitor: with the \
              wrong number of runs there is no field position to attribute a byte to, and a \
              guess would name a field the caller did not write. Its span is the whole \
              expression and therefore starts at 0, so it is the leftmost answer as well as \
              the necessary one.",
    evidence: Evidence::Ordered(&[Pair {
      heard: Probe {
        entry: Entry::Quartz1,
        input: "0 0 * * *%",
        answer: whole(
          ErrorKind::WrongFieldCount {
            found: 5,
            min: 6,
            max: 7,
            dialect: "Quartz",
          },
          0,
          10,
        ),
      },
      other: Probe {
        entry: Entry::Vixie,
        input: "0 0 * * *%",
        answer: at(ErrorKind::UnexpectedCharacter, 9, 10, FieldKind::DayOfWeek),
      },
    }]),
    also: &[
      the_field_count_preflight_runs_before_any_field_is_read,
      a_bad_byte_does_not_outrank_a_wrong_field_count,
    ],
  },
  Site {
    at: "schedule/mod.rs — `parse_calendar`, one field after another",
    between: "a failure in one field and a failure in a later one",
    decided: "The leftmost field. This is the plain reading of the contract, and the whole \
              of what the generated lexical matrix asserts.",
    evidence: Evidence::Ordered(&[Pair {
      heard: Probe {
        entry: Entry::Vixie,
        input: "99 99 1 1 0",
        answer: at(
          ErrorKind::ValueOutOfRange {
            value: 99,
            min: 0,
            max: 59,
          },
          0,
          2,
          FieldKind::Minute,
        ),
      },
      other: Probe {
        entry: Entry::Vixie,
        input: "0 99 1 1 0",
        answer: at(
          ErrorKind::ValueOutOfRange {
            value: 99,
            min: 0,
            max: 23,
          },
          2,
          4,
          FieldKind::Hour,
        ),
      },
    }]),
    also: &[],
  },
  Site {
    at: "schedule/mod.rs — `parse_calendar`, the two `TrailingInput` guards around the year",
    between: "`TrailingInput` and a failure in the year field",
    decided: "Neither can happen. `count_fields` has already fixed how many runs there are, \
              so nothing but whitespace can follow the last field, and a dialect whose \
              count admits a seventh run always has a year `FieldSpec` to read it with. \
              Both branches are written out rather than asserted so the parser has no way \
              to panic; the comment at the first of them says exactly that.",
    evidence: Evidence::Cannot(
      "the count preflight makes both branches unreachable: an extra run would have failed \
       the count, and a seventh run implies `YearField::Optional`",
    ),
    also: &[],
  },
  Site {
    at: "schedule/mod.rs — `check_dom_dow`, after every field has parsed",
    between: "`QuestionMarkRequired` / `QuestionMarkInBothDayFields` and a failure in any \
              field",
    decided: "The field. This is the departure a reader of \"the first thing wrong\" would \
              get backwards: the rule's span is the whole expression and starts at 0, and it \
              still loses. It is a postcondition, not a competitor — it reads the \
              `question_mark` outcome of both day fields, which does not exist until they \
              have parsed. The count preflight is its mirror image, and for the same \
              structural reason.",
    evidence: Evidence::Ordered(&[Pair {
      heard: Probe {
        entry: Entry::Quartz1,
        input: "0 0 0 1 99 1",
        answer: at(
          ErrorKind::ValueOutOfRange {
            value: 99,
            min: 1,
            max: 12,
          },
          8,
          10,
          FieldKind::Month,
        ),
      },
      other: Probe {
        entry: Entry::Quartz1,
        input: "0 0 0 1 1 1",
        answer: whole(ErrorKind::QuestionMarkRequired { dialect: "Quartz" }, 0, 11),
      },
    }]),
    also: &[],
  },
  // ----- field/mod.rs: inside one field -----
  Site {
    at: "field/mod.rs — `parse_field`, one item of a list after another",
    between: "a failure in one item and a failure in a later one",
    decided: "The leftmost item.",
    evidence: Evidence::Ordered(&[Pair {
      heard: Probe {
        entry: Entry::Vixie,
        input: "99,88 0 1 1 0",
        answer: at(
          ErrorKind::ValueOutOfRange {
            value: 99,
            min: 0,
            max: 59,
          },
          0,
          2,
          FieldKind::Minute,
        ),
      },
      other: Probe {
        entry: Entry::Vixie,
        input: "0,88 0 1 1 0",
        answer: at(
          ErrorKind::ValueOutOfRange {
            value: 88,
            min: 0,
            max: 59,
          },
          2,
          4,
          FieldKind::Minute,
        ),
      },
    }]),
    also: &[],
  },
  Site {
    at: "field/mod.rs — `parse_field`, `SoleItem::violation` at the comma",
    between: "an item that has to be the whole field and a failure in another item beside it",
    decided: "Whichever is leftmost, and the check runs at the comma rather than at the end \
              of the field so that both orders come out that way: an offending first item is \
              caught before the second is read, and an offending second item after the first \
              has been. What it points at is the offending item's *own* text, which is now a \
              property of the type rather than of the call site: `SoleItem` holds the claim \
              and the span in one slot, so an item cannot claim the field without saying \
              where it was written and there is no fallback left to be wrong.",
    evidence: Evidence::Ordered(&[
      Pair {
        heard: Probe {
          entry: Entry::Quartz1,
          input: "0 0 0 ? * 6#3,99",
          answer: at(ErrorKind::ModifierMustBeAlone, 10, 13, FieldKind::DayOfWeek),
        },
        other: Probe {
          entry: Entry::Quartz1,
          input: "0 0 0 ? * 6,99",
          answer: at(
            ErrorKind::ValueOutOfRange {
              value: 99,
              min: 1,
              max: 7,
            },
            12,
            14,
            FieldKind::DayOfWeek,
          ),
        },
      },
      Pair {
        heard: Probe {
          entry: Entry::Quartz1,
          input: "0 0 0 ? * 99,6#3",
          answer: at(
            ErrorKind::ValueOutOfRange {
              value: 99,
              min: 1,
              max: 7,
            },
            10,
            12,
            FieldKind::DayOfWeek,
          ),
        },
        other: Probe {
          entry: Entry::Quartz1,
          input: "0 0 0 ? * 6,6#3",
          answer: at(ErrorKind::ModifierMustBeAlone, 12, 15, FieldKind::DayOfWeek),
        },
      },
      Pair {
        heard: Probe {
          entry: Entry::Quartz1,
          input: "0 0 0 L,99 1 ?",
          answer: at(ErrorKind::ModifierMustBeAlone, 6, 7, FieldKind::DayOfMonth),
        },
        other: Probe {
          entry: Entry::Quartz1,
          input: "0 0 0 1,99 1 ?",
          answer: at(
            ErrorKind::ValueOutOfRange {
              value: 99,
              min: 1,
              max: 31,
            },
            8,
            10,
            FieldKind::DayOfMonth,
          ),
        },
      },
      Pair {
        heard: Probe {
          entry: Entry::Quartz1,
          input: "0 0 0 99,L 1 ?",
          answer: at(
            ErrorKind::ValueOutOfRange {
              value: 99,
              min: 1,
              max: 31,
            },
            6,
            8,
            FieldKind::DayOfMonth,
          ),
        },
        other: Probe {
          entry: Entry::Quartz1,
          input: "0 0 0 1,L 1 ?",
          answer: at(ErrorKind::ModifierMustBeAlone, 8, 9, FieldKind::DayOfMonth),
        },
      },
    ]),
    also: &[a_predicate_in_a_list_is_reported_over_the_whole_predicate],
  },
  Site {
    at: "field/mod.rs — `SoleItem::violation`, between its own two kinds",
    between: "`ModifierMustBeAlone` and `QuestionMarkMustBeAlone`",
    decided: "Cannot both hold, so the order the function tests them in is never live. The \
              check runs at every comma, so the field is refused the moment either flag is \
              set and no later item is ever read to set the other: `?,L` answers with the \
              question mark and `L,?` with the modifier, and in each the second item is \
              never parsed.",
    evidence: Evidence::Cannot(
      "a field is refused at the first comma after either flag is set, so a state with both \
       set is unreachable",
    ),
    also: &[],
  },
  Site {
    at: "field/mod.rs — `parse_field`, `trailing_error` after the sole-item check",
    between: "the lexeme a field ends on and an item that had to be the whole field",
    decided: "Whichever is leftmost. The sole-item check runs first and reports the \
              offending item's own span, which is to the left of whatever the field then \
              ended on.",
    evidence: Evidence::Ordered(&[Pair {
      heard: Probe {
        entry: Entry::Quartz1,
        input: "0 0 0 1,L% 1 ?",
        answer: at(ErrorKind::ModifierMustBeAlone, 8, 9, FieldKind::DayOfMonth),
      },
      other: Probe {
        entry: Entry::Quartz1,
        input: "0 0 0 1,1% 1 ?",
        answer: at(ErrorKind::UnexpectedCharacter, 9, 10, FieldKind::DayOfMonth),
      },
    }]),
    also: &[],
  },
  Site {
    at: "field/mod.rs — `parse_field`, the deferred `Unrepresentable`",
    between: "a storage limit and any fault in the expression",
    decided: "The fault in the expression, wherever it sits — the second departure from \
              leftmost-by-span, and the one 12ae660 installed. A storage limit is not a \
              fault in the text: the value is legal in the dialect and it is this \
              instantiation that is too narrow. `ValueSink::insert` returns an \
              `Unrepresentable` rather than an `ErrorKind` so the two cannot be confused at \
              a call site, `record` holds the first one in `ItemState::deferred`, and \
              `parse_field` answers it only once the field is classified — discarded when \
              the union constrains nothing, raised when the stored set is what answers for \
              the field. So `2098,%` reports the bad byte to its *right*, and `*,2098` \
              reports nothing at all.",
    evidence: Evidence::Ordered(&[
      Pair {
        heard: Probe {
          entry: Entry::Quartz1,
          input: "0 0 0 ? * * 2098,%",
          answer: at(ErrorKind::UnexpectedCharacter, 17, 18, FieldKind::Year),
        },
        other: Probe {
          entry: Entry::Quartz1,
          input: "0 0 0 ? * * 2098",
          answer: at(
            ErrorKind::YearNotRepresentable {
              year: 2098,
              max_representable: 2097,
              required_n: 2,
            },
            12,
            16,
            FieldKind::Year,
          ),
        },
      },
      Pair {
        heard: Probe {
          entry: Entry::Quartz1,
          input: "0 0 0 ? * * 2098,2100",
          answer: at(
            ErrorKind::ValueOutOfRange {
              value: 2100,
              min: 1970,
              max: 2099,
            },
            17,
            21,
            FieldKind::Year,
          ),
        },
        other: Probe {
          entry: Entry::Quartz1,
          input: "0 0 0 ? * * 2098",
          answer: at(
            ErrorKind::YearNotRepresentable {
              year: 2098,
              max_representable: 2097,
              required_n: 2,
            },
            12,
            16,
            FieldKind::Year,
          ),
        },
      },
      Pair {
        heard: Probe {
          entry: Entry::Quartz1,
          input: "0 0 0 ? * * 2100,2098",
          answer: at(
            ErrorKind::ValueOutOfRange {
              value: 2100,
              min: 1970,
              max: 2099,
            },
            12,
            16,
            FieldKind::Year,
          ),
        },
        other: Probe {
          entry: Entry::Quartz1,
          input: "0 0 0 ? * * 2098",
          answer: at(
            ErrorKind::YearNotRepresentable {
              year: 2098,
              max_representable: 2097,
              required_n: 2,
            },
            12,
            16,
            FieldKind::Year,
          ),
        },
      },
    ]),
    also: &[a_storage_limit_is_not_a_fault_in_the_expression],
  },
  Site {
    at: "field/mod.rs — `record`, `Option::get_or_insert_with`",
    between: "one storage failure and a later one in the same field",
    decided: "The first, so that a restricted field reports the same value and the same \
              span it always did. The slot's own documentation says so.",
    evidence: Evidence::Ordered(&[Pair {
      heard: Probe {
        entry: Entry::Quartz1,
        input: "0 0 0 ? * * 2098,2099",
        answer: at(
          ErrorKind::YearNotRepresentable {
            year: 2098,
            max_representable: 2097,
            required_n: 2,
          },
          12,
          16,
          FieldKind::Year,
        ),
      },
      other: Probe {
        entry: Entry::Quartz1,
        input: "0 0 0 ? * * 2099",
        answer: at(
          ErrorKind::YearNotRepresentable {
            year: 2099,
            max_representable: 2097,
            required_n: 2,
          },
          12,
          16,
          FieldKind::Year,
        ),
      },
    }]),
    also: &[],
  },
  Site {
    at: "field/mod.rs — `parse_item`, the `?` arm",
    between: "`QuestionMarkNotSupported` and `QuestionMarkNotValidHere`",
    decided: "The dialect refusal. The two carry the same span, so byte order settles \
              nothing; telling a Vixie user to move their `?` into a day field would be \
              wrong advice, because Vixie has no `?` in any field.",
    evidence: Evidence::Ordered(&[Pair {
      heard: Probe {
        entry: Entry::Vixie,
        input: "? 0 1 1 0",
        answer: at(
          ErrorKind::QuestionMarkNotSupported { dialect: "Vixie" },
          0,
          1,
          FieldKind::Minute,
        ),
      },
      other: Probe {
        entry: Entry::Quartz1,
        input: "? 0 0 ? 1 1",
        answer: at(ErrorKind::QuestionMarkNotValidHere, 0, 1, FieldKind::Second),
      },
    }]),
    also: &[],
  },
  Site {
    at: "field/mod.rs — `parse_hashed_item`",
    between: "`HashedValueNotSupported` and `HashedValueNeedsSeed`",
    decided: "The dialect refusal, same span and the same reasoning: `parse_with` would not \
              help a dialect that never had `H`. The function's own documentation says the \
              two are different questions; this is which of them is asked first.",
    evidence: Evidence::Ordered(&[Pair {
      heard: Probe {
        entry: Entry::Vixie,
        input: "H 0 1 1 0",
        answer: at(
          ErrorKind::HashedValueNotSupported { dialect: "Vixie" },
          0,
          1,
          FieldKind::Minute,
        ),
      },
      other: Probe {
        entry: Entry::Cronexpr,
        input: "H 0 1 1 0",
        answer: at(ErrorKind::HashedValueNeedsSeed, 0, 1, FieldKind::Minute),
      },
    }]),
    also: &[],
  },
  Site {
    at: "field/mod.rs — `parse_value_modifier` and `parse_last_item`, the field check \
         before `value_of`",
    between: "`ModifierNotValidHere` and the value's own bound",
    decided: "The misplaced predicate — the third departure, and the only one where the \
              reported span lies strictly to the *right* of the unreported one. `99W` in the \
              seconds field is out of range and in the wrong field at once, and reporting \
              the range invites the caller to write `59W`, which is still wrong. Where the \
              predicate does belong, the bound is reported as usual.",
    evidence: Evidence::Ordered(&[Pair {
      heard: Probe {
        entry: Entry::Quartz1,
        input: "99W 0 0 ? 1 1",
        answer: at(ErrorKind::ModifierNotValidHere, 2, 3, FieldKind::Second),
      },
      other: Probe {
        entry: Entry::Quartz1,
        input: "0 0 0 99W 1 ?",
        answer: at(
          ErrorKind::ValueOutOfRange {
            value: 99,
            min: 1,
            max: 31,
          },
          6,
          8,
          FieldKind::DayOfMonth,
        ),
      },
    }]),
    also: &[],
  },
  Site {
    at: "field/mod.rs — `parse_item`'s `#` arm, and `trailing_error`",
    between: "`ModifierNotSupported` and `UnexpectedToken` for the same byte",
    decided: "One byte and two namings rather than two failures, and the dialect picks \
              between them. A dialect that *has* the predicates must not blame the dialect \
              for a `#` written where no predicate can go.",
    evidence: Evidence::Ordered(&[Pair {
      heard: Probe {
        entry: Entry::Robfig,
        input: "# 0 0 1 1 0",
        answer: at(
          ErrorKind::ModifierNotSupported { dialect: "Robfig" },
          0,
          1,
          FieldKind::Second,
        ),
      },
      other: Probe {
        entry: Entry::Quartz1,
        input: "# 0 0 ? 1 1",
        answer: at(ErrorKind::UnexpectedToken, 0, 1, FieldKind::Second),
      },
    }]),
    also: &[],
  },
  Site {
    at: "field/mod.rs — `parse_value_item`, the range before the step",
    between: "`ReversedRange` and a failure in the step written after it",
    decided: "The range, which is the leftmost.",
    evidence: Evidence::Ordered(&[Pair {
      heard: Probe {
        entry: Entry::Vixie,
        input: "5-1/0 0 1 1 0",
        answer: at(
          ErrorKind::ReversedRange { start: 5, end: 1 },
          0,
          3,
          FieldKind::Minute,
        ),
      },
      other: Probe {
        entry: Entry::Vixie,
        input: "5-6/0 0 1 1 0",
        answer: at(ErrorKind::ZeroStep, 4, 5, FieldKind::Minute),
      },
    }]),
    also: &[],
  },
  Site {
    at: "field/mod.rs — `parse_value_item`, the slash before the step's value",
    between: "`OpenEndedStepNotSupported` and the step's own value",
    decided: "The slash, which is the leftmost.",
    evidence: Evidence::Ordered(&[Pair {
      heard: Probe {
        entry: Entry::Vixie,
        input: "5/0 0 1 1 0",
        answer: at(
          ErrorKind::OpenEndedStepNotSupported { dialect: "Vixie" },
          1,
          2,
          FieldKind::Minute,
        ),
      },
      other: Probe {
        entry: Entry::Cronexpr,
        input: "5/0 0 1 1 0",
        answer: at(ErrorKind::ZeroStep, 2, 3, FieldKind::Minute),
      },
    }]),
    also: &[],
  },
  // ----- years/mod.rs -----
  Site {
    at: "years/mod.rs — `Years::insert`",
    between: "`YearBelowEpoch` and `YearNotRepresentable`",
    decided: "Cannot both hold. `checked_sub(EPOCH)` decides, and the ceiling is consulted \
              only once the floor has been cleared. `YearBelowEpoch` is unreachable while \
              every dialect's floor is at or above `EPOCH`, and both are `Unrepresentable` \
              in any case, so a dialect that declared an earlier floor would get the same \
              deferral without that being a second decision.",
    evidence: Evidence::Cannot(
      "one subtraction partitions the input: a year is below the epoch or it is not, and \
       only the second branch can then overrun the words",
    ),
    also: &[],
  },
  // ----- every/mod.rs -----
  Site {
    at: "every/mod.rs — `parse`, one duration component after another",
    between: "a failure in one component and a failure in a later one",
    decided: "The leftmost component. Every component's failure is raised where it is \
              found, an overflow included, so the scan never runs past one.",
    evidence: Evidence::Ordered(&[Pair {
      heard: Probe {
        entry: Entry::Robfig,
        input: "@every 1x2y",
        answer: whole(ErrorKind::UnknownDurationUnit, 8, 9),
      },
      other: Probe {
        entry: Entry::Robfig,
        input: "@every 1s2y",
        answer: whole(ErrorKind::UnknownDurationUnit, 10, 11),
      },
    }]),
    also: &[],
  },
  Site {
    at: "every/mod.rs — the three per-component failures",
    between: "`MalformedDuration`, `DurationMissingUnit` and `UnknownDurationUnit`",
    decided: "Cannot hold at once. At one component they partition it: no digits and no \
              fraction at all, digits with no unit run after them, and a unit run that names \
              no unit this scanner knows.",
    evidence: Evidence::Cannot(
      "the three test disjoint conditions of the same component, in the order the bytes are \
       consumed",
    ),
    also: &[],
  },
  Site {
    at: "every/mod.rs — `ZeroDuration` and the final `DurationOverflow`",
    between: "the two whole-text postconditions and any component failure",
    decided: "Cannot hold at once. Both are reached only after every component parsed and \
              was added to the total, so no component failure is outstanding when either is \
              asked.",
    evidence: Evidence::Cannot(
      "the loop returns on the first component failure, so the postconditions run only when \
       there is none",
    ),
    also: &[],
  },
  // ----- schedule/zoned/mod.rs -----
  Site {
    at: "schedule/zoned/mod.rs — `parse_seeded`, `TimezoneNotSupported` first",
    between: "`TimezoneNotSupported` and everything that could be wrong with the text",
    decided: "The type. This is the one refusal that is not about the expression at all, \
              which is why `parse`'s documentation says \"Otherwise\" before restating the \
              contract: no edit to the text would help, so reporting a fault in the text \
              would send the caller to fix the wrong thing. It outranks even \
              `EmptyExpression`.",
    evidence: Evidence::Ordered(&[Pair {
      heard: Probe {
        entry: Entry::ZonedVixie,
        input: "",
        answer: whole(ErrorKind::TimezoneNotSupported { dialect: "Vixie" }, 0, 0),
      },
      other: Probe {
        entry: Entry::ZonedCronexpr,
        input: "",
        answer: whole(ErrorKind::EmptyExpression, 0, 0),
      },
    }]),
    also: &[],
  },
  Site {
    at: "schedule/zoned/mod.rs — `parse_seeded`, the `MAX_FIELDS + 1` gate",
    between: "splitting a trailing timezone off and letting the schedule parser report the \
              count",
    decided: "The count, for every run total but exactly one more than the dialect takes. \
              Splitting a field off an expression that was already the wrong length would \
              mislabel whatever it found and hide the shape that is actually wrong.",
    evidence: Evidence::Ordered(&[Pair {
      heard: Probe {
        entry: Entry::ZonedCronexpr,
        input: "0 99 * * * * * Asia/Shanghai",
        answer: whole(
          ErrorKind::WrongFieldCount {
            found: 8,
            min: 5,
            max: 5,
            dialect: "Cronexpr",
          },
          0,
          28,
        ),
      },
      other: Probe {
        entry: Entry::ZonedCronexpr,
        input: "0 99 * * * Asia/Shanghai",
        answer: at(
          ErrorKind::ValueOutOfRange {
            value: 99,
            min: 0,
            max: 23,
          },
          2,
          4,
          FieldKind::Hour,
        ),
      },
    }]),
    also: &[],
  },
  Site {
    at: "schedule/zoned/mod.rs — `is_timezone_name`, the other half of the same gate",
    between: "`MalformedTimezone` over the last run and `WrongFieldCount` over the whole \
              expression",
    decided: "`MalformedTimezone`, over the run. The row above decides every *other* run \
              total; this decides the one the gate lets through. At exactly one run more \
              than the dialect takes, the last run is the timezone by position, so a run \
              that could be no zone name is a fault *there* — naming the four bytes that \
              cannot be a name says more than telling the caller the expression has six \
              fields, which is true of a correct zoned expression too. The same text \
              through the type that has no timezone is the count, and that is the answer \
              this one outranks. **This is the site the review found**: the check was a \
              character allowlist, digits were in it, and `0 0 * * * 2025` was accepted \
              with the timezone `Some(\"2025\")` — a year retained as a zone in a dialect \
              with no year field, which is exactly the stray cron field the split exists \
              to catch.",
    evidence: Evidence::Ordered(&[Pair {
      heard: Probe {
        entry: Entry::ZonedCronexpr,
        input: "0 0 * * * 2025",
        answer: whole(ErrorKind::MalformedTimezone, 10, 14),
      },
      other: Probe {
        entry: Entry::Cronexpr,
        input: "0 0 * * * 2025",
        answer: whole(
          ErrorKind::WrongFieldCount {
            found: 6,
            min: 5,
            max: 5,
            dialect: "Cronexpr",
          },
          0,
          14,
        ),
      },
    }]),
    also: &[],
  },
  Site {
    at: "schedule/zoned/mod.rs — `parse_seeded`, the prefix before `is_timezone_name`",
    between: "a fault in the cron half and `MalformedTimezone`",
    decided: "The cron half, which is the leftmost: the prefix occupies every byte before \
              the name and the name is the last field, so it is the last thing that can be \
              wrong. **This is the site the review found.** The order used to be the other \
              way round, so `99 0 * * * @` answered `MalformedTimezone` while the minute was \
              out of range, and a caller branching on that variant could not tell the case \
              from one whose only fault was the timezone.",
    evidence: Evidence::Ordered(&[Pair {
      heard: Probe {
        entry: Entry::ZonedCronexpr,
        input: "99 0 * * * @",
        answer: at(
          ErrorKind::ValueOutOfRange {
            value: 99,
            min: 0,
            max: 59,
          },
          0,
          2,
          FieldKind::Minute,
        ),
      },
      other: Probe {
        entry: Entry::ZonedCronexpr,
        input: "0 0 * * * @",
        answer: whole(ErrorKind::MalformedTimezone, 10, 11),
      },
    }]),
    also: &[],
  },
  Site {
    at: "schedule/zoned/mod.rs — `resolve_in` and `resolve`",
    between: "an unregistered name and anything else",
    decided: "Cannot both hold. A `ZonedSchedule` exists only because its expression \
              parsed, and resolution is one lookup of one name: it is in the application's \
              table, or in jiff's database, or it is not.",
    evidence: Evidence::Cannot(
      "resolution runs on an already-parsed value and asks one question of one name",
    ),
    also: &[],
  },
  // ----- date/mod.rs -----
  Site {
    at: "date/mod.rs — `CivilDateTime::new`",
    between: "the six components, any number of which can be out of range at once",
    decided: "The most significant, which is the leftmost of the rendered date: the check \
              order is the field order of `Display`. Two of the six are forced rather than \
              chosen — the day is checked against the length of *this* month in *this* year, \
              so the year and the month have to be settled before it.",
    evidence: Evidence::Elsewhere(
      "a `DateError` is not a `ParseError` and has no span, so the table's probes cannot \
       reach it",
    ),
    also: &[the_date_constructor_reports_its_most_significant_bad_component],
  },
];

// ---------------------------------------------------------------------------
// The guards.
// ---------------------------------------------------------------------------

/// How many sites the sweep found.
///
/// Pinned as a literal for the same reason the generated matrix pins its case count: the
/// failure this guards against is a row going *missing*. A census that quietly shrinks
/// still passes every assertion it has left, and a site with no row is exactly the shape
/// the timezone defect had.
const SITES_DECIDED: usize = 32;

/// How many of those have two failures that really can hold at once.
const SITES_ORDERED: usize = 22;

/// How many are members with a reason rather than with a case.
const SITES_IMPOSSIBLE: usize = 9;

/// How many distinct [`ErrorKind`]s the corpus sweep actually produces.
///
/// Not the number of variants — several are unreachable from any text, `YearBelowEpoch`
/// among them — but the number the *witness* reaches. The classification in [`points`] is
/// total over the type; this is how wide the evidence for it is, written down so that it
/// cannot narrow quietly.
const KINDS_WITNESSED: usize = 28;

#[test]
fn every_precedence_site_answers_with_the_failure_it_is_supposed_to() {
  let mut ordered = 0usize;
  let mut impossible = 0usize;
  let mut elsewhere = 0usize;

  for site in SITES {
    match &site.evidence {
      Evidence::Ordered(pairs) => {
        ordered += 1;
        assert!(
          !pairs.is_empty(),
          "{}: claims two failures can coexist and offers none",
          site.at
        );
        for pair in *pairs {
          check(site, &pair.heard, "the failure the caller hears");
          check(site, &pair.other, "the failure it outranked");
          assert_ne!(
            pair.heard.answer, pair.other.answer,
            "{}: both probes answer the same way, so the pair pins no ordering",
            site.at
          );
        }
      }
      Evidence::Cannot(reason) => {
        impossible += 1;
        assert!(
          reason.len() > 40,
          "{}: a non-member needs a reason, and {reason:?} is not one",
          site.at
        );
      }
      Evidence::Elsewhere(reason) => {
        elsewhere += 1;
        assert!(
          !site.also.is_empty(),
          "{}: says the probes cannot reach it ({reason}) and names no test that can",
          site.at
        );
      }
    }

    assert!(
      !site.at.is_empty() && !site.between.is_empty() && !site.decided.is_empty(),
      "a site with no location, no pair of failures, or no decision is not a row"
    );
  }

  assert_eq!(
    (SITES.len(), ordered, impossible),
    (SITES_DECIDED, SITES_ORDERED, SITES_IMPOSSIBLE),
    "the census changed size. A row added is a site newly found — say so and update the \
     counts. A row removed is either a site that no longer exists, or the beginning of the \
     next round."
  );
  assert_eq!(
    ordered + impossible + elsewhere,
    SITES.len(),
    "a site was counted twice"
  );
}

/// Runs one probe and holds it against the answer the row claims.
fn check(site: &Site, probe: &Probe, role: &str) {
  let error = match probe.entry.parse(probe.input) {
    Err(error) => error,
    Ok(()) => panic!(
      "{}\n  {role}: {}({:?}) was accepted, so this row pins no coexistence at all",
      site.at,
      probe.entry.name(),
      probe.input
    ),
  };

  assert_eq!(
    Answer {
      kind: *error.kind(),
      start: error.span().start(),
      end: error.span().end(),
      field: error.field(),
    },
    probe.answer,
    "{}\n  {role}\n  call:    {}({:?})\n  between: {}\n  decided: {}",
    site.at,
    probe.entry.name(),
    probe.input,
    site.between,
    site.decided
  );

  // The two span axes. The equality above pins the bytes this row expects; these pin that
  // the bytes are a shape the kind allows and text the kind's payload agrees with. The
  // second is the one a row cannot supply by itself — a row written from what the parser
  // did would agree with a wrong span as readily as with a right one.
  if let Some(fault) = misplaced(*error.kind(), error.span(), probe.input)
    .or_else(|| misdescribes(*error.kind(), error.span(), probe.input))
  {
    panic!(
      "{}\n  {role}\n  call:    {}({:?})\n  answer:  {:?} at {} ({:?})\n  {fault}",
      site.at,
      probe.entry.name(),
      probe.input,
      error.kind(),
      error.span(),
      probe.input.get(error.span().start()..error.span().end()),
    );
  }
}

/// The three things about a storage limit that a pair of failures cannot express.
///
/// A [`Pair`] says which of two failures is reported. What it cannot say is that a storage
/// limit is not a failure in the first place: that widening `N` changes the answer where a
/// real fault would not, that a wildcard beside it discards it entirely, and that both
/// orders of the same list agree. Those are the three that were wrong before 12ae660, and
/// they are the reason the deferral exists at all.
fn a_storage_limit_is_not_a_fault_in_the_expression() {
  // The width, and only the width, is what refuses this one.
  assert!(Schedule::<Quartz, 1>::parse("0 0 0 ? * * 2098").is_err());
  assert!(Schedule::<Quartz, 2>::parse("0 0 0 ? * * 2098").is_ok());

  // A union containing a wildcard is every year, and every year is what an empty set
  // means — so there is no stored value for a width to be too small for, in either order.
  for expression in ["0 0 0 ? * * *,2098", "0 0 0 ? * * 2098,*"] {
    let schedule = Schedule::<Quartz, 1>::parse(expression)
      .unwrap_or_else(|e| panic!("{expression} was refused for storage width: {e}"));
    let calendar = schedule.calendar().expect("an ordinary expression");
    assert!(
      calendar.admits_year(2098),
      "{expression} places no year restriction, so it admits 2098"
    );
    assert!(calendar.years().is_empty(), "{expression} stores no years");
  }

  // A fault in the expression is not excused the same way: `*,2100` is refused because
  // 2100 is not a year Quartz declares, at any `N`.
  for expression in ["0 0 0 ? * * *,2100", "0 0 0 ? * * 2100,*"] {
    assert_eq!(
      *Schedule::<Quartz, 1>::parse(expression)
        .expect_err(expression)
        .kind(),
      ErrorKind::ValueOutOfRange {
        value: 2100,
        min: 1970,
        max: 2099,
      },
      "{expression}: a wildcard excuses a storage limit and not an invalid year"
    );
  }

  // Where it points, which is the half a kind-only assertion cannot see. Every value a
  // run produces is *generated*: `1970-2099` contains 2098 in none of its bytes and `*/2`
  // contains it in none of its three, so there is no narrower text the failure could
  // honestly name and the answer is the construct that generated it. Reporting
  // `first_span` instead gave `YearNotRepresentable { year: 2098 }` over the bytes `1970`
  // — a non-empty slice of the input, so the shape rule passed it.
  //
  // The last two rows are the spellings that write the value down, and they are here so
  // that widening the span cannot be mistaken for widening it everywhere: a bare value
  // and a list item still report exactly their own digits.
  for (expression, span) in [
    ("0 0 0 ? * * 1970-2099", (12, 21)),
    ("0 0 0 ? * * 1970-2099/1", (12, 23)),
    ("0 0 0 ? * * */2", (12, 15)),
    ("0 0 0 ? * * 2098", (12, 16)),
    ("0 0 0 ? * * 2098,2099", (12, 16)),
  ] {
    let error = Schedule::<Quartz, 1>::parse(expression).expect_err(expression);
    assert_eq!(
      *error.kind(),
      ErrorKind::YearNotRepresentable {
        year: 2098,
        max_representable: 2097,
        required_n: 2,
      },
      "{expression}"
    );
    assert_eq!(
      (error.span().start(), error.span().end()),
      span,
      "{expression}: reported over {:?} rather than over the construct that generated 2098",
      expression.get(error.span().start()..error.span().end())
    );
  }

  // And the answer for a list carrying both does not depend on which side each is
  // written: a storage limit never wins, so the order of the pair cannot change it.
  for expression in ["0 0 0 ? * * 2098,2100", "0 0 0 ? * * 2100,2098"] {
    for found in [
      Schedule::<Quartz, 1>::parse(expression).expect_err(expression),
      Schedule::<Quartz, 2>::parse(expression).expect_err(expression),
    ] {
      assert_eq!(
        *found.kind(),
        ErrorKind::ValueOutOfRange {
          value: 2100,
          min: 1970,
          max: 2099,
        },
        "{expression}: the invalid year is the answer at every width and in either order"
      );
    }
  }
}

/// The date constructor reports the most significant component that is out of range.
///
/// Every component below the reported one is out of range in each case, so this is a
/// precedence and not six independent checks. The order is the one `Display` renders in,
/// and the first three of it are forced besides: the day is checked against the length of
/// the month in the given year.
fn the_date_constructor_reports_its_most_significant_bad_component() {
  const CASES: &[(DateComponent, u16, u8, u8, u8, u8, u8)] = &[
    (DateComponent::Year, 0, 13, 32, 25, 60, 60),
    (DateComponent::Month, 2024, 13, 32, 25, 60, 60),
    (DateComponent::Day, 2024, 2, 30, 25, 60, 60),
    (DateComponent::Hour, 2024, 2, 29, 25, 60, 60),
    (DateComponent::Minute, 2024, 2, 29, 23, 60, 60),
    (DateComponent::Second, 2024, 2, 29, 23, 59, 60),
  ];

  for &(component, year, month, day, hour, minute, second) in CASES {
    let error = CivilDateTime::new(year, month, day, hour, minute, second)
      .expect_err("every one of these carries at least one bad component");
    assert_eq!(
      error.component(),
      component,
      "{year}-{month}-{day}T{hour}:{minute}:{second}"
    );
  }

  // The day's range comes from the month, which is why the month cannot be checked after
  // it: February 29th exists in 2024 and not in 2023, and the message has to say which.
  let error = CivilDateTime::new(2023, 2, 29, 0, 0, 0).expect_err("2023 is not a leap year");
  assert_eq!(error.component(), DateComponent::Day);
  assert_eq!((error.min(), error.max()), (1, 28));
  assert!(CivilDateTime::new(2024, 2, 29, 0, 0, 0).is_ok());
}

#[test]
fn the_two_sites_the_probes_cannot_reach_are_held_anyway() {
  a_storage_limit_is_not_a_fault_in_the_expression();
  the_date_constructor_reports_its_most_significant_bad_component();
}

/// Every failure the parser produces spans a shape its kind allows, and text its kind
/// agrees with.
///
/// Named for the two things it checks, which is narrower than what an earlier name
/// promised. It ran as `every_answer_points_at_what_it_is_about` over 150,208 answers and
/// **could not see a span of the right shape over the wrong bytes** — which is exactly what
/// both defects in this campaign were. "Points at what it is about" is a relation between a
/// failure and the text it concerns; establishing that in general needs a second parser
/// that decides the text independently, and this is not one. What it does check is stated
/// in [`misplaced`] (the shape, total over [`ErrorKind`]) and [`misdescribes`] (the text,
/// partial and exact where it applies).
///
/// The rows above pin the span of about fifty answers. This pins the two rules over all of
/// them: the differential corpus, in every dialect and through both entry points. A row can
/// only catch a span someone wrote down, and both defects were spans nobody had.
///
/// Prefixes are the half that matters most and the reason the corpus is the right source.
/// Truncation is where a span runs off the end of the text it is supposed to index, and
/// the corpus carries every prefix of every case by construction — so `UnexpectedEnd`
/// arrives from hundreds of places rather than from the one input a hand-written row would
/// have used.
///
/// # Totality of the match is not totality of the witness
///
/// [`points`] is total over `ErrorKind`, and that is a claim about the *classification*,
/// not about the evidence. The evidence is whatever inputs the corpus reaches, and one of
/// the classifications was wrong for a spelling the corpus did not contain: `EmptyDuration`
/// was called a caret, and `@every1s` — no separator — answered it over the `1`. Every
/// corpus input had a space there.
///
/// Three things are done about that, because no one of them is enough:
///
///   - the corpus gained the shapes it was missing, the nickname-without-separator family
///     among them, so the differential covers them too rather than only this sweep;
///   - the count of distinct kinds this sweep witnesses is pinned below, so a kind
///     drifting out of reach is a failure and not a silent narrowing;
///   - and the [`Points::Nothing`] claim is universal — "always empty" cannot be
///     established by sampling at all — so it rests on each raise site building its span
///     from an expression that is empty by construction, and there are exactly four:
///     `Cursor::end_span()` for `UnexpectedEnd`, `input.len()..input.len()` for
///     `EmptyExpression`, `base..base + 0` for `EmptyDuration`, and a unit run of length
///     zero for `DurationMissingUnit`. This sweep corroborates that; it does not prove it.
#[test]
#[cfg_attr(
  miri,
  ignore = "the corpus in six instantiations is far too slow under an interpreter"
)]
fn every_answer_spans_the_shape_its_kind_allows_and_text_its_kind_agrees_with() {
  const ENTRIES: &[Entry] = &[
    Entry::Vixie,
    Entry::Quartz1,
    Entry::Robfig,
    Entry::Cronexpr,
    Entry::ZonedVixie,
    Entry::ZonedCronexpr,
  ];

  /// Holds every entry point's answer for one input against both rules.
  fn sweep(input: &str, answers: &mut usize, seen: &mut HashSet<Discriminant<ErrorKind>>) {
    for &entry in ENTRIES {
      let Err(error) = entry.parse(input) else {
        continue;
      };
      *answers += 1;
      seen.insert(discriminant(error.kind()));
      if let Some(fault) = misplaced(*error.kind(), error.span(), input)
        .or_else(|| misdescribes(*error.kind(), error.span(), input))
      {
        panic!(
          "{}({input:?})\n  answered {:?} at {} ({:?})\n  {fault}",
          entry.name(),
          error.kind(),
          error.span(),
          input.get(error.span().start()..error.span().end()),
        );
      }
    }
  }

  let corpus = corpus();
  let mut answers = 0usize;
  let mut seen = HashSet::new();
  for expression in &corpus {
    sweep(expression, &mut answers, &mut seen);
    // Every prefix, the same way the reference differential takes them: `char_indices`
    // yields every boundary except the input's length, and the whole expression above is
    // that one.
    for (boundary, _) in expression.char_indices() {
      sweep(&expression[..boundary], &mut answers, &mut seen);
    }
  }

  // The corpus above is the *scanner's*, and almost every string in it is one field long
  // — which is the wrong number of fields in every dialect, so the count preflight answers
  // before a field is ever read. Sweeping it alone reaches the whole-expression failures
  // and very little of the field grammar, which is where both of this campaign's defects
  // were: with only the loop above, reverting either fix left this test green.
  //
  // So the second half embeds each atom in each field position of a well-shaped
  // expression, sharing `ATOMS` and `BASES` with the reference differential rather than
  // copying them.
  for base in BASES {
    let fields: std::vec::Vec<&str> = base.split(' ').collect();
    for index in 0..fields.len() {
      for atom in ATOMS {
        let mut written = fields.clone();
        written[index] = atom;
        sweep(&written.join(" "), &mut answers, &mut seen);
      }
    }
  }

  // Which kinds the witness actually reached. Pinned as a literal for the same reason the
  // site count is: the failure this guards against is the evidence quietly shrinking while
  // every assertion left in the sweep still passes.
  assert_eq!(
    seen.len(),
    KINDS_WITNESSED,
    "the sweep reached {} of the {KINDS_WITNESSED} kinds it used to. A kind fewer is a \
     shape the corpus stopped producing; a kind more is a shape it gained, and both want \
     saying out loud.",
    seen.len()
  );

  // A sweep that answered nothing would satisfy every assertion in it, so the floor is
  // part of the test. Almost every corpus expression is refused by almost every dialect.
  assert!(
    corpus.len() > 5_000 && answers > 100_000,
    "the corpus shrank: {} expressions, {answers} answers classified",
    corpus.len()
  );
}
