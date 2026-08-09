//! The lexical surface of every cron dialect, recognised in place.
//!
//! Cron's lexical surface is six one-byte punctuation marks, two one-byte
//! case-insensitive letters, a digit run, a closed set of nineteen three-letter names, an
//! `@`-prefixed nickname and a whitespace run — all ASCII.
//!
//! There is no token type. A scanner that produced one spent most of its time not
//! recognising bytes but *building the value*: a `(Result<Token, LexError>,
//! Range<usize>)` is forty-odd bytes written and read back for every lexeme, and it cost
//! about twice what deciding which lexeme it was cost. Measured on this crate,
//! recognising the tokens of a five-field expression and throwing them away took 9 ns;
//! recognising the same tokens and materialising them took 37 ns, and it took that
//! whether the recogniser was a generated automaton or a hand-written match. So
//! [`Cursor`] hands the parser no lexemes at all. It answers the questions the grammar
//! actually asks — is the next byte a comma, read a digit run's value, read a word and
//! say which of the nineteen names it was — and the grammar consumes each answer where
//! it is produced.
//!
//! What survives as a value is [`Lexeme`], which is a lexeme's *kind* with none of its
//! payload. Only the error paths ask for it: naming what was found costs a scan, and a
//! parser that is about to fail can afford one.
//!
//! Recognition is still total and still dialect-blind: `?` is recognised even though
//! Vixie has no `?`, `L` and `W` and `#` even though only Quartz uses them, `@every` even
//! though only the Go dialect accepts it. Which of them a dialect allows is the parser's
//! decision, and keeping it out of here is what lets one scanner serve all of them.
//!
//! Everything here is bounds-checked and allocation-free, and the crate keeps
//! `#![forbid(unsafe_code)]`.

use core::ops::Range;

#[cfg(test)]
mod tests;

/// Whether a byte is one a whitespace run would match.
///
/// This class is the field separator, so it is also what
/// [`count_fields`](crate::schedule) splits on. One definition, because a scanner and a
/// field counter that disagreed about whitespace would disagree about how many fields an
/// expression has.
pub(crate) const fn is_space_byte(byte: u8) -> bool {
  matches!(byte, b' ' | b'\t' | b'\r' | b'\n' | b'\x0C')
}

/// How many of [`NAMES`] are months.
///
/// The table is the twelve months in order and then the seven weekdays from Sunday, and
/// that order is what lets a name be resolved from where it was found instead of being
/// compared as a string. `the_table_is_the_months_then_the_weekdays_in_order` pins it.
pub(crate) const MONTHS: u8 = 12;

/// The nineteen month and weekday names, case-folded by [`key`].
///
/// Spelled out as a closed set rather than matched as "three letters" so that `LW` reads
/// as `L` followed by `W` instead of being swallowed by a longer name match.
/// `lw_is_two_modifiers_not_a_name` pins that.
///
/// This is the only place the names live. [`begins_a_name`] derives the two-letter
/// prefixes from it rather than listing them again, because a second table would be a
/// second thing to keep in step.
pub(crate) const NAMES: [u32; 19] = [
  key(b'J', b'A', b'N'),
  key(b'F', b'E', b'B'),
  key(b'M', b'A', b'R'),
  key(b'A', b'P', b'R'),
  key(b'M', b'A', b'Y'),
  key(b'J', b'U', b'N'),
  key(b'J', b'U', b'L'),
  key(b'A', b'U', b'G'),
  key(b'S', b'E', b'P'),
  key(b'O', b'C', b'T'),
  key(b'N', b'O', b'V'),
  key(b'D', b'E', b'C'),
  key(b'S', b'U', b'N'),
  key(b'M', b'O', b'N'),
  key(b'T', b'U', b'E'),
  key(b'W', b'E', b'D'),
  key(b'T', b'H', b'U'),
  key(b'F', b'R', b'I'),
  key(b'S', b'A', b'T'),
];

/// Packs three ASCII letters into one integer, upper-cased.
///
/// Clearing bit five is a case fold for ASCII letters and nothing else, so every caller
/// establishes that all three bytes are ASCII alphabetic before getting here. Comparing
/// one integer is what makes a nineteen-way name match cost about as much as a byte
/// compare.
pub(crate) const fn key(first: u8, second: u8, third: u8) -> u32 {
  (upper(first) << 16) | (upper(second) << 8) | upper(third)
}

