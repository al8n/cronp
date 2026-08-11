//! The scanner as it stood when the parser consumed a token stream.
//!
//! The whitespace class and the name table stay in [`crate::token`], because they are
//! lexical *data* rather than scanning logic and a second copy would only be a second
//! thing to keep in step. Everything that decides where one lexeme ends and the next
//! begins is here, so that the fused scanner is checked against this one rather than
//! against a paraphrase of it.
//!
//! The scanner is unchanged from what it replaced. [`Cursor::peek_spanned`] is the one
//! addition: [`Cursor::peek_token`] answers `None` for a lexical failure and for the end
//! of the input alike, and the parser needs to tell those apart.

use core::ops::Range;

use crate::token::{begins_a_name, key, name_index, prefix_key};

#[cfg(test)]
pub(crate) mod tests;

/// Why the lexer could not turn the input into a token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LexError {
  /// A byte that begins no token in any dialect.
  UnexpectedCharacter,
  /// A run of digits too long to be a value in any field.
  NumberTooLarge,
}

/// One lexeme of a cron expression.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Token<'a> {
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
  /// `H` — a value chosen by hashing a seed, in either case.
  Hashed,
  /// A run of digits, as a value.
  Number(u32),
  /// A three-letter month or weekday name, in the case the input wrote it.
  Name(&'a str),
  /// An `@`-prefixed nickname, including the `@`.
  Macro(&'a str),
  /// A run of whitespace: the field separator.
  Space,
}

/// A lex result and the byte range it came from.
pub(crate) type Spanned<'a> = (Result<Token<'a>, LexError>, Range<usize>);

/// A scanner over one cron expression.
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
  fn word(&mut self, first: u8, start: usize, bytes: &[u8]) -> Result<Token<'a>, LexError> {
    let letter = |offset: usize| bytes.get(offset).copied().filter(u8::is_ascii_alphabetic);
    let second = letter(start + 1);

    if let (Some(second), Some(third)) = (second, letter(start + 2)) {
      if name_index(key(first, second, third)).is_some() {
        self.pos = start + 3;
        return Ok(Token::Name(self.slice(start)));
      }
    }

    match first {
      b'L' | b'l' => Ok(Token::Last),
      b'W' | b'w' => Ok(Token::Weekday),
      b'H' | b'h' => Ok(Token::Hashed),
      _ => {
        if second.is_some_and(|second| begins_a_name(prefix_key(first, second))) {
          self.pos = start + 2;
        }
        Err(LexError::UnexpectedCharacter)
      }
    }
  }

  /// A byte that begins no token at all, spanned over a whole character.
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
      byte if crate::token::is_space_byte(byte) => {
        while bytes
          .get(self.pos)
          .copied()
          .is_some_and(crate::token::is_space_byte)
        {
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

  /// The next lex result and the span it came from, if the input has not run out.
  ///
  /// [`Self::peek_token`] cannot tell a lexical failure from the end of the input —
  /// both are `None` — and two of this parser's decisions turn on the difference.
  pub(crate) fn peek_spanned(&self) -> Option<&Spanned<'a>> {
    self.next.as_ref()
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
