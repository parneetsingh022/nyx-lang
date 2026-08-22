// SPDX-License-Identifier: MIT OR Apache-2.0

use super::*;

use nyx_token::Keyword;
use rstest::rstest;

fn make_lexer(source: &str) -> Lexer {
    let source_file = SourceFile::new("main.nyx", source);
    Lexer::new(source_file)
}

fn next_token(lexer: &mut Lexer) -> Token {
    lexer
        .next()
        .expect("expected another token, but reached end of input")
        .expect("expected a token, but lexing failed")
}

fn assert_eof(lexer: &mut Lexer) {
    assert!(
        lexer.next().is_none(),
        "expected end of input, but another lexer result was produced"
    );
}

#[derive(Debug)]
enum ExpectedToken<'source> {
    Kind(TokenKind),
    Identifier(&'source str),
    Integer(&'source str),
    Float(&'source str),
}

impl ExpectedToken<'_> {
    fn assert_matches(&self, lexer: &Lexer, token: &Token, index: usize) {
        match (self, token.kind) {
            (Self::Kind(expected), actual) => {
                assert_eq!(actual, *expected, "token kind mismatch at index {index}");
            }

            (Self::Identifier(expected), TokenKind::Identifier(symbol))
            | (Self::Integer(expected), TokenKind::IntLiteral(symbol))
            | (Self::Float(expected), TokenKind::FloatLiteral(symbol)) => {
                assert_eq!(
                    lexer.symbol_registry.resolve(symbol),
                    *expected,
                    "token value mismatch at index {index}"
                );
            }

            (expected, actual) => {
                panic!("token mismatch at index {index}: expected {expected:?}, found {actual:?}");
            }
        }
    }
}

fn assert_tokens(source: &str, expected: &[ExpectedToken<'_>]) {
    let mut lexer = make_lexer(source);

    for (index, expected_token) in expected.iter().enumerate() {
        let token = next_token(&mut lexer);
        expected_token.assert_matches(&lexer, &token, index);
    }

    assert_eof(&mut lexer);
}

fn assert_spans(source: &str, expected: &[(ExpectedToken<'_>, Span)]) {
    let mut lexer = make_lexer(source);

    for (index, (expected_token, expected_span)) in expected.iter().enumerate() {
        let token = next_token(&mut lexer);

        expected_token.assert_matches(&lexer, &token, index);

        assert_eq!(
            token.span, *expected_span,
            "span mismatch at token index {index}"
        );
    }

    assert_eof(&mut lexer);
}

#[derive(Debug, Clone, Copy)]
enum ExpectedError {
    IncompleteFloat,
    InvalidNumericSuffix,
    UnexpectedCharacter,
}

impl ExpectedError {
    fn matches(self, error: &LexerError) -> bool {
        matches!(
            (self, error),
            (Self::IncompleteFloat, LexerError::IncompleteFloat { .. })
                | (
                    Self::InvalidNumericSuffix,
                    LexerError::InvalidNumericSuffix { .. }
                )
                | (Self::UnexpectedCharacter, LexerError::UnexpectedChar { .. })
        )
    }
}

fn assert_lexer_errors(source: &str, expected: &[ExpectedError]) {
    let mut lexer = make_lexer(source);

    // Exhaust the lexer so all recoverable errors are collected.
    lexer.by_ref().for_each(drop);

    let errors = lexer.take_errors();

    assert_eq!(
        errors.len(),
        expected.len(),
        "expected {} lexer error(s), found {}: {errors:#?}",
        expected.len(),
        errors.len(),
    );

    for (index, (error, expected_error)) in errors.iter().zip(expected).enumerate() {
        assert!(
            expected_error.matches(error),
            "error mismatch at index {index}: \
             expected {expected_error:?}, found {error:?}"
        );
    }
}

mod token_kinds {
    use super::*;

    #[test]
    fn ignores_whitespace_only_input() {
        let mut lexer = make_lexer("   \n\t\r\n  ");
        assert_eof(&mut lexer);
    }

    #[rstest]
    #[case("ident")]
    #[case("a")]
    #[case("Z")]
    #[case("underscore_ident")]
    #[case("_start_with_underscore")]
    #[case("ident123")]
    #[case("a_b_c_1_2_3")]
    #[case("__with_two_underscores")]
    fn recognizes_identifier(#[case] source: &str) {
        assert_tokens(source, &[ExpectedToken::Identifier(source)]);
    }

    #[rstest]
    #[case("let", Keyword::Let)]
    #[case("const", Keyword::Const)]
    #[case("true", Keyword::True)]
    #[case("false", Keyword::False)]
    fn recognizes_keyword(#[case] source: &str, #[case] keyword: Keyword) {
        assert_tokens(source, &[ExpectedToken::Kind(TokenKind::Keyword(keyword))]);
    }

    #[test]
    fn recognizes_integer_literals() {
        assert_tokens(
            "234 596 32 0",
            &[
                ExpectedToken::Integer("234"),
                ExpectedToken::Integer("596"),
                ExpectedToken::Integer("32"),
                ExpectedToken::Integer("0"),
            ],
        );
    }

    #[test]
    fn recognizes_float_literals() {
        assert_tokens(
            "234.49 4549.5239 32.39 0.0",
            &[
                ExpectedToken::Float("234.49"),
                ExpectedToken::Float("4549.5239"),
                ExpectedToken::Float("32.39"),
                ExpectedToken::Float("0.0"),
            ],
        );
    }

    #[test]
    fn recognizes_mixed_token_sequence() {
        assert_tokens(
            "let x 123 45.67 const",
            &[
                ExpectedToken::Kind(TokenKind::Keyword(Keyword::Let)),
                ExpectedToken::Identifier("x"),
                ExpectedToken::Integer("123"),
                ExpectedToken::Float("45.67"),
                ExpectedToken::Kind(TokenKind::Keyword(Keyword::Const)),
            ],
        );
    }

    #[rstest]
    #[case("+", TokenKind::Plus)]
    #[case("++", TokenKind::PlusPlus)]
    #[case("-", TokenKind::Minus)]
    #[case("--", TokenKind::MinusMinus)]
    #[case("*", TokenKind::Star)]
    #[case("**", TokenKind::StarStar)]
    #[case("/", TokenKind::Slash)]
    #[case("%", TokenKind::Percent)]
    #[case("^", TokenKind::Caret)]
    fn recognizes_arithmetic_operator(#[case] source: &str, #[case] expected: TokenKind) {
        assert_tokens(source, &[ExpectedToken::Kind(expected)]);
    }

    #[rstest]
    #[case("=", TokenKind::Eq)]
    #[case("==", TokenKind::EqEq)]
    #[case("!", TokenKind::Bang)]
    #[case("!=", TokenKind::BangEq)]
    #[case("<", TokenKind::Lt)]
    #[case("<=", TokenKind::LtEq)]
    #[case(">", TokenKind::Gt)]
    #[case(">=", TokenKind::GtEq)]
    fn recognizes_comparison_operator(#[case] source: &str, #[case] expected: TokenKind) {
        assert_tokens(source, &[ExpectedToken::Kind(expected)]);
    }

    #[test]
    fn excludes_single_line_comments() {
        let source = r#"
// leading comment
let x = 10; // trailing comment
// let ignored = 20;
print(x);
print(true);
print(false);
"#;

        assert_tokens(
            source,
            &[
                ExpectedToken::Kind(TokenKind::Keyword(Keyword::Let)),
                ExpectedToken::Identifier("x"),
                ExpectedToken::Kind(TokenKind::Eq),
                ExpectedToken::Integer("10"),
                ExpectedToken::Kind(TokenKind::Semi),
                ExpectedToken::Identifier("print"),
                ExpectedToken::Kind(TokenKind::OpenParen),
                ExpectedToken::Identifier("x"),
                ExpectedToken::Kind(TokenKind::CloseParen),
                ExpectedToken::Kind(TokenKind::Semi),
                ExpectedToken::Identifier("print"),
                ExpectedToken::Kind(TokenKind::OpenParen),
                ExpectedToken::Kind(TokenKind::Keyword(Keyword::True)),
                ExpectedToken::Kind(TokenKind::CloseParen),
                ExpectedToken::Kind(TokenKind::Semi),
                ExpectedToken::Identifier("print"),
                ExpectedToken::Kind(TokenKind::OpenParen),
                ExpectedToken::Kind(TokenKind::Keyword(Keyword::False)),
                ExpectedToken::Kind(TokenKind::CloseParen),
                ExpectedToken::Kind(TokenKind::Semi),
            ],
        );
    }
}

mod token_spans {
    use super::*;

    #[test]
    fn tracks_tokens_across_lines() {
        assert_spans(
            "let\n  x",
            &[
                (
                    ExpectedToken::Kind(TokenKind::Keyword(Keyword::Let)),
                    Span::new(0, 3),
                ),
                (ExpectedToken::Identifier("x"), Span::new(6, 7)),
            ],
        );
    }

    #[test]
    fn tracks_tokens_after_blank_lines() {
        assert_spans(
            "a\n\nb",
            &[
                (ExpectedToken::Identifier("a"), Span::new(0, 1)),
                (ExpectedToken::Identifier("b"), Span::new(3, 4)),
            ],
        );
    }

    #[test]
    fn tracks_span_after_leading_whitespace() {
        assert_spans(
            "  \n  hello",
            &[(ExpectedToken::Identifier("hello"), Span::new(5, 10))],
        );
    }

    #[test]
    fn tracks_span_after_tabs() {
        assert_spans(
            "\t\tabc",
            &[(ExpectedToken::Identifier("abc"), Span::new(2, 5))],
        );
    }

    #[test]
    fn treats_crlf_as_a_single_newline() {
        assert_spans(
            "a\r\nb",
            &[
                (ExpectedToken::Identifier("a"), Span::new(0, 1)),
                (ExpectedToken::Identifier("b"), Span::new(3, 4)),
            ],
        );
    }

    #[rstest]
    #[case("let", Span::new(0, 3))]
    #[case("    let", Span::new(4, 7))]
    #[case("\t\tlet", Span::new(2, 5))]
    #[case("\n  let", Span::new(3, 6))]
    fn tracks_columns_after_whitespace(#[case] source: &str, #[case] expected_span: Span) {
        assert_spans(
            source,
            &[(
                ExpectedToken::Kind(TokenKind::Keyword(Keyword::Let)),
                expected_span,
            )],
        );
    }

    #[rstest]
    #[case("++", TokenKind::PlusPlus)]
    #[case("**", TokenKind::StarStar)]
    #[case("--", TokenKind::MinusMinus)]
    #[case("==", TokenKind::EqEq)]
    #[case("!=", TokenKind::BangEq)]
    #[case("&&", TokenKind::And)]
    #[case("||", TokenKind::Or)]
    fn tracks_two_character_operator_span(#[case] source: &str, #[case] kind: TokenKind) {
        assert_spans(source, &[(ExpectedToken::Kind(kind), Span::new(0, 2))]);
    }

    #[rstest]
    #[case(";", TokenKind::Semi)]
    #[case(",", TokenKind::Comma)]
    #[case(".", TokenKind::Dot)]
    #[case("(", TokenKind::OpenParen)]
    #[case(")", TokenKind::CloseParen)]
    #[case("{", TokenKind::OpenBrace)]
    #[case("}", TokenKind::CloseBrace)]
    #[case("[", TokenKind::OpenBracket)]
    #[case("]", TokenKind::CloseBracket)]
    fn tracks_punctuation_span(#[case] source: &str, #[case] kind: TokenKind) {
        assert_spans(source, &[(ExpectedToken::Kind(kind), Span::new(0, 1))]);
    }

    #[test]
    fn tracks_spans_in_mixed_expression() {
        assert_spans(
            "let x = (1 + [2 ** 3]);",
            &[
                (
                    ExpectedToken::Kind(TokenKind::Keyword(Keyword::Let)),
                    Span::new(0, 3),
                ),
                (ExpectedToken::Identifier("x"), Span::new(4, 5)),
                (ExpectedToken::Kind(TokenKind::Eq), Span::new(6, 7)),
                (ExpectedToken::Kind(TokenKind::OpenParen), Span::new(8, 9)),
                (ExpectedToken::Integer("1"), Span::new(9, 10)),
                (ExpectedToken::Kind(TokenKind::Plus), Span::new(11, 12)),
                (
                    ExpectedToken::Kind(TokenKind::OpenBracket),
                    Span::new(13, 14),
                ),
                (ExpectedToken::Integer("2"), Span::new(14, 15)),
                (ExpectedToken::Kind(TokenKind::StarStar), Span::new(16, 18)),
                (ExpectedToken::Integer("3"), Span::new(19, 20)),
                (
                    ExpectedToken::Kind(TokenKind::CloseBracket),
                    Span::new(20, 21),
                ),
                (
                    ExpectedToken::Kind(TokenKind::CloseParen),
                    Span::new(21, 22),
                ),
                (ExpectedToken::Kind(TokenKind::Semi), Span::new(22, 23)),
            ],
        );
    }

    #[test]
    fn tracks_spans_after_single_line_comment() {
        assert_spans(
            "// comment\nlet x = 10;",
            &[
                (
                    ExpectedToken::Kind(TokenKind::Keyword(Keyword::Let)),
                    Span::new(11, 14),
                ),
                (ExpectedToken::Identifier("x"), Span::new(15, 16)),
                (ExpectedToken::Kind(TokenKind::Eq), Span::new(17, 18)),
                (ExpectedToken::Integer("10"), Span::new(19, 21)),
                (ExpectedToken::Kind(TokenKind::Semi), Span::new(21, 22)),
            ],
        );
    }
}

mod errors {
    use super::*;

    #[rstest]
    #[case(
        "395.",
        &[ExpectedError::IncompleteFloat]
    )]
    #[case(
        "395. 123.",
        &[
            ExpectedError::IncompleteFloat,
            ExpectedError::IncompleteFloat,
        ]
    )]
    #[case(
        "423. 34",
        &[ExpectedError::IncompleteFloat]
    )]
    fn reports_incomplete_float_literals(#[case] source: &str, #[case] expected: &[ExpectedError]) {
        assert_lexer_errors(source, expected);
    }

    #[test]
    fn reports_invalid_numeric_suffixes() {
        assert_lexer_errors(
            "395abc 492.4adb",
            &[
                ExpectedError::InvalidNumericSuffix,
                ExpectedError::InvalidNumericSuffix,
            ],
        );
    }

    #[rstest]
    #[case("@")]
    #[case("#")]
    #[case("$")]
    #[case("§")]
    #[case("name@unexp")]
    #[case("@name")]
    fn reports_unexpected_character(#[case] source: &str) {
        assert_lexer_errors(source, &[ExpectedError::UnexpectedCharacter]);
    }
}
