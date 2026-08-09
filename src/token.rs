//! The lexical surface of every cron dialect.
//!
//! The lexer is **total**: it knows nothing about dialects and rejects nothing on
//! dialect grounds. `?` is a token even though Vixie has no `?`, `L` and `W` and `#` are
//! tokens even though only Quartz uses them, and `@every` is a token even though only
//! the Go dialect accepts it. Deciding which of these a given dialect allows is the
//! parser's job, and keeping that decision out of here is what lets one lexer serve all
//! of them.
//!
//! Cron's lexical surface is six one-byte punctuation marks, two one-byte
//! case-insensitive letters, a digit run, a closed set of nineteen three-letter names, an
//! `@`-prefixed nickname and a whitespace run — all ASCII. That is small enough to scan
//! by hand and too small to be worth a generated automaton, which is why [`Scanner`] is
//! written out here. It stays bounds-checked and allocation-free, and the crate keeps
//! `#![forbid(unsafe_code)]`.

use core::ops::Range;

/// Why the lexer could not turn the input into a token.
///
/// Both variants describe a *lexical* failure. An input that lexes cleanly but that no
/// dialect's grammar accepts — `1-`, a lone `/`, `#` with no digit — is not represented
/// here, because the lexer is right to accept it and the parser is what rejects it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LexError {
  /// A byte that begins no token in any dialect.
  UnexpectedCharacter,
  /// A run of digits too long to be a value in any field.
  NumberTooLarge,
}

/// One lexeme of a cron expression.
///
/// Whitespace is a [`Token::Space`] rather than something the lexer skips, because in
/// cron whitespace *is* the field separator: skipping it would leave the parser unable to
/// tell five fields from one. Keeping it a token also keeps every span a byte offset into
/// the original input, with no subslicing and no offset arithmetic anywhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Token<'a> {
  /// `*` — every value the field admits.
  Star,
  /// `?` — "no specific value". Quartz requires it in exactly one of day-of-month and
  /// day-of-week; the Go dialect takes it as a synonym for `*`; Vixie has no such token.
  Question,
  /// `/` — the step separator.
  Slash,
  /// `-` — the range separator, and the offset marker in Quartz's `L-n`.
  Hyphen,
  /// `,` — the list separator.
  Comma,
  /// `#` — Quartz's nth-weekday-of-month marker.
  Hash,
  /// `L` — Quartz's "last". Its meaning depends on the field it appears in.
  ///
  /// Matched as ASCII `L` or `l` and nothing else: cron is an ASCII grammar, and a
  /// Unicode case fold would quietly admit characters no crontab contains.
  Last,
  /// `W` — Quartz's "nearest weekday".
  Weekday,
  /// A run of digits, as a value.
  ///
  /// The width is the lexer's, not any field's: a value outside a field's bounds is a
  /// parse error with a range in its message, which is more useful than a lexical
  /// failure. Only a run too long for `u32` fails here.
  Number(u32),
  /// A three-letter month or weekday name, in the case the input wrote it.
  Name(&'a str),
  /// An `@`-prefixed nickname, including the `@` — `@daily`, `@reboot`, `@every`.
  ///
  /// For `@every` the duration that follows is not cron syntax and is deliberately not
  /// lexed here; the caller reads it from [`Cursor::rest`].
  Macro(&'a str),
  /// A run of whitespace: the field separator.
  Space,
}

/// A lex result and the byte range it came from.
pub(crate) type Spanned<'a> = (Result<Token<'a>, LexError>, Range<usize>);

/// Whether a byte is one [`Token::Space`] would match.
///
/// This class is the field separator, so it is also what
/// [`count_fields`](crate::schedule) splits on. One definition, because a lexer and a
/// field counter that disagreed about whitespace would disagree about how many fields an
/// expression has.
pub(crate) const fn is_space_byte(byte: u8) -> bool {
  matches!(byte, b' ' | b'\t' | b'\r' | b'\n' | b'\x0C')
}

/// The nineteen month and weekday names, case-folded by [`key`].
///
/// Spelled out as a closed set rather than matched as "three letters" so that `LW` lexes
/// as [`Token::Last`] followed by [`Token::Weekday`] instead of being swallowed by a
/// longer name match. `lw_is_two_modifiers_not_a_name` pins that.
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

pub(crate) fn is_name(folded: u32) -> bool {
  NAMES.contains(&folded)
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

/// A hand-written scanner over one cron expression.
///
/// Yields every lexeme in order, including the failures: a byte that begins no token is
/// an [`Err`] with a span rather than the end of the stream, so the parser can report
/// where the input went wrong instead of where it ran out.
pub(crate) struct Scanner<'a> {
  input: &'a str,
  pos: usize,
}

impl<'a> Scanner<'a> {
  /// Starts a scan at the beginning of `input`.
  pub(crate) const fn new(input: &'a str) -> Self {
    Self { input, pos: 0 }
  }

