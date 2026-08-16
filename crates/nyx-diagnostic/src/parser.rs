// SPDX-License-Identifier: MIT OR Apache-2.0

use miette::SourceSpan;

use nyx_source::SourceFile;
use nyx_token::TokenKind;

#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub enum ParserError {
    #[error("unexpected end of file")]
    #[diagnostic(
        code(nyx::parser::unexpected_eof),
        help("check for unclosed delimiters, incomplete expressions, or trailing operators")
    )]
    UnexpectedEof {
        #[label("unexpected end of file here")]
        at: SourceSpan,

        #[source_code]
        src: SourceFile,
    },

    #[error("expected `{expected}`, found `{found}`")]
    #[diagnostic(
        code(nyx::parser::expected_token),
        help("insert the missing token or remove the unexpected one")
    )]
    ExpectedToken {
        expected: TokenKind,
        found: TokenKind,

        #[label("expected `{expected}` after this")]
        at: SourceSpan,

        #[source_code]
        src: SourceFile,
    },

    #[error("expected an identifier, found `{found}`")]
    #[diagnostic(
        code(nyx::parser::expected_identifier),
        help("provide a valid identifier")
    )]
    ExpectedIdentifier {
        found: TokenKind,

        #[label("expected an identifier here")]
        at: SourceSpan,

        #[source_code]
        src: SourceFile,
    },

    #[error("expected an expression, found `{found}`")]
    #[diagnostic(
        code(nyx::parser::expected_expression),
        help("remove the unexpected token or provide the missing expression")
    )]
    ExpectedExpression {
        found: TokenKind,

        #[label("expected an expression here")]
        at: SourceSpan,

        #[source_code]
        src: SourceFile,
    },

    #[error("expected a statement, found `{found}`")]
    #[diagnostic(
        code(nyx::parser::expected_statement),
        help("remove the unexpected token or begin a valid statement")
    )]
    ExpectedStatement {
        found: TokenKind,

        #[label("expected a statement here")]
        at: SourceSpan,

        #[source_code]
        src: SourceFile,
    },

    #[error("expected ';'")]
    #[diagnostic(
        code(nyx::parser::expected_semicolon),
        help("add a semicolon `;` to terminate the statement")
    )]
    ExpectedSemicolon {
        #[label("expected ';' here")]
        at: SourceSpan,

        #[source_code]
        src: SourceFile,
    },

    #[error("unclosed delimiter")]
    #[diagnostic(
        code(nyx::parser::unclosed_delimiter),
        help("insert a `{expected}` to close this group")
    )]
    UnclosedDelimiter {
        expected: TokenKind,

        #[label("expected `{expected}` to close this")]
        opened_at: SourceSpan,

        #[source_code]
        src: SourceFile,
    },

    #[error("missing operator between expressions")]
    #[diagnostic(
        code(nyx::parser::missing_operator),
        help("use an operator (like `*`, `+`, etc.) between these values.")
    )]
    MissingOperator {
        #[label("this expression...")]
        left_span: SourceSpan,

        #[label("...is followed directly by this, but needs an operator between them")]
        right_span: SourceSpan,

        #[source_code]
        src: SourceFile,
    },

    /// Raised when an expression is used as a statement even though its result
    /// is not meaningful when ignored.
    #[error("expression result is unused")]
    #[diagnostic(
        code(nyx::parser::invalid_expression_statement),
        help("use the expression as part of another expression, assign its result, or remove it")
    )]
    InvalidExpressionStatement {
        #[label("the result of this expression is ignored")]
        at: SourceSpan,

        #[source_code]
        src: SourceFile,
    },

    #[error("expected a block or `if` after `else`, found `{found}`")]
    #[diagnostic(
        code(nyx::parser::expected_else_branch),
        help("follow `else` with either a block or another `if` statement")
    )]
    ExpectedElseBranch {
        found: TokenKind,

        #[label("expected `{{` or `if` here")]
        at: SourceSpan,

        #[source_code]
        src: SourceFile,
    },

    /// Raised when an `else` branch appears without a preceding `if`.
    #[error("`else` without a matching `if`")]
    #[diagnostic(
        code(nyx::parser::else_without_if),
        help("remove this `else` or place it after an `if` statement")
    )]
    ElseWithoutIf {
        #[label("this `else` has no matching `if`")]
        at: SourceSpan,

        #[source_code]
        src: SourceFile,
    },
}
