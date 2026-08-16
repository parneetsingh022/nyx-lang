// SPDX-License-Identifier: MIT OR Apache-2.0

use miette::SourceSpan;

use nyx_diagnostic::LexerError;
use nyx_source::{SourceFile, Span};
use nyx_token::{SymbolRegistry, Token, TokenKind};

#[cfg(test)]
mod tests;

/// Returns whether the byte is ASCII whitespace.
///
/// This includes spaces, tabs, newlines, carriage returns, and other ASCII
/// whitespace bytes recognized by [`u8::is_ascii_whitespace`].
#[inline]
fn is_whitespace(ch: char) -> bool {
    ch.is_ascii_whitespace()
}

/// Returns whether the byte can start an identifier.
///
/// Identifiers may start with an ASCII letter (`a-z`, `A-Z`) or an underscore
/// (`_`).
#[inline]
fn is_ident_start(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ch == '_'
}

/// Returns whether the byte can continue an identifier.
///
/// After the first character, identifiers may contain ASCII letters, digits
/// (`0-9`), or underscores (`_`).
#[inline]
fn is_ident_continue(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

/// Tracks current position for lexer in source file
#[derive(Debug, Default, Eq, PartialEq, Clone, Copy)]
pub struct Cursor {
    offset: usize,
}

impl Cursor {
    fn consume(&mut self, ch: char) {
        self.offset += ch.len_utf8();
    }
}

pub struct Lexer {
    source_file: SourceFile,
    cursor: Cursor,

    // Diagnostics that can be reported together after tokenization.
    //
    // Some lexer errors do not prevent us from continuing to scan the rest of
    // the file. For those cases, we record the diagnostic here and keep going,
    // so the user can see multiple errors at once.
    //
    // Fatal errors are different: if the lexer cannot reliably continue, such as
    // after an unterminated block comment or string, the error is returned
    // immediately as `Err` from the iterator.
    errors: Vec<LexerError>,
    pub symbol_registry: SymbolRegistry,
}

impl Iterator for Lexer {
    type Item = Result<Token, LexerError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.is_eof() {
            return None;
        }

        self.lex_next_token()
    }
}

impl Lexer {
    /// Creates a lexer for the given shared source file.
    ///
    /// The lexer borrows the source text from the provided [`SourceFile`] and
    /// retains a clone of the shared pointer for generating diagnostics.
    pub fn new(source_file: SourceFile) -> Self {
        Self {
            source_file,
            cursor: Cursor::default(),
            errors: Vec::new(),
            symbol_registry: SymbolRegistry::new(),
        }
    }

    /// takes all lexer errors collected so far, leaving the lexer with an empty
    /// error list.
    pub fn take_errors(&mut self) -> Vec<LexerError> {
        std::mem::take(&mut self.errors)
    }

    pub fn take_registry(&mut self) -> SymbolRegistry {
        std::mem::take(&mut self.symbol_registry)
    }

    /// Returns whether the cursor has reached or passed the end of the source.
    #[inline]
    fn is_eof(&self) -> bool {
        self.cursor.offset >= self.source_file.contents().len()
    }

    /// Returns the unconsumed portion of the source starting at the current cursor.
    ///
    /// The cursor offset must always lie on a valid UTF-8 character boundary.
    #[inline]
    fn remaining(&self) -> &str {
        &self.source_file.contents()[self.cursor.offset..]
    }

    /// Returns the character at the current cursor position without consuming it.
    ///
    /// Returns `None` if the cursor is at the end of the source.
    #[inline]
    fn peek(&self) -> Option<char> {
        self.remaining().chars().next()
    }

    /// Checks if the remaining source string starts with the provided pattern.
    ///
    /// This performs a non-consuming check, allowing the lexer to look ahead
    /// for multi-character tokens without advancing the internal cursor.
    ///
    /// # Arguments
    ///
    /// * `s` - The string pattern to match against the current position.
    ///
    /// # Returns
    ///
    /// * `true` if the source at the current cursor matches the pattern `s`.
    /// * `false` otherwise, or if the remaining source is shorter than `s`.
    #[inline]
    fn starts_with(&self, s: &str) -> bool {
        self.remaining().starts_with(s)
    }

