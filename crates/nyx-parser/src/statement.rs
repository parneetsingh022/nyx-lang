// SPDX-License-Identifier: MIT OR Apache-2.0

use nyx_diagnostic::ParserError;
use nyx_source::Span;
use nyx_token::{Keyword, Token, TokenKind};

use crate::{
    Parser,
    ast::{Stmt, StmtKind},
    expression::is_expr_start,
};

impl<'a> Parser<'a> {
    pub fn parse_stmt(&mut self) -> Result<Stmt, ParserError> {
        let token = self.peek().ok_or_else(|| self.unexpected_eof_error())?;

        match token.kind {
            TokenKind::Keyword(_) => self.parse_keyword_stmt(token),
            TokenKind::OpenBrace => self.parse_block_stmt(),
            _ if is_expr_start(token) => self.parse_expr_stmt(),
            _ => Err(self.expected_statement_error(token)),
        }
    }

    fn parse_keyword_stmt(&mut self, token: Token) -> Result<Stmt, ParserError> {
        match token.kind {
            TokenKind::Keyword(Keyword::Let) => self.parse_let_stmt(),
            TokenKind::Keyword(Keyword::If) => self.parse_if_stmt(),
            TokenKind::Keyword(Keyword::Else) => Err(ParserError::ElseWithoutIf {
                at: token.span.into(),
                src: self.source_file.clone(),
            }),
            TokenKind::Keyword(_) => Err(self.expected_statement_error(token)),
            _ => unreachable!("non-keyword token passed to parse_keyword_stmt"),
        }
    }

    fn parse_expr_stmt(&mut self) -> Result<Stmt, ParserError> {
        let expr = self.parse_expr()?;
        let span = expr.span();

        self.ensure_no_adjacent_expression(expr.span())?;
        self.expect_semicolon()?;

        if !expr.is_valid_expr_statement() {
            return Err(ParserError::InvalidExpressionStatement {
                at: span.into(),
                src: self.source_file.clone(),
            });
        }

        Ok(Stmt::new(StmtKind::ExprStmt { expr }, span))
    }

    fn parse_let_stmt(&mut self) -> Result<Stmt, ParserError> {
        // Consume the keyword
        let start = self.expect(TokenKind::Keyword(Keyword::Let))?.span;
        let identifier = self.expect_identifier()?;
        self.expect(TokenKind::Eq)?;

        let expr = self.parse_expr()?;

        self.ensure_no_adjacent_expression(expr.span())?;

        let semi = self.expect_semicolon()?;

        let span = Span::from_bounds(start, semi.span);

        Ok(Stmt::new(
            StmtKind::Let {
                name: identifier,
                expr,
            },
            span,
        ))
    }

    fn parse_block_stmt(&mut self) -> Result<Stmt, ParserError> {
        let start = self.expect(TokenKind::OpenBrace)?.span;
        let mut stmts = Vec::new();

        while let Some(token) = self.peek() {
            if token.kind == TokenKind::CloseBrace {
                break;
            }

            stmts.push(self.parse_stmt()?);
        }

        let end = self.expect_closing(TokenKind::CloseBrace, start)?.span;

        let span = Span::from_bounds(start, end);

        Ok(Stmt::new(StmtKind::Block { stmts }, span))
    }

    fn parse_if_stmt(&mut self) -> Result<Stmt, ParserError> {
        let start = self.expect(TokenKind::Keyword(Keyword::If))?.span;

        let condition = self.parse_expr()?;
        let then_branch = Box::new(self.parse_block_stmt()?);
        let else_branch = self.parse_else_branch()?;

        let end = else_branch.as_deref().unwrap_or(&then_branch).span();

        let span = Span::from_bounds(start, end);

        Ok(Stmt::new(
            StmtKind::If {
                condition,
                then_branch,
                else_branch,
            },
            span,
        ))
    }

