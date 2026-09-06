// SPDX-License-Identifier: MIT OR Apache-2.0

use nyx_diagnostic::ParserError;
use nyx_source::Span;
use nyx_token::{Keyword, Symbol, Token, TokenKind};

use crate::{
    Parser,
    ast::{BinaryOp, Expr, ExprKind, UnaryOp},
};

/// Determines whether a given [`TokenKind`] represents a valid unary (prefix) operator.
///
/// The parser uses this check to identify tokens that can legitimately start
/// an expression during the null denotation (`nud`) step.
fn is_unary_operator(token_kind: TokenKind) -> bool {
    matches!(token_kind, TokenKind::Minus | TokenKind::Bang)
}

/// Identifies whether a [`TokenKind`] represents an atomic, irreducible primary expression.
///
/// In the Pratt parser, primary expressions—such as variables, literal constants,
/// and explicitly grouped sub-expressions—form the foundational leaf nodes of the AST.
fn is_primary_expr_start(token_kind: TokenKind) -> bool {
    matches!(
        token_kind,
        TokenKind::Identifier(_)
            | TokenKind::IntLiteral(_)
            | TokenKind::FloatLiteral(_)
            | TokenKind::OpenParen
            // A boolean "true" or "false" can also start an expression
            | TokenKind::Keyword(Keyword::True | Keyword::False)
    )
}

/// Checks if the token can start an expression.
pub(crate) fn is_expr_start(token: Token) -> bool {
    let kind = token.kind;
    is_unary_operator(kind) | is_primary_expr_start(kind)
}

impl<'a> Parser<'a> {
    /// Parses an [`Expr`] starting at the current token.
    ///
    /// This is the main entry point for expression parsing. It starts the Pratt
    /// parser with the lowest binding power so that the complete [`Expr`] can
    /// be parsed.
    pub fn parse_expr(&mut self) -> Result<Expr, ParserError> {
        self.parse_bp(0)
    }

    /// Ensures that a completed statement expression is not immediately followed
    /// by another expression on the same line.
    ///
    /// This check is used after parsing expressions that must end before a statement
    /// terminator, such as the initializer in a let statement or a standalone
    /// expression statement.
    ///
    /// For example, in let x = 5 6;, the parser successfully parses 5 as one
    /// expression, but the following 6 indicates that an operator is probably
    /// missing between them. The second expression is parsed so its span can be
    /// included in a [MissingOperatorError].
    ///
    /// This validation is intentionally not performed by [Parser::parse_expr].
    /// Expression parsing is also used inside contexts such as function argument
    /// lists, where an adjacent expression may instead indicate a missing comma.
    /// The surrounding grammar rule is responsible for deciding what tokens are
    /// valid after an expression.
    ///
    /// Expressions beginning on a different line are left untouched so the
    /// statement parser can report a more appropriate error, such as a missing
    /// semicolon.
    pub(super) fn ensure_no_adjacent_expression(&mut self, left: Span) -> Result<(), ParserError> {
        let Some(token) = self.peek() else {
            return Ok(());
        };

        if !is_expr_start(token) {
            return Ok(());
        }

        // A new expression on another line is more likely the beginning of the
        // next statement. Let `expect_semicolon` report the missing terminator.
        if !self
            .source_file
            .is_same_line(left.end(), token.span.start())
        {
            return Ok(());
        }

        let right = self.parse_expr()?;

        Err(ParserError::MissingOperator {
            left_span: left.into(),
            right_span: right.span().into(),
            src: self.source_file.clone(),
        })
    }

    /// Parses an [`Expr`] using Pratt parsing.
    ///
    /// `min_bp` is the minimum binding power an operator must have to become part
    /// of the current expression. Operators with lower binding power are left for
    /// the caller to parse.
    fn parse_bp(&mut self, min_bp: u8) -> Result<Expr, ParserError> {
        let mut lhs = self.parse_prefix_expression()?;

        loop {
            // Function calls bind more tightly than binary operators.
            if self.check(TokenKind::OpenParen) {
                lhs = self.parse_call_expression(lhs)?;
                continue;
            }

            let Some(op) = self.peek().and_then(BinaryOp::from_token) else {
                break;
            };

            let (left_bp, right_bp) = op.binding_power();

            if left_bp < min_bp {
                break;
            }

            self.advance();

            lhs = self.parse_binary_expression(lhs, op, right_bp)?;
        }

        Ok(lhs)
    }