/// The two-letter form of [`key`].
pub(crate) const fn prefix_key(first: u8, second: u8) -> u32 {
  (upper(first) << 8) | upper(second)
}

/// One ASCII letter, upper-cased and widened.
///
/// The widening is a cast rather than `u32::from` because `From` is not a `const` trait
/// on this crate's MSRV. One byte into four bytes loses nothing either way.
const fn upper(letter: u8) -> u32 {
  const CASE: u8 = 0b1101_1111;
  (letter & CASE) as u32
}

/// Where a folded name sits in [`NAMES`], if it is one.
///
/// The *place* rather than a yes or no, because the place is the value: index `n` below
/// [`MONTHS`] is the month `n + 1`, and index `n` at or above it is the weekday `n -
/// MONTHS` counted from Sunday. Resolving a name is then an integer subtraction instead
/// of a walk over two string tables.
pub(crate) fn name_index(folded: u32) -> Option<u8> {
  NAMES
    .iter()
    .position(|&name| name == folded)
    .and_then(|index| u8::try_from(index).ok())
}

/// Whether two letters could still have become a name.
///
/// Only the error path asks. A byte that begins no token is reported over everything the
/// scan consumed before it failed, so `MAX` is one error spanning `MA` and then one over
/// `X`, while `SX` is two errors of one byte each — `MA` could have been `MAR` and `SX`
/// could not have been anything.
pub(crate) fn begins_a_name(prefix: u32) -> bool {
  NAMES.iter().any(|name| name >> 8 == prefix)
}

/// What a lexeme is, with none of what it says.
///
/// The parser reads a value where it recognises one, so the only thing left to report is
/// a *kind* — which is what an error message needs and nothing else does. Both lexical
/// failures are variants rather than an `Err`, so that a scan that fails and a scan that
/// finds a token it did not want are the same shape of answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Lexeme {
  /// `*` — every value the field admits.
  Star,
  /// `?` — "no specific value", or another spelling of `*`, depending on the dialect.
  Question,
  /// `/` — the step separator.
  Slash,
  /// `-` — the range separator, and the offset marker in Quartz's `L-n`.
  Hyphen,
  /// `,` — the list separator.
  Comma,
  /// `#` — Quartz's nth-weekday-of-month marker.
  Hash,
  /// `L` — Quartz's "last", in either case.
  Last,
  /// `W` — Quartz's "nearest weekday", in either case.
  Weekday,
  /// A run of digits that fits in a `u32`.
  Number,
  /// One of the nineteen names.
  Name,
  /// An `@`-prefixed nickname, including the `@`.
  Macro,
  /// A run of whitespace: the field separator.
  Space,
  /// A byte that begins no token in any dialect.
  UnexpectedCharacter,
  /// A run of digits too long to be a value in any field.
  NumberTooLarge,
}

/// What a run beginning with an ASCII letter turned out to be.
///
/// A name arrives as its place in [`NAMES`], not as the text: the scanner had to fold and
/// match the three letters to recognise it at all, so the index is already computed and
/// the slice would only have to be resolved again later.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Word {
  /// One of the nineteen names, by its index in [`NAMES`].
  Name(u8),
  /// `L`, in either case.
  Last,
  /// `W`, in either case.
  Weekday,
  /// A letter run that is none of those.
  Unexpected,
}

/// A position in one cron expression, and the recognisers that move it.
///
/// The parser holds one of these and nothing else. Every span it reports is a byte
/// offset into the original `&str`, taken as the difference between two positions, so
/// there is no subslicing and no offset arithmetic anywhere.
pub(crate) struct Cursor<'a> {
  input: &'a str,
  pos: usize,
}

impl<'a> Cursor<'a> {
  /// Starts at the beginning of `input`.
  pub(crate) const fn new(input: &'a str) -> Self {
    Self { input, pos: 0 }
  }

  /// Where the scan has reached.
  pub(crate) const fn pos(&self) -> usize {
    self.pos
  }

  /// Whether the input is exhausted.
  pub(crate) const fn at_end(&self) -> bool {
    self.pos >= self.input.len()
  }

  /// An empty span at the end of the input, for errors with no text to point at.
  pub(crate) const fn end_span(&self) -> Range<usize> {
    let end = self.input.len();
    end..end
  }