    /// Consumes the current character if it matches `expected`.
    ///
    /// Returns `true` if the character matched and was consumed, or `false`
    /// if the current character is different or the cursor is at the end
    /// of the source.
    #[inline]
    fn consume_if(&mut self, expected: char) -> bool {
        if self.peek() == Some(expected) {
            self.advance();
            true
        } else {
            false
        }
    }

    /// Advances the cursor by one Unicode character.
    ///
    /// Returns `true` if a character was consumed, or `false` if the cursor
    /// is already at the end of the source.
    #[inline]
    fn advance(&mut self) -> bool {
        let Some(ch) = self.peek() else {
            return false;
        };

        self.cursor.consume(ch);
        true
    }

    /// Advances the cursor by up to `n` Unicode characters.
    ///
    /// Stops early if the end of the source is reached.
    fn advance_n(&mut self, n: usize) {
        for _ in 0..n {
            if !self.advance() {
                break;
            }
        }
    }

    /// Creates a span from `start` to the lexer's current cursor position.
    ///
    /// The start position is usually captured before consuming a token, while the
    /// current cursor position marks the end of that token.
    fn span_from(&self, start: Cursor) -> Span {
        // `SourceFile::new` rejects sources larger than `u32::MAX`, and cursor
        // offsets never advance beyond the source, so these conversions must succeed.
        Span::new(start.offset as u32, self.cursor.offset as u32)
    }

    /// Consumes bytes while `predicate` returns true and returns the consumed text.
    ///
    /// Returns the span covering consumed portion of the source.
    fn read_while(&mut self, predicate: impl Fn(char) -> bool) -> Span {
        let start = self.cursor;
        while let Some(ch) = self.peek()
            && predicate(ch)
        {
            self.advance();
        }

        self.span_from(start)
    }

    fn skip_whitespace(&mut self) {
        while let Some(ch) = self.peek()
            && is_whitespace(ch)
        {
            self.advance();
        }
    }

    fn skip_single_line_comments(&mut self) {
        if !self.starts_with("//") {
            return;
        }

        while let Some(ch) = self.peek() {
            if ch == '\n' {
                break;
            }

            self.advance();
        }
    }

    /// Skips over multi-line comments `/* ... */`.
    ///
    /// If the comment is not terminated (EOF reached), it returns an `UnterminatedCommentError`.
    fn skip_multi_line_comment(&mut self) -> Result<(), LexerError> {
        if !self.starts_with("/*") {
            return Ok(());
        }

        let start = self.cursor;
        self.advance_n(2); // Consume "/*"

        while !self.starts_with("*/") {
            if self.is_eof() {
                // Limit the error span to just the opening "/*"
                // by setting the length to two
                let error_span = SourceSpan::new(start.offset.into(), 2);
                // Unterminated block comments are fatal because the lexer cannot reliably
                // determine where normal tokenization should resume.
                return Err(LexerError::UnterminatedComment {
                    at: error_span,
                    src: self.source_file.clone(),
                });
            }
            self.advance();
        }

        self.advance_n(2); // Consume "*/"

        Ok(())
    }

    fn push_diagnostic(&mut self, err: LexerError) {
        // Store diagnostics for errors where the lexer can still continue scanning.
        // These are printed together after tokenization finishes.
        self.errors.push(err);
    }

    /// Creates an `Unexpected` token for an unrecognized character.
    ///
    /// The character is consumed, and a diagnostic is stored so lexing can continue
    /// and report more errors from the same file.
    fn unexpected_char_token(&mut self, start: Cursor, ch: char) -> Token {
        self.advance();

        let span = self.span_from(start);

        self.push_diagnostic(LexerError::UnexpectedChar {
            char: ch,
            at: span.into(),
            src: self.source_file.clone(),
        });

        Token::new(TokenKind::Unexpected, span)
    }