    fn parse_else_branch(&mut self) -> Result<Option<Box<Stmt>>, ParserError> {
        if !self.eat(TokenKind::Keyword(Keyword::Else)) {
            return Ok(None);
        }

        let Some(token) = self.peek() else {
            return Err(ParserError::ExpectedElseBranch {
                found: TokenKind::Eof,
                at: self.eof_span().into(),
                src: self.source_file.clone(),
            });
        };

        let branch = match token.kind {
            TokenKind::OpenBrace => self.parse_block_stmt()?,
            TokenKind::Keyword(Keyword::If) => self.parse_if_stmt()?,
            found => {
                return Err(ParserError::ExpectedElseBranch {
                    found,
                    at: token.span.into(),
                    src: self.source_file.clone(),
                });
            }
        };

        Ok(Some(Box::new(branch)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use nyx_lexer::Lexer;
    use nyx_source::SourceFile;
    use nyx_token::SymbolRegistry;
    use rstest::rstest;

    use crate::{
        ast::{BinaryOp, Expr, ExprKind, SpannedIdentifier, UnaryOp},
        expression::tests::{binary, boolean, float, identifier, int, unary},
    };

    mod support {
        use super::*;

        pub fn try_parse_stmt(source: &str) -> Result<(Stmt, SymbolRegistry), ParserError> {
            let source_file = SourceFile::new("main.nyx", source);
            let mut lexer = Lexer::new(source_file.clone());

            let tokens = lexer
                .by_ref()
                .collect::<Result<Vec<_>, _>>()
                .expect("expected lexing to succeed");

            let lexer_errors = lexer.take_errors();

            assert!(
                lexer_errors.is_empty(),
                "unexpected lexer errors for `{source}`: {lexer_errors:#?}"
            );

            let symbol_registry = lexer.take_registry();
            let mut parser = Parser::new(&tokens, &symbol_registry, source_file);

            parser.parse_stmt().map(|stmt| (stmt, symbol_registry))
        }

        pub fn parse_stmt(source: &str) -> (Stmt, SymbolRegistry) {
            try_parse_stmt(source).unwrap_or_else(|error| {
                panic!(
                    "expected `{source}` to parse successfully, \
                     found: {error:?}"
                )
            })
        }

        pub fn try_parse_all(source: &str) -> Result<Vec<Stmt>, ParserError> {
            let source_file = SourceFile::new("main.nyx", source);
            let mut lexer = Lexer::new(source_file.clone());

            let tokens = lexer
                .by_ref()
                .collect::<Result<Vec<_>, _>>()
                .expect("expected lexing to succeed");

            let lexer_errors = lexer.take_errors();

            assert!(
                lexer_errors.is_empty(),
                "unexpected lexer errors for `{source}`: {lexer_errors:#?}"
            );

            let symbol_registry = lexer.take_registry();
            let mut parser = Parser::new(&tokens, &symbol_registry, source_file);

            let mut statements = Vec::new();

            while parser.peek().is_some() {
                statements.push(parser.parse_stmt()?);
            }

            Ok(statements)
        }

        pub fn expect_let(stmt: &Stmt) -> (&SpannedIdentifier, &Expr) {
            match stmt.kind() {
                StmtKind::Let { name, expr } => (name, expr),
                kind => {
                    panic!("expected let statement, found: {kind:?}")
                }
            }
        }

        pub fn expect_expr_stmt(stmt: &Stmt) -> &Expr {
            match stmt.kind() {
                StmtKind::ExprStmt { expr } => expr,
                kind => {
                    panic!("expected expression statement, found: {kind:?}")
                }
            }
        }

        pub fn expect_block(stmt: &Stmt) -> &[Stmt] {
            match stmt.kind() {
                StmtKind::Block { stmts } => stmts,
                kind => panic!("expected block, found: {kind:?}"),
            }
        }

        pub fn expect_if(stmt: &Stmt) -> (&Expr, &Stmt, Option<&Stmt>) {
            match stmt.kind() {
                StmtKind::If {
                    condition,
                    then_branch,
                    else_branch,
                } => (condition, then_branch.as_ref(), else_branch.as_deref()),
                kind => {
                    panic!("expected if statement, found: {kind:?}")
                }
            }
        }

        pub fn whole_single_line_span(source: &str) -> Span {
            let end = u32::try_from(source.len())
                .expect("source file exceeds the maximum supported size");

            Span::new(0, end)
        }

        pub fn assert_missing_operator(source: &str) {
            let error = try_parse_stmt(source).expect_err("expected parsing to fail");

            assert!(
                matches!(error, ParserError::MissingOperator { .. }),
                "expected MissingOperator for `{source}`, \
                 found: {error:?}"
            );
        }

        pub fn assert_missing_semicolon(source: &str) {
            let error = try_parse_stmt(source).expect_err("expected parsing to fail");

            assert!(
                matches!(error, ParserError::ExpectedSemicolon { .. }),
                "expected ExpectedSemicolon for `{source}`, \
                 found: {error:?}"
            );
        }
    }

    mod statement_dispatch {
        use super::support::*;
        use super::*;

        #[test]
        fn rejects_empty_input() {
            let error = try_parse_stmt("").expect_err("expected empty input to fail");

            assert!(
                matches!(error, ParserError::UnexpectedEof { .. }),
                "expected UnexpectedEof, found: {error:?}"
            );
        }

        #[rstest]
        #[case("else { foo(); }")]
        #[case("else if true { foo(); }")]
        #[case("if true { foo(); } else { bar(); } else { baz(); }")]
        #[case(
            "if true { foo(); } else { bar(); } \
             else if false { baz(); }"
        )]
        fn reports_else_without_matching_if(#[case] source: &str) {
            let error = try_parse_all(source).expect_err("expected unmatched else to fail");

            assert!(
                matches!(error, ParserError::ElseWithoutIf { .. }),
                "expected ElseWithoutIf for `{source}`, \
                 found: {error:?}"
            );
        }
    }

    mod let_statements {
        use super::support::*;
        use super::*;

        mod parsing {
            use super::*;

            #[rstest]
            #[case("x", "42", int(42))]
            #[case("_underscore_variable", "204.2101", float(204.2101))]
            #[case("___boolean_true", "true", boolean(true))]
            #[case("___boolean_false___293", "false", boolean(false))]
            #[case("unary_variable", "-39", unary(UnaryOp::Minus, int(39)))]
            #[case("unary_bool___", "!false", unary(UnaryOp::Not, boolean(false)))]
            fn parses_literal_initializers(
                #[case] variable_name: &str,
                #[case] value: &str,
                #[case] expected: Expr,
            ) {
                let source = format!("let {variable_name} = {value};");

                let (stmt, symbol_registry) = parse_stmt(&source);

                let (name, expr) = expect_let(&stmt);

                assert_eq!(variable_name, symbol_registry.resolve(name.symbol()));

                assert_eq!(expr, &expected);
            }

            #[rstest]
            #[case("x", "y")]
            #[case("result", "_input")]
            #[case("same", "same")]
            fn parses_identifier_initializers(#[case] variable_name: &str, #[case] value: &str) {
                let source = format!("let {variable_name} = {value};");

                let (stmt, symbol_registry) = parse_stmt(&source);

                let (name, expr) = expect_let(&stmt);

                assert_eq!(variable_name, symbol_registry.resolve(name.symbol()));

                let ExprKind::Identifier(symbol) = expr.kind() else {
                    panic!(
                        "expected identifier initializer, \
                         found: {:?}",
                        expr.kind()
                    );
                };

                assert_eq!(value, symbol_registry.resolve(*symbol));
            }

            #[test]
            fn parses_mixed_expression_initializer() {
                let source = "let result = -(value + 42) * !false;";

                let (stmt, mut symbol_registry) = parse_stmt(source);

                let value_symbol = symbol_registry.intern("value");

                let expected = binary(
                    unary(
                        UnaryOp::Minus,
                        binary(identifier(value_symbol), BinaryOp::Plus, int(42)),
                    ),
                    BinaryOp::Multiply,
                    unary(UnaryOp::Not, boolean(false)),
                );

                let (name, expr) = expect_let(&stmt);

                assert_eq!("result", symbol_registry.resolve(name.symbol()));

                assert_eq!(expr, &expected);
            }

            #[rstest]
            #[case("let x = 5 + 6;")]
            #[case("let x = foo();")]
            #[case("let x = foo() + bar();")]
            #[case("let x = foo()(1);")]
            fn accepts_valid_initializer_expressions(#[case] source: &str) {
                try_parse_stmt(source).unwrap_or_else(|error| {
                    panic!(
                        "expected `{source}` to parse, \
                         found: {error:?}"
                    )
                });
            }

            #[rstest]
            #[case("let x = 5 \n + 6;")]
            #[case("let x = 10 + \n 20;")]
            #[case("let x = \n foo();")]
            #[case("let x = \n ( \n 5 + 6 \n );")]
            #[case("let x = foo() \n + \n bar();")]
            fn accepts_multiline_initializer_expressions(#[case] source: &str) {
                try_parse_stmt(source).unwrap_or_else(|error| {
                    panic!(
                        "expected multiline initializer \
                         `{source}` to parse, found: {error:?}"
                    )
                });
            }
        }

        mod diagnostics {
            use super::*;

            #[rstest]
            #[case("let = 42;")]
            #[case("let 42 = 42;")]
            #[case("let x 42;")]
            #[case("let x = ;")]
            fn rejects_incomplete_declarations(#[case] source: &str) {
                assert!(
                    try_parse_stmt(source).is_err(),
                    "expected `{source}` to fail"
                );
            }

            #[rstest]
            #[case("let x = 5 6;")]
            #[case("let x = value other;")]
            #[case("let x = foo() bar();")]
            #[case("let x = (1 + 2) 3;")]
            #[case("let x = 5 true;")]
            #[case("let x = -5 value;")]
            #[case("let x = 5    6;")]
            #[case("let x = foo()\tbar();")]
            fn reports_missing_operator_between_initializers(#[case] source: &str) {
                assert_missing_operator(source);
            }

            #[rstest]
            #[case("let x = 10")]
            #[case("let x = 10 * 39 - 19 + 29")]
            #[case("let x = 10\nlet y = 20;")]
            #[case("let x = 5\nfoo();")]
            #[case("let x = value\nother();")]
            #[case("let x = foo()\nbar();")]
            #[case("let x = (1 + 2)\nfoo();")]
            #[case("let x = 5\n\nfoo();")]
            fn reports_missing_semicolon(#[case] source: &str) {
                assert_missing_semicolon(source);
            }
        }

        mod spans {
            use super::*;

            #[test]
            fn statement_span_covers_entire_declaration() {
                let source = "let result = 42;";
                let (stmt, _) = parse_stmt(source);

                assert_eq!(stmt.span(), whole_single_line_span(source));
            }

            #[test]
            fn statement_span_starts_at_let_keyword() {
                let source = "\n  let result = 42;";
                let (stmt, _) = parse_stmt(source);

                assert_eq!(stmt.span(), Span::new(3, source.len() as u32));
            }

            #[test]
            fn declaration_identifier_has_correct_span() {
                let source = "let result = 42;";
                let (stmt, _) = parse_stmt(source);

                let (name, _) = expect_let(&stmt);

                assert_eq!(name.span(), Span::new(4, 10));
            }

            #[test]
            fn identifier_tracks_line_and_column() {
                let source = "\n  let long_name = 42;";
                let (stmt, _) = parse_stmt(source);

                let (name, _) = expect_let(&stmt);

                assert_eq!(name.span(), Span::new(7, 16));
            }

            #[test]
            fn declaration_and_value_have_independent_spans() {
                let source = "let destination = source;";

                let (stmt, symbol_registry) = parse_stmt(source);

                let (name, expr) = expect_let(&stmt);

                assert_eq!("destination", symbol_registry.resolve(name.symbol()));

                assert_eq!(name.span(), Span::new(4, 15));

                let ExprKind::Identifier(symbol) = expr.kind() else {
                    panic!(
                        "expected identifier expression, \
                         found: {:?}",
                        expr.kind()
                    );
                };

                assert_eq!("source", symbol_registry.resolve(*symbol));

                assert_eq!(expr.span(), Span::new(18, 24));
            }
        }
    }

    mod expression_statements {
        use super::support::*;
        use super::*;

        mod parsing {
            use super::*;

            #[rstest]
            #[case("foo();")]
            #[case("foo(1, 2);")]
            #[case("foo(bar(), baz());")]
            #[case("x = 42;")]
            #[case("x = y = 10;")]
            fn parses_valid_expression_statements(#[case] source: &str) {
                let (stmt, _) = parse_stmt(source);
                let _ = expect_expr_stmt(&stmt);
            }

            #[rstest]
            #[case("foo(\n bar(), \n baz() \n);")]
            #[case("x \n = \n 5;")]
            fn parses_multiline_expression_statements(#[case] source: &str) {
                let (stmt, _) = parse_stmt(source);
                let _ = expect_expr_stmt(&stmt);
            }
        }

        mod diagnostics {
            use super::*;

            #[rstest]
            #[case("foo() bar();")]
            #[case("x = 5 6;")]
            #[case("x = value other;")]
            #[case("x = foo() bar();")]
            #[case("y = (14 * 25) \n - 900 105;")]
            #[case("y = (14 * 25) - \n 900 105;")]
            fn reports_missing_operator(#[case] source: &str) {
                assert_missing_operator(source);
            }

            #[rstest]
            #[case("foo()")]
            #[case("foo(92, 49, age)")]
            #[case("x = 10")]
            #[case("foo(92, 49, age)\nlet y = 20;")]
            #[case("foo()\nlet y = 20;")]
            #[case("x = 10\nlet y = 20;")]
            #[case("foo()\nbar();")]
            #[case("x = 5\nfoo();")]
            #[case("foo()\n\nbar();")]
            #[case("x = 5\n\n\nfoo();")]
            fn reports_missing_semicolon(#[case] source: &str) {
                assert_missing_semicolon(source);
            }
        }
    }

    mod blocks {
        use super::support::*;
        use super::*;

        mod parsing {
            use super::*;

            #[test]
            fn parses_empty_block() {
                let (stmt, _) = parse_stmt("{}");

                assert!(expect_block(&stmt).is_empty());
            }

            #[test]
            fn parses_single_statement_block() {
                let source = "{ let x = 42; }";
                let (stmt, _) = parse_stmt(source);

                let statements = expect_block(&stmt);

                assert_eq!(statements.len(), 1);
                assert!(matches!(statements[0].kind(), StmtKind::Let { .. }));
            }

            #[test]
            fn parses_multiple_statement_block() {
                let source = "{ let x = 42; foo(); x = 10; }";

                let (stmt, _) = parse_stmt(source);
                let statements = expect_block(&stmt);

                assert_eq!(statements.len(), 3);

                assert!(matches!(statements[0].kind(), StmtKind::Let { .. }));

                assert!(matches!(statements[1].kind(), StmtKind::ExprStmt { .. }));

                assert!(matches!(statements[2].kind(), StmtKind::ExprStmt { .. }));
            }

            #[test]
            fn parses_nested_blocks() {
                let source = "{ { let x = 5; } foo(); }";

                let (stmt, _) = parse_stmt(source);
                let outer = expect_block(&stmt);

                assert_eq!(outer.len(), 2);

                let inner = expect_block(&outer[0]);

                assert_eq!(inner.len(), 1);
                assert!(matches!(inner[0].kind(), StmtKind::Let { .. }));

                assert!(matches!(outer[1].kind(), StmtKind::ExprStmt { .. }));
            }
        }

        mod diagnostics {
            use super::*;

            #[rstest]
            #[case("{")]
            #[case("{ let x = 5;")]
            #[case("{ foo(); { let y = 10; }")]
            fn reports_unclosed_block(#[case] source: &str) {
                let error = try_parse_stmt(source).expect_err("expected unclosed block to fail");

                assert!(
                    matches!(
                        error,
                        ParserError::UnclosedDelimiter { .. } | ParserError::UnexpectedEof { .. }
                    ),
                    "expected an unclosed-delimiter error for \
                     `{source}`, found: {error:?}"
                );
            }
        }

        mod spans {
            use super::*;

            #[test]
            fn single_line_span_includes_both_braces() {
                let source = "{ let x = 5; }";
                let (stmt, _) = parse_stmt(source);

                assert_eq!(stmt.span(), whole_single_line_span(source));
            }

            #[test]
            fn multiline_span_includes_both_braces() {
                let source = "{\n    let x = 5;\n}";
                let (stmt, _) = parse_stmt(source);

                assert_eq!(stmt.span().start(), 0);
                assert_eq!(stmt.span().end(), source.len() as u32);
            }
        }
    }

    mod if_statements {
        use super::support::*;
        use super::*;

        mod parsing {
            use super::*;

            #[test]
            fn parses_if_without_else() {
                let source = "if true { foo(); }";
                let (stmt, _) = parse_stmt(source);

                let (condition, then_branch, else_branch) = expect_if(&stmt);

                assert_eq!(condition, &boolean(true));
                assert!(else_branch.is_none());

                let then_statements = expect_block(then_branch);

                assert_eq!(then_statements.len(), 1);
                assert!(matches!(
                    then_statements[0].kind(),
                    StmtKind::ExprStmt { .. }
                ));
            }

            #[test]
            fn parses_if_else() {
                let source = "if true { foo(); } else { bar(); }";

                let (stmt, _) = parse_stmt(source);

                let (_, then_branch, else_branch) = expect_if(&stmt);

                assert_eq!(expect_block(then_branch).len(), 1);

                let else_branch = else_branch.expect("expected else branch");

                assert_eq!(expect_block(else_branch).len(), 1);
            }

            #[test]
            fn parses_else_if_chain() {
                let source = "if x { foo(); } else if y { bar(); }";

                let (stmt, symbol_registry) = parse_stmt(source);

                let (_, then_branch, else_branch) = expect_if(&stmt);

                assert_eq!(expect_block(then_branch).len(), 1);

                let else_if = else_branch.expect("expected else-if branch");

                let (inner_condition, inner_then, inner_else) = expect_if(else_if);

                let ExprKind::Identifier(symbol) = inner_condition.kind() else {
                    panic!(
                        "expected identifier condition, \
                         found: {:?}",
                        inner_condition.kind()
                    );
                };

                assert_eq!("y", symbol_registry.resolve(*symbol));

                assert_eq!(expect_block(inner_then).len(), 1);

                assert!(inner_else.is_none());
            }

            #[test]
            fn parses_else_if_else_chain() {
                let source = "if x { foo(); } \
                     else if y { bar(); } \
                     else { baz(); }";

                let (stmt, _) = parse_stmt(source);

                let (_, _, else_branch) = expect_if(&stmt);

                let else_if = else_branch.expect("expected else-if branch");

                let (_, _, final_else) = expect_if(else_if);

                let final_else = final_else.expect("expected final else branch");

                assert_eq!(expect_block(final_else).len(), 1);
            }
        }

        mod diagnostics {
            use super::*;

            #[test]
            fn rejects_missing_then_block() {
                let source = "if true foo();";

                let error = try_parse_stmt(source).expect_err("expected missing block to fail");

                assert!(
                    matches!(error, ParserError::ExpectedToken { .. }),
                    "expected ExpectedToken, found: {error:?}"
                );
            }

            #[rstest]
            #[case("if true {} else foo();")]
            #[case("if true {} else 5;")]
            fn rejects_invalid_else_branch(#[case] source: &str) {
                let error = try_parse_stmt(source).expect_err("expected invalid else to fail");

                assert!(
                    matches!(error, ParserError::ExpectedElseBranch { .. }),
                    "expected ExpectedElseBranch for `{source}`, \
                     found: {error:?}"
                );
            }

            #[test]
            fn reports_identifier_after_else() {
                let source = "if true {} else foo();";

                let error = try_parse_stmt(source).expect_err("expected invalid else to fail");

                assert!(
                    matches!(
                        error,
                        ParserError::ExpectedElseBranch {
                            found: TokenKind::Identifier(_),
                            ..
                        }
                    ),
                    "expected identifier in ExpectedElseBranch, \
                     found: {error:?}"
                );
            }

            #[test]
            fn reports_eof_after_else() {
                let source = "if true {} else";

                let error = try_parse_stmt(source).expect_err("expected missing else branch");

                assert!(
                    matches!(
                        error,
                        ParserError::ExpectedElseBranch {
                            found: TokenKind::Eof,
                            ..
                        }
                    ),
                    "expected EOF in ExpectedElseBranch, \
                     found: {error:?}"
                );
            }

            #[rstest]
            #[case("if true { foo(); ")]
            #[case("if true {} else { foo(); ")]
            fn reports_unclosed_branch_block(#[case] source: &str) {
                let error = try_parse_stmt(source).expect_err("expected unclosed block to fail");

                assert!(
                    matches!(
                        error,
                        ParserError::UnclosedDelimiter { .. } | ParserError::UnexpectedEof { .. }
                    ),
                    "expected unclosed delimiter for `{source}`, \
                     found: {error:?}"
                );
            }
        }

        mod spans {
            use super::*;

            #[rstest]
            #[case("if true { foo(); }")]
            #[case("if true { foo(); } else { bar(); }")]
            #[case(
                "if a { } else if b { } \
                 else if c { } else { }"
            )]
            fn span_covers_complete_statement(#[case] source: &str) {
                let (stmt, _) = parse_stmt(source);

                assert_eq!(stmt.span(), whole_single_line_span(source));
            }
        }
    }
}