  /// The next byte, if there is one.
  pub(crate) fn peek(&self) -> Option<u8> {
    self.input.as_bytes().get(self.pos).copied()
  }

  /// Whether the next byte is this one.
  pub(crate) fn at(&self, byte: u8) -> bool {
    self.peek() == Some(byte)
  }

  /// Steps over one byte.
  ///
  /// Only ever called where the caller has just read that byte, so it cannot step over a
  /// character boundary.
  pub(crate) fn advance(&mut self) {
    debug_assert!(self.pos < self.input.len(), "advanced past the end");
    self.pos = self.pos.saturating_add(1);
  }

  /// Steps over a run of whitespace, if there is one.
  ///
  /// A run rather than a byte, because whitespace is the field separator and the
  /// separator is the whole run however long it is.
  pub(crate) fn skip_space(&mut self) {
    let bytes = self.input.as_bytes();
    while bytes.get(self.pos).copied().is_some_and(is_space_byte) {
      self.pos = self.pos.saturating_add(1);
    }
  }

  /// The unconsumed text, and the byte offset it starts at.
  ///
  /// `@every`'s duration is not cron syntax, so its scanner takes the raw tail. The
  /// offset comes back with it so that the duration's own errors still point into the
  /// whole expression.
  pub(crate) fn rest(&self) -> (&'a str, usize) {
    debug_assert!(self.input.is_char_boundary(self.pos));
    (self.input.get(self.pos..).unwrap_or(""), self.pos)
  }