    /// Creates an `Unexpected` token for a number with an invalid suffix.
    ///
    /// The full suffix is consumed so input like `123abc` is reported as one invalid
    /// token instead of an integer token followed by an identifier token.
    fn invalid_numeric_suffix_token(&mut self, start: Cursor) -> Token {
        // Consume the full suffix so `123abc` becomes one error token,
        // not an integer token followed by an identifier token.
        self.read_while(is_ident_continue);

        let span = self.span_from(start);

        self.push_diagnostic(LexerError::InvalidNumericSuffix {
            at: span.into(),
            src: self.source_file.clone(),
        });

        Token::new(TokenKind::Unexpected, span)
    }

    /// Creates an `Unexpected` token for an incomplete floating-point literal.
    ///
    /// This handles numbers ending with a decimal point, such as `123.`. The
    /// diagnostic includes a suggested `.0` completion.
    fn incomplete_float_token(&mut self, start: Cursor) -> Token {
        let span = self.span_from(start);
        debug_assert!(!span.is_empty(), "lex_float received an empty span");

        let source_span = span.into();

        let value_span = Span::new(span.start(), span.end() - 1);
        let err = LexerError::IncompleteFloat {
            // `span.end - 1` removes the trailing `.` from the suggestion value.
            value: self.source_file.slice(value_span).to_string(),
            at: source_span,
            suggestion: source_span,
            src: self.source_file.clone(),
        };

        self.push_diagnostic(err);
        Token::new(TokenKind::Unexpected, span)
    }

    fn lex_next_token(&mut self) -> Option<Result<Token, LexerError>> {
        loop {
            let start_offset = self.cursor.offset;

            self.skip_whitespace();
            self.skip_single_line_comments();
            if let Err(err) = self.skip_multi_line_comment() {
                return Some(Err(err));
            }

            // If the offset didn't move, current char
            // doesn't represent any whitespace or comment
            if start_offset == self.cursor.offset {
                break;
            }
        }

        let ch = self.peek()?;
        let token = match ch {
            _ if is_ident_start(ch) => self.lex_identifier(),
            _ if ch.is_ascii_digit() => self.lex_number(),

            // Double char tokens
            _ if self.starts_with("&&") => self.lex_double_char_token(TokenKind::And),
            _ if self.starts_with("||") => self.lex_double_char_token(TokenKind::Or),

            // Potential two character symbols
            '+' => self.lex_compound_operator('+', TokenKind::PlusPlus, TokenKind::Plus),
            '-' => self.lex_compound_operator('-', TokenKind::MinusMinus, TokenKind::Minus),
            '=' => self.lex_compound_operator('=', TokenKind::EqEq, TokenKind::Eq),
            '!' => self.lex_compound_operator('=', TokenKind::BangEq, TokenKind::Bang),
            '<' => self.lex_compound_operator('=', TokenKind::LtEq, TokenKind::Lt),
            '>' => self.lex_compound_operator('=', TokenKind::GtEq, TokenKind::Gt),

            // Single char symbols
            '*' => self.lex_single_char_token(TokenKind::Star),
            '/' => self.lex_single_char_token(TokenKind::Slash),
            '%' => self.lex_single_char_token(TokenKind::Percent),
            '^' => self.lex_single_char_token(TokenKind::Caret),
            ';' => self.lex_single_char_token(TokenKind::Semi),
            ',' => self.lex_single_char_token(TokenKind::Comma),
            '.' => self.lex_single_char_token(TokenKind::Dot),
            '(' => self.lex_single_char_token(TokenKind::OpenParen),
            ')' => self.lex_single_char_token(TokenKind::CloseParen),
            '{' => self.lex_single_char_token(TokenKind::OpenBrace),
            '}' => self.lex_single_char_token(TokenKind::CloseBrace),
            '[' => self.lex_single_char_token(TokenKind::OpenBracket),
            ']' => self.lex_single_char_token(TokenKind::CloseBracket),
            invalid_char => self.unexpected_char_token(self.cursor, invalid_char),
        };

        Some(Ok(token))
    }

