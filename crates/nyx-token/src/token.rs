// SPDX-License-Identifier: MIT OR Apache-2.0

//! Defines the core tokens used by the Nyx lexer and parser.
//!
//! A [`Token`] ties together two important pieces of information: what a piece
//! of code actually means (its [`TokenKind`]), and exactly where it lives in
//! the source file (its [`Span`]).
//!
//! To keep the compiler fast and memory-friendly, dynamic text like variable
//! names and numbers aren't stored directly inside the tokens. Instead, they
//! are saved in a centralized [`SymbolRegistry`] and referenced using tiny,
//! lightweight [`Symbol`] values.

use std::{fmt, str::FromStr};

use strum_macros::{Display, EnumString};

use crate::{Symbol, SymbolRegistry};
use nyx_source::Span;

/// A single, meaningful chunk of source code parsed by the lexer.
///
/// Every token knows both what it is (its [`TokenKind`]) and exactly where
/// it was found in the source text (its [`Span`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

impl Token {
    pub fn new(kind: TokenKind, span: Span) -> Self {
        Self { kind, span }
    }
}

/// A reserved keyword within the Nyx programming language.
///
/// The variants are serialized in `snake_case` to seamlessly integrate with
/// [`FromStr`] for automated string parsing and [`fmt::Display`] for formatted output.
#[derive(Debug, Display, Clone, Copy, Eq, PartialEq, EnumString)]
#[strum(serialize_all = "snake_case")]
pub enum Keyword {
    Let,
    Const,
    If,
    Else,
    True,  // Boolean 'true'
    False, // Boolean 'false'
}

impl Keyword {
    pub fn is_boolean(&self) -> bool {
        matches!(self, Self::True | Self::False)
    }
}

/// All the possible types of tokens the lexer can recognize.
///
/// To save memory, tokens with dynamic text (like identifiers and numbers)
/// don't hold their own strings. Instead, they hold a lightweight [`Symbol`]
/// that points to the actual text in a registry.
///
/// Fixed tokens—like operators and punctuation—don't need this, so they are
/// represented directly by simple enum variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    Identifier(Symbol),
    Keyword(Keyword),

    IntLiteral(Symbol),
    FloatLiteral(Symbol),

    /// `&&`
    And,
    /// `||`
    Or,
    /// `-`
    Minus,
    /// `--`
    MinusMinus,
    /// `+`
    Plus,
    /// `++`
    PlusPlus,
    /// `=`
    Eq,
    /// `==`
    EqEq,
    /// `!`
    Bang,
    /// `!=`
    BangEq,
    /// `<`
    Lt,
    /// `<=`
    LtEq,
    /// `>`
    Gt,
    /// `>=`
    GtEq,
    /// `*`
    Star,
    /// `**`
    StarStar,
    /// `/`
    Slash,
    /// `^`
    Caret,
    /// `%`
    Percent,

    /// `;`
    Semi,
    /// `,`
    Comma,
    /// `.`
    Dot,
    /// `(`
    OpenParen,
    /// `)`
    CloseParen,
    /// `{`
    OpenBrace,
    /// `}`
    CloseBrace,
    /// `[`
    OpenBracket,
    /// `]`
    CloseBracket,

    Unexpected,

    /// End of file
    Eof,
}

impl TokenKind {
    /// Attempts to resolve the provided source text into a recognized reserved keyword.
    ///
    /// Returns `None` if the text does not correspond to a valid keyword.
    pub fn map_keyword(keyword: &str) -> Option<Self> {
        Keyword::from_str(keyword).ok().map(Self::Keyword)
    }

    /// Returns `true` if the token represents a reserved keyword.
    pub fn is_keyword(&self) -> bool {
        matches!(self, Self::Keyword(_))
    }
    /// Returns `true` if the token represents a boolean keyword (`true` or `false`).
    pub fn is_boolean(&self) -> bool {
        matches!(self, Self::Keyword(keyword) if keyword.is_boolean())
    }

    /// Interns the source text and constructs an identifier token.
    pub fn identifier(registry: &mut SymbolRegistry, value: &str) -> Self {
        Self::intern(registry, value, Self::Identifier)
    }

    /// Interns the source text and constructs an integer literal token.
    pub fn int_literal(registry: &mut SymbolRegistry, value: &str) -> Self {
        Self::intern(registry, value, Self::IntLiteral)
    }

    /// Interns the source text and constructs a floating-point literal token.
    pub fn float_literal(registry: &mut SymbolRegistry, value: &str) -> Self {
        Self::intern(registry, value, Self::FloatLiteral)
    }

    /// Interns the provided string slice into the registry and constructs the
    /// corresponding token variant using the supplied constructor.
    fn intern(registry: &mut SymbolRegistry, value: &str, constructor: fn(Symbol) -> Self) -> Self {
        let symbol = registry.intern(value);
        constructor(symbol)
    }
}

impl fmt::Display for TokenKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Identifier(_) => write!(f, "identifier"),
            Self::Keyword(kw) => write!(f, "{kw}"),
            Self::IntLiteral(_) => write!(f, "integer literal"),
            Self::FloatLiteral(_) => write!(f, "float literal"),

            Self::And => write!(f, "&&"),
            Self::Or => write!(f, "||"),
            Self::Minus => write!(f, "-"),
            Self::MinusMinus => write!(f, "--"),
            Self::Plus => write!(f, "+"),
            Self::PlusPlus => write!(f, "++"),
            Self::Eq => write!(f, "="),
            Self::EqEq => write!(f, "=="),
            Self::Bang => write!(f, "!"),
            Self::BangEq => write!(f, "!="),
            Self::Lt => write!(f, "<"),
            Self::LtEq => write!(f, "<="),
            Self::Gt => write!(f, ">"),
            Self::GtEq => write!(f, ">="),
            Self::Star => write!(f, "*"),
            Self::StarStar => write!(f, "**"),
            Self::Slash => write!(f, "/"),
            Self::Caret => write!(f, "^"),
            Self::Percent => write!(f, "%"),

            Self::Semi => write!(f, ";"),
            Self::Comma => write!(f, ","),
            Self::Dot => write!(f, "."),
            Self::OpenParen => write!(f, "("),
            Self::CloseParen => write!(f, ")"),
            Self::OpenBrace => write!(f, "{{"),
            Self::CloseBrace => write!(f, "}}"),
            Self::OpenBracket => write!(f, "["),
            Self::CloseBracket => write!(f, "]"),

            Self::Unexpected => write!(f, "unknown token"),
            Self::Eof => write!(f, "end of file"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn displays_brace_tokens() {
        assert_eq!(TokenKind::OpenBrace.to_string(), "{");
        assert_eq!(TokenKind::CloseBrace.to_string(), "}");
    }

    #[test]
    fn displays_keyword_tokens() {
        assert_eq!(TokenKind::Keyword(Keyword::Let).to_string(), "let");
        assert_eq!(TokenKind::Keyword(Keyword::Const).to_string(), "const");
        assert_eq!(TokenKind::Keyword(Keyword::True).to_string(), "true");
        assert_eq!(TokenKind::Keyword(Keyword::False).to_string(), "false");
    }
}