    /// Parses an [`Expr`] that begins at the current token.
    ///
    /// This is the null denotation (nud) step in Pratt parsing. It handles tokens
    /// that can start an expression, such as integer literals, identifiers, prefix
    /// operators, and parenthesized expressions.
    ///
    /// For example, when parsing 1 + 2, this method first parses 1 before the
    /// parser continues with the + operator.
    fn parse_prefix_expression(&mut self) -> Result<Expr, ParserError> {
        let (kind, span) = self
            .advance()
            .map(|token| (token.kind, token.span))
            .ok_or_else(|| self.unexpected_eof_error())?;

        let expr = match kind {
            TokenKind::IntLiteral(symbol) => self.parse_integer_literal(symbol, span)?,
            TokenKind::FloatLiteral(symbol) => self.parse_float_literal(symbol, span)?,
            TokenKind::Identifier(symbol) => Expr::new(ExprKind::Identifier(symbol), span),
            TokenKind::OpenParen => self.parse_grouped_expression(span)?,
            _ if kind.is_boolean() => self.parse_boolean(kind, span),
            _ if is_unary_operator(kind) => self.parse_unary_expr()?,
            _ => {
                return Err(ParserError::ExpectedExpression {
                    at: span.into(),
                    src: self.source_file.clone(),
                    found: kind,
                });
            }
        };

        Ok(expr)
    }

    /// Parses a unary prefix expression.
    ///
    /// This method is called as part of the null denotation (nud) step in
    /// Pratt parsing. It expects the caller to have already consumed the
    /// prefix operator token. It dynamically fetches the operator's binding
    /// power and parses the right-hand operand accordingly.
    fn parse_unary_expr(&mut self) -> Result<Expr, ParserError> {
        // `parse_unary` is exclusively called by `parse_prefix_expression`
        // immediately after advancing past a validated prefix operator.
        // Therefore, `self.previous()` is guaranteed to exist, and
        // `UnaryOp::from_token` is guaranteed to succeed. A panic here
        // indicates a parser bug.
        let op = self
            .previous()
            .expect("parse_unary called without advancing the parser first");

        let start = op.span;
        let unary_op = UnaryOp::from_token(op).unwrap_or_else(|| {
            panic!(
                "parse_unary expected prefix operator, but found {:?}",
                op.kind
            )
        });

        let right_bp = unary_op.binding_power();
        let expr = self.parse_bp(right_bp)?;

        let span = Span::from_bounds(start, expr.span());

        Ok(Expr::new(
            ExprKind::Unary {
                op: unary_op,
                expr: Box::new(expr),
            },
            span,
        ))
    }

    /// Parses the right-hand side of a binary [`Expr`]  and combines it with the
    /// previously parsed left-hand side.
    ///
    /// This corresponds to the left denotation (`led`) step in Pratt parsing.
    /// The right-hand expression is parsed using `right_bp`, which controls
    /// precedence and associativity.
    ///
    /// For example, after parsing `1` and consuming `+` in `1 + 2`, this method
    /// parses `2` and produces the Expr `1 + 2`.
    fn parse_binary_expression(
        &mut self,
        lhs: Expr,
        op: BinaryOp,
        right_bp: u8,
    ) -> Result<Expr, ParserError> {
        let rhs = self.parse_bp(right_bp)?;
        let span = Span::from_bounds(lhs.span(), rhs.span());

        Ok(Expr::new(
            ExprKind::Binary {
                left: Box::new(lhs),
                op,
                right: Box::new(rhs),
            },
            span,
        ))
    }

    /// Parses an [`Expr`]  enclosed in parentheses.
    ///
    /// The opening ( is consumed by the caller before this method is invoked.
    /// This method parses the expression inside the parentheses and then requires
    /// a matching closing ).
    fn parse_grouped_expression(&mut self, open_paren_span: Span) -> Result<Expr, ParserError> {
        let mut expr = self.parse_expr()?;

        let close_paren_span = self
            .expect_closing(TokenKind::CloseParen, open_paren_span)?
            .span;

        // Grouping parentheses are omitted from the AST, but the expression span
        // still covers them so diagnostics can reference the full source range.
        let span = Span::from_bounds(open_paren_span, close_paren_span);
        expr.set_span(span);

        Ok(expr)
    }

    /// Parses a function call whose callee has already been parsed.
    ///
    /// For example, after parsing `foo` in `foo(1, 2)`, this method parses the
    /// argument list and produces a call [`Expr`]  with `foo` as its callee.
    fn parse_call_expression(&mut self, callee: Expr) -> Result<Expr, ParserError> {
        let open_paren = self.expect(TokenKind::OpenParen)?;
        let open_paren_span = open_paren.span;

        let mut arguments = Vec::new();

        if !self.check(TokenKind::CloseParen) {
            loop {
                arguments.push(self.parse_expr()?);

                if self.check(TokenKind::Comma) {
                    self.advance();
                    continue;
                }

                if self.check(TokenKind::CloseParen) {
                    break;
                }

                // We are missing either a `,` or a `)`.
                // Check if the current token could be the start of a new expression.
                if self.peek().is_some_and(is_expr_start) {
                    // Example: `hello(29, 39 11)`
                    // The `11` is an expression start. Force a missing comma error here.
                    self.expect(TokenKind::Comma)?;
                } else {
                    // Example: `call(a, b \n` or `call(a, b }`
                    // The next token (Newline, EOF, etc.) does NOT start an expression.
                    // Break the loop so the final `expect` below throws the missing `)` error.
                    break;
                }
            }
        }

        let close_paren_span = self
            .expect_closing(TokenKind::CloseParen, open_paren_span)?
            .span;

        let span = Span::from_bounds(callee.span(), close_paren_span);

        Ok(Expr::new(
            ExprKind::Call {
                callee: Box::new(callee),
                arguments,
            },
            span,
        ))
    }