    fn lex_identifier(&mut self) -> Token {
        let span = self.read_while(is_ident_continue);
        let ident = self.source_file.slice(span);

        // Attempt to classify the identifier as a language keyword.
        // If it is not a keyword, fall back to treating it as a standard identifier.
        let kind = TokenKind::map_keyword(ident)
            .unwrap_or(TokenKind::identifier(&mut self.symbol_registry, ident));

        Token::new(kind, span)
    }

    /// Lexes an integer or floating-point number.
    ///
    /// If the number is followed by a `.`, lexing continues as a float.
    fn lex_number(&mut self) -> Token {
        let start = self.cursor;
        let span = self.read_while(|ch| ch.is_ascii_digit());
        let value = self.source_file.slice(span);

        // A `.` after digits means this number may be a float.
        if self.peek() == Some('.') {
            return self.lex_float(start);
        }

        // Identifiers cannot be attached directly to number literals.
        if let Some(ch) = self.peek()
            && is_ident_start(ch)
        {
            return self.invalid_numeric_suffix_token(start);
        }
        let span = self.span_from(start);
        Token::new(
            TokenKind::int_literal(&mut self.symbol_registry, value),
            span,
        )
    }

    /// Continues lexing a floating-point number after the integer part.
    ///
    /// This assumes the current byte is `.` and consumes it before reading the
    /// fractional digits.
    fn lex_float(&mut self, start: Cursor) -> Token {
        // We enter this function only after seeing `.`, so consume it first.
        self.advance();

        let span = self.read_while(|ch| ch.is_ascii_digit());
        let rest = self.source_file.slice(span);

        if rest.is_empty() {
            // A decimal point must be followed by at least one digit.
            // For example, `123.` is treated as an incomplete float.
            return self.incomplete_float_token(start);
        }

        if let Some(ch) = self.peek()
            && is_ident_start(ch)
        {
            // A number cannot be immediately followed by an identifier-like suffix.
            // For example, `123abc` or `123.45abc` should be reported as one
            // invalid numeric token instead of separate number and identifier tokens.
            return self.invalid_numeric_suffix_token(start);
        }

        let span = self.span_from(start);
        let value = self.source_file.slice(span);

        Token::new(
            TokenKind::float_literal(&mut self.symbol_registry, value),
            span,
        )
    }

    /// Lexes a single-character token.
    ///
    /// Captures the cursor position, consumes the current character by advancing,
    /// and then constructs a new token using the span from the captured start
    /// position to the new cursor position.
    fn lex_single_char_token(&mut self, kind: TokenKind) -> Token {
        let start = self.cursor;
        self.advance();
        Token::new(kind, self.span_from(start))
    }

    /// Lexes a two-character token.
    ///
    /// Captures the cursor position, consumes the next two character by advancing,
    /// and then constructs a new token using the span from the captured start
    /// position to the new cursor position.
    fn lex_double_char_token(&mut self, kind: TokenKind) -> Token {
        let start = self.cursor;
        self.advance_n(2);
        Token::new(kind, self.span_from(start))
    }

    /// Lexes an operator that may have a one- or two-character form.
    ///
    /// After consuming the first character, this method checks whether the next
    /// character matches `expected_second`. If it does, the operator is emitted
    /// with `compound_kind`; otherwise, it is emitted with `single_kind`.
    ///
    /// Examples include `+` and `++`, `-` and `--`, and `=` and `==`.
    fn lex_compound_operator(
        &mut self,
        expected: char,
        compound_kind: TokenKind,
        single_kind: TokenKind,
    ) -> Token {
        let start = self.cursor;
        // Consume the first character, which was already matched by the caller.
        self.advance();

        let kind = if self.consume_if(expected) {
            compound_kind
        } else {
            single_kind
        };

        Token::new(kind, self.span_from(start))
    }
}
