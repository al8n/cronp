//! The lexical surface of every cron dialect.
//!
//! The lexer is **total**: it knows nothing about dialects and rejects nothing on
//! dialect grounds. `?` is a token even though Vixie has no `?`, `L` and `W` and `#` are
//! tokens even though only Quartz uses them, and `@every` is a token even though only
//! the Go dialect accepts it. Deciding which of these a given dialect allows is the
//! parser's job, and keeping that decision out of here is what lets one lexer serve all
//! of them.

use logos::{Lexer, Logos};

#[cfg(test)]
mod tests;

/// Why the lexer could not turn the input into a token.
///
/// Both variants describe a *lexical* failure. An input that lexes cleanly but that no
/// dialect's grammar accepts — `1-`, a lone `/`, `#` with no digit — is not represented
/// here, because the lexer is right to accept it and the parser is what rejects it.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LexError {
  /// A byte that begins no token in any dialect.
  #[default]
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
#[derive(Logos, Debug, Clone, Copy, PartialEq, Eq)]
#[logos(error = LexError)]
pub(crate) enum Token<'a> {
  /// `*` — every value the field admits.
  #[token("*")]
  Star,
  /// `?` — "no specific value". Quartz requires it in exactly one of day-of-month and
  /// day-of-week; the Go dialect takes it as a synonym for `*`; Vixie has no such token.
  #[token("?")]
  Question,
  /// `/` — the step separator.
  #[token("/")]
  Slash,
  /// `-` — the range separator, and the offset marker in Quartz's `L-n`.
  #[token("-")]
  Hyphen,
  /// `,` — the list separator.
  #[token(",")]
  Comma,
  /// `#` — Quartz's nth-weekday-of-month marker.
  #[token("#")]
  Hash,
  /// `L` — Quartz's "last". Its meaning depends on the field it appears in.
  ///
  /// Spelled as an explicit ASCII class rather than `ignore(case)`, which logos 0.16
  /// implements as Unicode case folding: cron is an ASCII grammar and the folded classes
  /// would quietly admit characters no crontab contains.
  #[regex("[Ll]")]
  Last,
  /// `W` — Quartz's "nearest weekday".
  #[regex("[Ww]")]
  Weekday,
  /// A run of digits, as a value.
  ///
  /// The width is the lexer's, not any field's: a value outside a field's bounds is a
  /// parse error with a range in its message, which is more useful than a lexical
  /// failure. Only a run too long for `u32` fails here.
  #[regex("[0-9]+", parse_number)]
  Number(u32),
  /// A three-letter month or weekday name, in the case the input wrote it.
  ///
  /// The nineteen names are spelled out rather than matched as "three letters" so that
  /// `LW` lexes as [`Token::Last`] followed by [`Token::Weekday`] instead of being
  /// swallowed by a longer name match.
  #[regex(
    "[Jj][Aa][Nn]|[Ff][Ee][Bb]|[Mm][Aa][Rr]|[Aa][Pp][Rr]|[Mm][Aa][Yy]|[Jj][Uu][Nn]\
     |[Jj][Uu][Ll]|[Aa][Uu][Gg]|[Ss][Ee][Pp]|[Oo][Cc][Tt]|[Nn][Oo][Vv]|[Dd][Ee][Cc]\
     |[Ss][Uu][Nn]|[Mm][Oo][Nn]|[Tt][Uu][Ee]|[Ww][Ee][Dd]|[Tt][Hh][Uu]|[Ff][Rr][Ii]\
     |[Ss][Aa][Tt]",
    slice
  )]
  Name(&'a str),
  /// An `@`-prefixed nickname, including the `@` — `@daily`, `@reboot`, `@every`.
  ///
  /// For `@every` the duration that follows is not cron syntax and is deliberately not
  /// lexed here; the caller reads it from [`Lexer::remainder`].
  #[regex("@[a-zA-Z]+", slice)]
  Macro(&'a str),
  /// A run of whitespace: the field separator.
  #[regex(r"[ \t\r\n\x0C]+")]
  Space,
}

fn parse_number<'a>(lex: &Lexer<'a, Token<'a>>) -> Result<u32, LexError> {
  lex
    .slice()
    .parse::<u32>()
    .map_err(|_| LexError::NumberTooLarge)
}

fn slice<'a>(lex: &Lexer<'a, Token<'a>>) -> &'a str {
  lex.slice()
}
