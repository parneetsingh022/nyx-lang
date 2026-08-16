// SPDX-License-Identifier: MIT OR Apache-2.0

use miette::SourceSpan;

use nyx_source::SourceFile;

#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub enum LexerError {
    #[error("unexpected character `{char}`")]
    #[diagnostic(
        code(nyx::lexer::unexpected_char),
        help("remove the character or replace it with valid syntax")
    )]
    UnexpectedChar {
        char: char,

        #[label("this character is not recognized")]
        at: SourceSpan,

        #[source_code]
        src: SourceFile,
    },

    #[error("incomplete floating-point literal")]
    #[diagnostic(
        code(nyx::lexer::incomplete_float),
        help("add a fractional component to the floating-point literal")
    )]
    IncompleteFloat {
        value: String,

        #[label("this literal is missing a fractional part")]
        at: SourceSpan,

        #[label("consider writing `{value}.0`")]
        suggestion: SourceSpan,

        #[source_code]
        src: SourceFile,
    },

    #[error("invalid numeric literal")]
    #[diagnostic(
        code(nyx::lexer::invalid_numeric_suffix),
        help("add whitespace or an operator between the number and the identifier")
    )]
    InvalidNumericSuffix {
        #[label("a number cannot be directly followed by identifier characters")]
        at: SourceSpan,

        #[source_code]
        src: SourceFile,
    },

    #[error("unterminated multi-line comment")]
    #[diagnostic(
        code(nyx::lexer::unterminated_comment),
        help("close the comment with `*/`")
    )]
    UnterminatedComment {
        #[label("this comment is never closed")]
        at: SourceSpan,

        #[source_code]
        src: SourceFile,
    },
}