  /// The whole expression being scanned.
  pub(crate) const fn input(&self) -> &'a str {
    self.input
  }

  /// The text from `start` to where the scan has reached.
  fn slice(&self, start: usize) -> &'a str {
    debug_assert!(self.input.is_char_boundary(start) && self.input.is_char_boundary(self.pos));
    self.input.get(start..self.pos).unwrap_or("")
  }

  /// A digit run, as a value.
  fn number(&mut self, first: u8, bytes: &[u8]) -> Result<Token<'a>, LexError> {
    let mut value = u32::from(first - b'0');
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
    if overflowed {
      Err(LexError::NumberTooLarge)
    } else {
      Ok(Token::Number(value))
    }
  }

  /// An `@`-prefixed nickname. A lone `@` is not one.
  fn nickname(&mut self, start: usize, bytes: &[u8]) -> Result<Token<'a>, LexError> {
    while bytes.get(self.pos).is_some_and(u8::is_ascii_alphabetic) {
      self.pos += 1;
    }
    if self.pos > start + 1 {
      Ok(Token::Macro(self.slice(start)))
    } else {
      Err(LexError::UnexpectedCharacter)
    }
  }

  /// A run beginning with an ASCII letter: a three-letter name, `L`, `W`, or a failure.
  ///
  /// The name is tried first and the single letters second, which is the longest match
  /// and is what makes `WED` a name while `W` on its own is [`Token::Weekday`].
  fn word(&mut self, first: u8, start: usize, bytes: &[u8]) -> Result<Token<'a>, LexError> {
    let letter = |offset: usize| bytes.get(offset).copied().filter(u8::is_ascii_alphabetic);
    let second = letter(start + 1);

    if let (Some(second), Some(third)) = (second, letter(start + 2)) {
      if is_name(key(first, second, third)) {
        self.pos = start + 3;
        return Ok(Token::Name(self.slice(start)));
      }
    }

    match first {
      b'L' | b'l' => Ok(Token::Last),
      b'W' | b'w' => Ok(Token::Weekday),
      _ => {
        if second.is_some_and(|second| begins_a_name(prefix_key(first, second))) {
          self.pos = start + 2;
        }
        Err(LexError::UnexpectedCharacter)
      }
    }
  }

  /// A byte that begins no token at all.
  ///
  /// The span covers a whole character rather than a byte, so that a span into the input
  /// is always sliceable and an error over a non-ASCII character points at the character
  /// instead of at half of one.
  fn unexpected(&mut self, start: usize) -> Result<Token<'a>, LexError> {
    let mut end = start + 1;
    // Terminates at the input's length, which is always a boundary.
    while !self.input.is_char_boundary(end) {
      end += 1;
    }
    self.pos = end;
    Err(LexError::UnexpectedCharacter)
  }
}

impl<'a> Iterator for Scanner<'a> {
  type Item = Spanned<'a>;

  fn next(&mut self) -> Option<Self::Item> {
    let bytes = self.input.as_bytes();
    let start = self.pos;
    let first = *bytes.get(start)?;
    self.pos = start + 1;

    let token = match first {
      b'*' => Ok(Token::Star),
      b'?' => Ok(Token::Question),
      b'/' => Ok(Token::Slash),
      b'-' => Ok(Token::Hyphen),
      b',' => Ok(Token::Comma),
      b'#' => Ok(Token::Hash),
      b'0'..=b'9' => self.number(first, bytes),
      b'@' => self.nickname(start, bytes),
      byte if is_space_byte(byte) => {
        while bytes.get(self.pos).copied().is_some_and(is_space_byte) {
          self.pos += 1;
        }
        Ok(Token::Space)
      }
      byte if byte.is_ascii_alphabetic() => self.word(byte, start, bytes),
      _ => self.unexpected(start),
    };

    Some((token, start..self.pos))
  }
}

/// A one-token-lookahead view over the scanner.
///
/// The parser needs to see the next token before deciding what to do with it, and to
/// know where the input ended when it ends too early. Both are the cursor's job, so no
/// parser ever touches the scanner or the input directly — which is what keeps every span
/// a byte offset into the original `&str` with no subslicing anywhere.
pub(crate) struct Cursor<'a> {
  scanner: Scanner<'a>,
  next: Option<Spanned<'a>>,
}

impl<'a> Cursor<'a> {
  /// Starts a cursor over a whole expression.
  pub(crate) fn new(input: &'a str) -> Self {
    let mut scanner = Scanner::new(input);
    let next = scanner.next();
    Self { scanner, next }
  }

  /// The next token's variant, if there is one and it lexed.
  pub(crate) fn peek_token(&self) -> Option<Token<'a>> {
    match self.next {
      Some((Ok(token), _)) => Some(token),
      _ => None,
    }
  }

  /// Consumes and returns the next token.
  pub(crate) fn bump(&mut self) -> Option<Spanned<'a>> {
    let current = self.next.take();
    self.next = self.scanner.next();
    current
  }

  /// Whether the input is exhausted.
  pub(crate) fn at_end(&self) -> bool {
    self.next.is_none()
  }

  /// An empty span at the end of the input, for errors with no text to point at.
  pub(crate) fn end_span(&self) -> Range<usize> {
    let end = self.scanner.input().len();
    end..end
  }

  /// The unconsumed text, and the byte offset it starts at.
  ///
  /// `@every`'s duration is not cron syntax, so its scanner takes the raw tail rather
  /// than a token stream. The offset comes back with it so that the duration's own
  /// errors still point into the whole expression.
  pub(crate) fn rest(&self) -> (&'a str, usize) {
    let input = self.scanner.input();
    let start = self.next_span().start;
    debug_assert!(input.is_char_boundary(start));
    (input.get(start..).unwrap_or(""), start)
  }

  /// The span the next token occupies, or the end-of-input span.
  pub(crate) fn next_span(&self) -> Range<usize> {
    match &self.next {
      Some((_, span)) => span.clone(),
      None => self.end_span(),
    }
  }
}