    /// Parses a boolean keyword token into a boolean [`Expr`].
    ///
    /// `kind` must be either [`TokenKind::Keyword`] containing
    /// [`Keyword::True`] or [`Keyword::False`]. Any other token kind indicates
    /// a parser bug and causes this function to panic.
    fn parse_boolean(&self, kind: TokenKind, span: Span) -> Expr {
        let value = match kind {
            TokenKind::Keyword(Keyword::True) => true,
            TokenKind::Keyword(Keyword::False) => false,
            unexpected => {
                unreachable!("parse_boolean called with non-boolean token: {unexpected:?}")
            }
        };

        Expr::new(ExprKind::Bool(value), span)
    }

    /// Parses an interned integer literal into an integer [`Expr`].
    ///
    /// The literal text is resolved through the symbol registry and converted to
    /// an [`i64`].
    ///
    /// # Errors
    ///
    /// Returns a [`ParserError::NumberOutOfBounds`] if the parsed integer
    /// exceeds the maximum or minimum bounds of a 64-bit signed integer.
    fn parse_integer_literal(&self, symbol: Symbol, span: Span) -> Result<Expr, ParserError> {
        let text = self.symbol_registry.resolve(symbol);

        let value = text
            .parse::<i64>()
            .map_err(|err| ParserError::NumberOutOfBounds {
                at: span,
                src: self.source_file.clone(),
                message: format!("Invalid integer: {}", err),
            })?;

        Ok(Expr::new(ExprKind::IntLiteral(value), span))
    }

    /// Parses an interned floating-point literal into a floating-point [`Expr`].
    ///
    /// The literal text is resolved through the symbol registry and converted to
    /// an [`f64`].
    ///
    /// # Errors
    ///
    /// Returns a [`ParserError::NumberOutOfBounds`] if the parsed float
    /// is too large and overflows to infinity.
    ///
    /// # Panics
    ///
    /// Panics if the lexer produced a malformed float token (e.g. invalid characters).
    /// If the lexer is implemented correctly, this will never happen.
    fn parse_float_literal(&self, symbol: Symbol, span: Span) -> Result<Expr, ParserError> {
        let text = self.symbol_registry.resolve(symbol);

        // Since the lexer guarantees the string is correctly formatted (e.g., digits and one decimal),
        // we can safely use expect() here. The only "error" is if the lexer is bugged.
        let value = text
            .parse::<f64>()
            .expect("Lexer produced an invalidly formatted float literal");

        // Manually trigger the out-of-bounds error if the number evaluated to infinity
        if value.is_infinite() {
            return Err(ParserError::NumberOutOfBounds {
                at: span,
                src: self.source_file.clone(),
                message: "Float literal is too large and overflows to infinity".to_string(),
            });
        }

        Ok(Expr::new(ExprKind::FloatLiteral(value), span))
    }
}

#[cfg(test)]
pub(crate) mod tests {

    use super::*;
    use nyx_lexer::Lexer;
    use nyx_source::SourceFile;
    use rstest::rstest;

    pub fn make_lexer(code: &str) -> (Lexer, SourceFile) {
        let source_file: SourceFile = SourceFile::new("main.nyx", code);
        (Lexer::new(source_file.clone()), source_file)
    }

    pub(crate) fn int(value: i64) -> Expr {
        Expr::new(ExprKind::IntLiteral(value), Span::default())
    }

    pub(crate) fn float(value: f64) -> Expr {
        Expr::new(ExprKind::FloatLiteral(value), Span::default())
    }

    pub(crate) fn boolean(value: bool) -> Expr {
        Expr::new(ExprKind::Bool(value), Span::default())
    }

    pub(crate) fn identifier(symbol: Symbol) -> Expr {
        Expr::new(ExprKind::Identifier(symbol), Span::default())
    }

    pub(crate) fn binary(left: Expr, op: BinaryOp, right: Expr) -> Expr {
        Expr::new(
            ExprKind::Binary {
                left: Box::new(left),
                op,
                right: Box::new(right),
            },
            Span::default(),
        )
    }