  /// The text from `start` to where the scan has reached.
  fn slice_from(&self, start: usize) -> &'a str {
    debug_assert!(self.input.is_char_boundary(start) && self.input.is_char_boundary(self.pos));
    self.input.get(start..self.pos).unwrap_or("")
  }

  /// Reads a digit run's value.
  ///
  /// `None` when the run is too long to be a `u32` — a *lexical* failure, because no
  /// field could hold it whatever its bounds. The caller has established that the next
  /// byte is a digit.
  pub(crate) fn take_number(&mut self) -> Option<u32> {
    let bytes = self.input.as_bytes();
    let first = bytes.get(self.pos).copied().unwrap_or(b'0');
    debug_assert!(
      first.is_ascii_digit(),
      "a digit run has to start with a digit"
    );
    self.pos = self.pos.saturating_add(1);

    let mut value = u32::from(first.wrapping_sub(b'0'));
    let mut overflowed = false;
    while let Some(&digit) = bytes.get(self.pos) {
      if !digit.is_ascii_digit() {
        break;
      }
      self.pos += 1;
      let (scaled, past_mul) = value.overflowing_mul(10);
      let (summed, past_add) = scaled.overflowing_add(u32::from(digit - b'0'));
      // Sticky, because a run can overflow and then wrap back into range.
      overflowed |= past_mul | past_add;
      value = summed;
    }

    (!overflowed).then_some(value)
  }

  /// Reads a run beginning with an ASCII letter.
  ///
  /// The name is tried first and the single letters second, which is the longest match
  /// and is what makes `WED` a name while `W` on its own is [`Word::Weekday`]. The caller
  /// has established that the next byte is an ASCII letter.
  pub(crate) fn take_word(&mut self) -> Word {
    let bytes = self.input.as_bytes();
    let start = self.pos;
    let first = bytes.get(start).copied().unwrap_or(b'\0');
    debug_assert!(
      first.is_ascii_alphabetic(),
      "a word has to start with a letter"
    );
    self.pos = start.saturating_add(1);

    let letter = |offset: usize| bytes.get(offset).copied().filter(u8::is_ascii_alphabetic);
    let second = letter(start + 1);

    if let (Some(second), Some(third)) = (second, letter(start + 2)) {
      if let Some(index) = name_index(key(first, second, third)) {
        self.pos = start + 3;
        return Word::Name(index);
      }
    }

    match first {
      b'L' | b'l' => Word::Last,
      b'W' | b'w' => Word::Weekday,
      _ => {
        if second.is_some_and(|second| begins_a_name(prefix_key(first, second))) {
          self.pos = start + 2;
        }
        Word::Unexpected
      }
    }
  }

  /// Reads an `@`-prefixed nickname, including the `@`.
  ///
  /// `None` for a lone `@`, which is not one — **and the cursor does not move**, because
  /// a lone `@` is a byte that begins no token and the caller has to be able to go on
  /// from where it stands. The caller has established that the next byte is `@`.
  ///
  /// The text comes back because a nickname is matched by name and there are eight of
  /// them across three dialects — too few to be worth a table and too many to fold into
  /// the scan.
  pub(crate) fn take_macro(&mut self) -> Option<&'a str> {
    let bytes = self.input.as_bytes();
    let start = self.pos;
    debug_assert!(
      bytes.get(start) == Some(&b'@'),
      "a nickname starts with `@`"
    );

    let mut end = start.saturating_add(1);
    while bytes.get(end).is_some_and(u8::is_ascii_alphabetic) {
      end += 1;
    }
    if end == start + 1 {
      return None;
    }
    self.pos = end;
    Some(self.slice_from(start))
  }

  /// Steps over a byte that begins no token, and the rest of its character.
  ///
  /// A whole character rather than a byte, so that a span into the input is always
  /// sliceable and an error over a non-ASCII character points at the character instead
  /// of at half of one.
  fn take_unexpected(&mut self) {
    let mut end = self.pos.saturating_add(1);
    // Terminates at the input's length, which is always a boundary.
    while !self.input.is_char_boundary(end) {
      end += 1;
    }
    self.pos = end;
  }

  /// Recognises the next lexeme and steps over it, reporting only what it was.
  ///
  /// The whole scanner in one place, and the only caller is an error path. The fast
  /// paths above test the bytes they care about directly; this is what names a lexeme
  /// once the parser has decided it does not want it.
  pub(crate) fn take_lexeme(&mut self) -> Option<Lexeme> {
    let first = self.peek()?;
    Some(match first {
      b'*' => {
        self.advance();
        Lexeme::Star
      }
      b'?' => {
        self.advance();
        Lexeme::Question
      }
      b'/' => {
        self.advance();
        Lexeme::Slash
      }
      b'-' => {
        self.advance();
        Lexeme::Hyphen
      }
      b',' => {
        self.advance();
        Lexeme::Comma
      }
      b'#' => {
        self.advance();
        Lexeme::Hash
      }
      b'0'..=b'9' => match self.take_number() {
        Some(_) => Lexeme::Number,
        None => Lexeme::NumberTooLarge,
      },
      b'@' => match self.take_macro() {
        Some(_) => Lexeme::Macro,
        // `take_macro` leaves a lone `@` where it found it, so the step over it is
        // this scan's to make.
        None => {
          self.advance();
          Lexeme::UnexpectedCharacter
        }
      },
      byte if is_space_byte(byte) => {
        self.skip_space();
        Lexeme::Space
      }
      byte if byte.is_ascii_alphabetic() => match self.take_word() {
        Word::Name(_) => Lexeme::Name,
        Word::Last => Lexeme::Last,
        Word::Weekday => Lexeme::Weekday,
        Word::Unexpected => Lexeme::UnexpectedCharacter,
      },
      _ => {
        self.take_unexpected();
        Lexeme::UnexpectedCharacter
      }
    })
  }

  /// A cursor at the same place, for looking ahead without moving.
  fn ahead(&self) -> Self {
    Self {
      input: self.input,
      pos: self.pos,
    }
  }

  /// What the next lexeme is and where it sits, without stepping over it.
  pub(crate) fn peek_lexeme(&self) -> Option<(Lexeme, Range<usize>)> {
    let mut ahead = self.ahead();
    let lexeme = ahead.take_lexeme()?;
    Some((lexeme, self.pos..ahead.pos))
  }

  /// What a run beginning with an ASCII letter would turn out to be.
  ///
  /// `None` when the next byte is not a letter. This is what tells `W` from the `WED`
  /// that starts with it, which a byte test cannot.
  pub(crate) fn peek_word(&self) -> Option<Word> {
    if !self.peek()?.is_ascii_alphabetic() {
      return None;
    }
    Some(self.ahead().take_word())
  }

  /// The span the next lexeme occupies, or the end-of-input span.
  pub(crate) fn next_span(&self) -> Range<usize> {
    match self.peek_lexeme() {
      Some((_, span)) => span,
      None => self.end_span(),
    }
  }
}