    pub(crate) fn unary(op: UnaryOp, expr: Expr) -> Expr {
        Expr::new(
            ExprKind::Unary {
                op,
                expr: Box::new(expr),
            },
            Span::default(),
        )
    }

    fn parse_expression(source: &str) -> Expr {
        let (mut lexer, source_file) = make_lexer(source);

        let tokens = lexer.by_ref().map(|tok| tok.unwrap()).collect::<Vec<_>>();

        let mut parser = Parser::new(&tokens, &lexer.symbol_registry, source_file);

        parser.parse_expr().unwrap()
    }

    fn parse_expression_error(source: &str) -> ParserError {
        let (mut lexer, source_file) = make_lexer(source);

        let tokens = lexer
            .by_ref()
            .map(|token| token.unwrap())
            .collect::<Vec<_>>();

        let mut parser = Parser::new(&tokens, &lexer.symbol_registry, source_file);

        parser
            .parse_expr()
            .expect_err("expected expression parsing to fail")
    }

    #[rstest]
    #[case("42", int(42))]
    #[case("0", int(0))]
    #[allow(clippy::approx_constant)]
    #[case("3.14", float(3.14))]
    #[case("0.5", float(0.5))]
    fn parses_literals(#[case] source: &str, #[case] expected: Expr) {
        assert_eq!(parse_expression(source), expected);
    }

    #[rstest]
    #[case("true", boolean(true))]
    #[case("false", boolean(false))]
    fn parses_boolean_literals(#[case] source: &str, #[case] expected: Expr) {
        assert_eq!(parse_expression(source), expected);
    }

    #[rstest]
    #[case("1 + 2", BinaryOp::Plus)]
    #[case("1 - 2", BinaryOp::Minus)]
    #[case("1 * 2", BinaryOp::Multiply)]
    #[case("1 / 2", BinaryOp::Divide)]
    #[case("1 ** 2", BinaryOp::Power)]
    fn parses_binary_operators(#[case] source: &str, #[case] op: BinaryOp) {
        let expected = binary(int(1), op, int(2));

        assert_eq!(parse_expression(source), expected);
    }

    #[rstest]
    #[case(
        "1 + 2 * 3",
        binary(int(1), BinaryOp::Plus, binary(int(2), BinaryOp::Multiply, int(3)),)
    )]
    #[case(
        "1 * 2 + 3",
        binary(binary(int(1), BinaryOp::Multiply, int(2)), BinaryOp::Plus, int(3),)
    )]
    #[case(
        "10 - 8 / 2",
        binary(int(10), BinaryOp::Minus, binary(int(8), BinaryOp::Divide, int(2)),)
    )]
    #[case(
        "8 / 2 - 1",
        binary(binary(int(8), BinaryOp::Divide, int(2)), BinaryOp::Minus, int(1),)
    )]
    fn factor_operators_bind_tighter_than_term_operators(
        #[case] source: &str,
        #[case] expected: Expr,
    ) {
        assert_eq!(parse_expression(source), expected);
    }

    #[rstest]
    #[case(
        "1 + 2 + 3",
        binary(binary(int(1), BinaryOp::Plus, int(2)), BinaryOp::Plus, int(3),)
    )]
    #[case(
        "10 - 5 - 2",
        binary(binary(int(10), BinaryOp::Minus, int(5)), BinaryOp::Minus, int(2),)
    )]
    #[case(
        "2 * 3 * 4",
        binary(binary(int(2), BinaryOp::Multiply, int(3)), BinaryOp::Multiply, int(4),)
    )]
    #[case(
        "16 / 4 / 2",
        binary(binary(int(16), BinaryOp::Divide, int(4)), BinaryOp::Divide, int(2),)
    )]
    fn arithmetic_operators_are_left_associative(#[case] source: &str, #[case] expected: Expr) {
        assert_eq!(parse_expression(source), expected);
    }

    #[rstest]
    #[case(
        "2 ** 3 ** 4",
        binary(int(2), BinaryOp::Power, binary(int(3), BinaryOp::Power, int(4)),)
    )]
    #[case(
        "2 ** 3 ** 4 ** 5",
        binary(
            int(2),
            BinaryOp::Power,
            binary(int(3), BinaryOp::Power, binary(int(4), BinaryOp::Power, int(5)),),
        )
    )]
    fn power_is_right_associative(#[case] source: &str, #[case] expected: Expr) {
        assert_eq!(parse_expression(source), expected);
    }

    #[test]
    fn grouped_expression_overrides_precedence() {
        let expr = parse_expression("(1 + 2) * 3");

        let expected = binary(
            binary(int(1), BinaryOp::Plus, int(2)),
            BinaryOp::Multiply,
            int(3),
        );

        assert_eq!(expr, expected);
    }

    #[test]
    fn grouped_power_overrides_right_associativity() {
        let expr = parse_expression("(2 ** 3) ** 4");

        let expected = binary(
            binary(int(2), BinaryOp::Power, int(3)),
            BinaryOp::Power,
            int(4),
        );

        assert_eq!(expr, expected);
    }

    #[rstest]
    #[case(
        "2 * 3 ** 4",
        binary(int(2), BinaryOp::Multiply, binary(int(3), BinaryOp::Power, int(4)),)
    )]
    #[case(
        "2 ** 3 * 4",
        binary(binary(int(2), BinaryOp::Power, int(3)), BinaryOp::Multiply, int(4),)
    )]
    #[case(
        "8 / 2 ** 3",
        binary(int(8), BinaryOp::Divide, binary(int(2), BinaryOp::Power, int(3)),)
    )]
    #[case(
        "8 ** 2 / 4",
        binary(binary(int(8), BinaryOp::Power, int(2)), BinaryOp::Divide, int(4),)
    )]
    fn power_binds_tighter_than_factor_operators(#[case] source: &str, #[case] expected: Expr) {
        assert_eq!(parse_expression(source), expected);
    }

    #[rstest]
    #[case(
        "1 + 2 ** 3",
        binary(int(1), BinaryOp::Plus, binary(int(2), BinaryOp::Power, int(3)),)
    )]
    #[case(
        "2 ** 3 + 4",
        binary(binary(int(2), BinaryOp::Power, int(3)), BinaryOp::Plus, int(4),)
    )]
    #[case(
        "10 - 2 ** 3",
        binary(int(10), BinaryOp::Minus, binary(int(2), BinaryOp::Power, int(3)),)
    )]
    fn power_binds_tighter_than_term_operators(#[case] source: &str, #[case] expected: Expr) {
        assert_eq!(parse_expression(source), expected);
    }

    #[test]
    fn power_binds_tighter_than_unary_minus() {
        // -2 ** 3 => -(2 ** 3)
        let expr = parse_expression("-2 ** 3");

        let expected = unary(UnaryOp::Minus, binary(int(2), BinaryOp::Power, int(3)));

        assert_eq!(expr, expected);
    }

    #[test]
    fn grouped_unary_base_can_be_powered() {
        // (-2) ** 3
        let expr = parse_expression("(-2) ** 3");

        let expected = binary(unary(UnaryOp::Minus, int(2)), BinaryOp::Power, int(3));

        assert_eq!(expr, expected);
    }

    #[test]
    fn power_accepts_call_expression_as_left_operand() {
        let expr = parse_expression("foo() ** 2");

        let ExprKind::Binary { left, op, right } = expr.kind() else {
            panic!("expected binary expression");
        };

        assert_eq!(*op, BinaryOp::Power);
        assert!(matches!(left.kind(), ExprKind::Call { .. }));
        assert_eq!(**right, int(2));
    }

    #[test]
    fn power_accepts_call_expression_as_right_operand() {
        let expr = parse_expression("2 ** foo()");

        let ExprKind::Binary { left, op, right } = expr.kind() else {
            panic!("expected binary expression");
        };

        assert_eq!(*op, BinaryOp::Power);
        assert_eq!(**left, int(2));
        assert!(matches!(right.kind(), ExprKind::Call { .. }));
    }

    #[rstest]
    #[case(
        "10 - 5 + 2",
        binary(binary(int(10), BinaryOp::Minus, int(5)), BinaryOp::Plus, int(2),)
    )]
    #[case(
        "10 + 5 - 2",
        binary(binary(int(10), BinaryOp::Plus, int(5)), BinaryOp::Minus, int(2),)
    )]
    #[case(
        "24 / 3 * 2",
        binary(binary(int(24), BinaryOp::Divide, int(3)), BinaryOp::Multiply, int(2),)
    )]
    #[case(
        "24 * 3 / 2",
        binary(binary(int(24), BinaryOp::Multiply, int(3)), BinaryOp::Divide, int(2),)
    )]
    fn operators_with_equal_precedence_are_left_associative(
        #[case] source: &str,
        #[case] expected: Expr,
    ) {
        assert_eq!(parse_expression(source), expected);
    }

    #[test]
    fn parses_mixed_precedence_regression() {
        let expr = parse_expression("30 * 30 + 665 * (4 + 3)");

        let expected = binary(
            binary(int(30), BinaryOp::Multiply, int(30)),
            BinaryOp::Plus,
            binary(
                int(665),
                BinaryOp::Multiply,
                binary(int(4), BinaryOp::Plus, int(3)),
            ),
        );

        assert_eq!(expr, expected);
    }

    #[test]
    fn nested_groups_are_parsed() {
        let expr = parse_expression("((1 + 2) * 3)");

        let expected = binary(
            binary(int(1), BinaryOp::Plus, int(2)),
            BinaryOp::Multiply,
            int(3),
        );

        assert_eq!(expr, expected);
    }

    #[rstest]
    #[case("-42", unary(UnaryOp::Minus, int(42)))]
    #[case("!42", unary(UnaryOp::Not, int(42)))]
    #[allow(clippy::approx_constant)]
    #[case("-3.14", unary(UnaryOp::Minus, float(3.14)))]
    fn parses_unary_operators(#[case] source: &str, #[case] expected: Expr) {
        assert_eq!(parse_expression(source), expected);
    }

    #[rstest]
    #[case(
        "1 + 2 * (false)",
        binary(
            int(1),
            BinaryOp::Plus,
            binary(int(2), BinaryOp::Multiply, boolean(false),),
        )
    )]
    #[case(
        "(true + 1) * 2",
        binary(
            binary(boolean(true), BinaryOp::Plus, int(1),),
            BinaryOp::Multiply,
            int(2),
        )
    )]
    #[case(
        "true + false * 2",
        binary(
            boolean(true),
            BinaryOp::Plus,
            binary(boolean(false), BinaryOp::Multiply, int(2),),
        )
    )]
    fn parses_booleans_in_complex_expressions(#[case] source: &str, #[case] expected: Expr) {
        assert_eq!(parse_expression(source), expected);
    }

    #[test]
    fn parses_boolean_inside_complex_expression() {
        let expr = parse_expression("1 + 2 * (false)");

        let expected = binary(
            int(1),
            BinaryOp::Plus,
            binary(int(2), BinaryOp::Multiply, boolean(false)),
        );

        assert_eq!(expr, expected);
    }

    #[test]
    fn parses_grouped_boolean_expression() {
        let expr = parse_expression("(true + 1) * 2");

        let expected = binary(
            binary(boolean(true), BinaryOp::Plus, int(1)),
            BinaryOp::Multiply,
            int(2),
        );

        assert_eq!(expr, expected);
    }
    #[test]
    fn parses_unary_identifier() {
        let expr = parse_expression("-foo");

        let ExprKind::Unary { op, expr: inner } = expr.kind() else {
            panic!("Expected unary expression");
        };

        assert_eq!(*op, UnaryOp::Minus);
        assert!(matches!(inner.kind(), ExprKind::Identifier(_)));
    }

    #[test]
    fn parses_unary_call() {
        let expr = parse_expression("!foo()");

        let ExprKind::Unary { op, expr: inner } = expr.kind() else {
            panic!("Expected unary expression");
        };

        assert_eq!(*op, UnaryOp::Not);
        assert!(matches!(inner.kind(), ExprKind::Call { .. }));
    }

    #[test]
    fn unary_binds_tighter_than_addition() {
        // -1 + 2 should be parsed as (-1) + 2, not -(1 + 2)
        let expr = parse_expression("-1 + 2");

        let expected = binary(unary(UnaryOp::Minus, int(1)), BinaryOp::Plus, int(2));

        assert_eq!(expr, expected);
    }

    #[test]
    fn unary_binds_tighter_than_multiplication() {
        // !1 * 2 should be parsed as (!1) * 2
        let expr = parse_expression("!1 * 2");

        let expected = binary(unary(UnaryOp::Not, int(1)), BinaryOp::Multiply, int(2));

        assert_eq!(expr, expected);
    }

    #[test]
    fn multiple_unary_operators_are_parsed() {
        // !!1 should be parsed as !(!1)
        let expr = parse_expression("!!1");

        let expected = unary(UnaryOp::Not, unary(UnaryOp::Not, int(1)));

        assert_eq!(expr, expected);
    }

    #[test]
    fn parses_complex_power_precedence_and_associativity() {
        // 1 + 2 * 3 ** 4 ** 5 - 6
        //
        // Expected:
        // 1 + (2 * (3 ** (4 ** 5))) - 6
        let expr = parse_expression("1 + 2 * 3 ** 4 ** 5 - 6");

        let expected = binary(
            binary(
                int(1),
                BinaryOp::Plus,
                binary(
                    int(2),
                    BinaryOp::Multiply,
                    binary(
                        int(3),
                        BinaryOp::Power,
                        binary(int(4), BinaryOp::Power, int(5)),
                    ),
                ),
            ),
            BinaryOp::Minus,
            int(6),
        );

        assert_eq!(expr, expected);
    }

    // =========================================================================
    // Span Tests
    // =========================================================================
    #[test]
    fn parses_literal_spans() {
        let expr = parse_expression("42");
        // Assuming "42" starts at byte 0 and ends at byte 2, on line 1, column 1
        assert_eq!(expr.span(), Span::new(0, 2));
    }

    #[test]
    fn parses_binary_expression_spans() {
        // "1 + 2"
        // '1' is at 0..1
        // '+' is at 2..3
        // '2' is at 4..5
        let expr = parse_expression("1 + 2");

        // The binary expression should span from the start of '1' to the end of '2' (0..5)
        assert_eq!(expr.span(), Span::new(0, 5));

        // Verify inner node spans as well
        if let ExprKind::Binary { left, right, .. } = expr.kind() {
            assert_eq!(left.span(), Span::new(0, 1));
            assert_eq!(right.span(), Span::new(4, 5));
        } else {
            panic!("Expected binary expression");
        }
    }

    #[test]
    fn parses_power_expression_spans() {
        let expr = parse_expression("2 ** 10");

        assert_eq!(expr.span(), Span::new(0, 7));

        let ExprKind::Binary { left, op, right } = expr.kind() else {
            panic!("expected binary expression");
        };

        assert_eq!(*op, BinaryOp::Power);
        assert_eq!(left.span(), Span::new(0, 1));
        assert_eq!(right.span(), Span::new(5, 7));
    }

    #[test]
    fn parses_grouped_expression_spans() {
        // "(1 + 2)"
        // '(' at 0..1, '1' at 1..2, '+' at 3..4, '2' at 5..6, ')' at 6..7
        let expr = parse_expression("(1 + 2)");

        // The grouped expression span should enclose the parentheses (0..7)
        assert_eq!(expr.span(), Span::new(0, 7));
    }

    #[test]
    fn parses_nested_precedence_spans() {
        // "1 + 2 * 3"
        let expr = parse_expression("1 + 2 * 3");

        // Outer binary expression (+): spans from '1' (0) to '3' (8) -> 0..9 roughly depending on exact spacing
        assert_eq!(expr.span().start(), 0);

        // Inner binary expression (*): "2 * 3" spans from '2' to '3'
        if let ExprKind::Binary { right, .. } = expr.kind() {
            assert_eq!(right.span(), Span::new(4, 9));
        } else {
            panic!("Expected binary expression");
        }
    }

    #[test]
    fn parses_complex_expression_spans_with_spacing() {
        // Source: "  ( 10 + 20 ) * 30  "        // Index 0-1: two spaces "  "
        // Index 2: '('
        // Index 3: ' '
        // Index 4-5: '10'
        // Index 6: ' '
        // Index 7: '+'
        // Index 8: ' '
        // Index 9-10: '20'
        // Index 11: ' '
        // Index 12: ')'
        // Index 13: ' '
        // Index 14: '*'
        // Index 15: ' '
        // Index 16-17: '30'
        // Index 18-19: two trailing spaces "  "
        let source = "  ( 10 + 20 ) * 30  ";
        let expr = parse_expression(source);

        // The overall expression is a multiplication (*) spanning from the opening '(' to '30'
        // Opening parenthesis starts at index 2, '30' ends at index 18.
        assert_eq!(expr.span(), Span::new(2, 18));

        if let ExprKind::Binary { left, op, right } = expr.kind() {
            assert_eq!(*op, BinaryOp::Multiply);

            // Left side is the grouped expression "( 10 + 20 )"
            assert_eq!(left.span(), Span::new(2, 13));

            // Right side is the integer literal "30"
            assert_eq!(right.span(), Span::new(16, 18));

            // Check inner parts of the grouped expression
            if let ExprKind::Binary {
                left: inner_left,
                op: inner_op,
                right: inner_right,
            } = left.kind()
            {
                assert_eq!(*inner_op, BinaryOp::Plus);
                // "10" spans from index 4 to 6
                assert_eq!(inner_left.span(), Span::new(4, 6));
                // "20" spans from index 9 to 11
                assert_eq!(inner_right.span(), Span::new(9, 11));
            } else {
                panic!("Expected inner binary expression inside parentheses");
            }
        } else {
            panic!("Expected outer multiplication expression");
        }
    }

    #[test]
    fn parses_unary_expression_spans() {
        // "-42"
        // '-' is at 0..1 (col 1)
        // '42' is at 1..3 (col 2)
        let expr = parse_expression("-42");

        // The unary expression should span from the start of '-' to the end of '42' (0..3)
        assert_eq!(expr.span(), Span::new(0, 3));

        // Verify inner node span as well
        if let ExprKind::Unary {
            expr: inner_expr, ..
        } = expr.kind()
        {
            assert_eq!(inner_expr.span(), Span::new(1, 3));
        } else {
            panic!("Expected unary expression");
        }
    }

    #[test]
    fn parses_unary_expression_spans_with_spacing() {
        // "!  42"
        // '!' is at 0..1 (col 1)
        // two spaces at 1..3
        // '42' is at 3..5 (col 4)
        let expr = parse_expression("!  42");

        // The unary expression should span from the start of '!' to the end of '42' (0..5)
        assert_eq!(expr.span(), Span::new(0, 5));

        if let ExprKind::Unary {
            expr: inner_expr, ..
        } = expr.kind()
        {
            assert_eq!(inner_expr.span(), Span::new(3, 5));
        } else {
            panic!("Expected unary expression");
        }
    }

    #[test]
    fn open_paren_can_start_primary_expression() {
        assert!(is_primary_expr_start(TokenKind::OpenParen));
    }

    #[test]
    fn open_brace_cannot_start_primary_expression() {
        assert!(!is_primary_expr_start(TokenKind::OpenBrace));
    }

    #[rstest]
    #[case("foo()")]
    #[case("foo(1)")]
    #[case("foo(1, true)")]
    #[case("foo(value, other)")]
    #[case("foo(bar(), baz())")]
    #[case("foo((1 + 2), -value)")]
    #[case("foo(1, bar(2, 3), true)")]
    fn parses_valid_argument_lists(#[case] source: &str) {
        parse_expression(source);
    }

    #[rstest]
    #[case("foo(1 true)")]
    #[case("foo(1 value)")]
    #[case("foo(true false)")]
    #[case("foo(value other)")]
    #[case("foo(1 bar())")]
    #[case("foo(bar() baz())")]
    #[case("foo((1 + 2) value)")]
    fn reports_missing_comma_between_arguments(#[case] source: &str) {
        let error = parse_expression_error(source);

        let ParserError::ExpectedToken { expected, .. } = error else {
            panic!(
                "expected ExpectedTokenError for missing comma in `{source}`, \
                 got: {error:?}"
            );
        };

        assert_eq!(
            expected,
            TokenKind::Comma,
            "expected a comma to be required in `{source}`"
        );
    }

    #[rstest]
    #[case("foo(1, )")]
    #[case("foo(true, )")]
    #[case("foo(value, )")]
    #[case("foo(bar(), )")]
    #[case("foo((1 + 2), )")]
    #[case("foo(1, 2, )")]
    fn reports_missing_expression_after_trailing_comma(#[case] source: &str) {
        let error = parse_expression_error(source);

        assert!(
            matches!(error, ParserError::ExpectedExpression { .. }),
            "expected ExpectedExpressionError after trailing comma in `{source}`, \
         got: {error:?}"
        );
    }

    #[rstest]
    #[case("foo(1,")]
    #[case("foo(true,")]
    #[case("foo(value,")]
    #[case("foo(bar(),")]
    #[case("foo(1, 2,")]
    fn reports_unexpected_eof_after_dangling_comma(#[case] source: &str) {
        let error = parse_expression_error(source);

        assert!(
            matches!(error, ParserError::UnexpectedEof { .. }),
            "expected UnexpectedEofError after dangling comma in `{source}`, \
         got: {error:?}"
        );
    }

    #[rstest]
    #[case("foo(,")]
    #[case("foo(, 1)")]
    #[case("foo(, true)")]
    #[case("foo(, value)")]
    fn reports_missing_expression_before_first_argument(#[case] source: &str) {
        let error = parse_expression_error(source);

        assert!(
            matches!(error, ParserError::ExpectedExpression { .. }),
            "expected ExpectedExpressionError for missing first argument in `{source}`, \
         got: {error:?}"
        );
    }

    #[rstest]
    #[case("foo(1,, 2)")]
    #[case("foo(value,, other)")]
    #[case("foo(bar(),, baz())")]
    fn reports_missing_expression_between_commas(#[case] source: &str) {
        let error = parse_expression_error(source);

        assert!(
            matches!(error, ParserError::ExpectedExpression { .. }),
            "expected ExpectedExpressionError between commas in `{source}`, \
         got: {error:?}"
        );
    }

    #[rstest]
    // i64::MAX is 9223372036854775807, so ending in 8 will overflow
    #[case("9223372036854775808")]
    // A ridiculously large number of digits
    #[case("99999999999999999999999999999999999")]
    fn reports_error_on_integer_overflow(#[case] source: &str) {
        let error = parse_expression_error(source);

        assert!(
            matches!(error, ParserError::NumberOutOfBounds { .. }),
            "expected NumberOutOfBounds error for huge integer `{source}`, \
             got: {error:?}"
        );
    }

    #[rstest]
    // 1.8e308 is just over the maximum limit of an f64
    #[case(
        "180000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000.0"
    )]
    fn reports_error_on_float_overflow(#[case] source: &str) {
        let error = parse_expression_error(source);

        assert!(
            matches!(error, ParserError::NumberOutOfBounds { .. }),
            "expected NumberOutOfBounds error for float overflow, got: {error:?}"
        );
    }
}
