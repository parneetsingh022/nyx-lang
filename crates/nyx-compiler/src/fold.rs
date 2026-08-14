//! Constant folding module for the Nyx AST.
//!
//! This module provides functionality to evaluate expressions at compile-time
//! (constant folding). It recursively traverses the Abstract Syntax Tree (AST)
//! and attempts to reduce operations on literal values (integers, floats, and booleans)
//! into single literal results. If an expression cannot be fully resolved at compile-time
//! (e.g., it relies on runtime variables or encounters an overflow), the folder will return `None`.

use crate::Value;
use nyx_parser::ast::{BinaryOp, Expr, ExprKind, UnaryOp};

/// Attempts to evaluate an expression into a constant `Value`.
///
/// This is the main entry point for the constant folding pass. It recursively
/// evaluates literals, unary operations, and binary operations.
///
/// # Arguments
/// * `expr` - A reference to the AST expression to evaluate.
///
/// # Returns
/// * `Some(Value)` if the expression can be completely resolved at compile-time.
/// * `None` if the expression contains non-constant nodes or if an arithmetic error occurs.
pub(crate) fn fold_expr(expr: &Expr) -> Option<Value> {
    match expr.kind() {
        ExprKind::IntLiteral(value) => Some(Value::Int(*value)),
        ExprKind::FloatLiteral(value) => Some(Value::Float(*value)),
        ExprKind::Bool(value) => Some(Value::Bool(*value)),

        ExprKind::Unary { op, expr } => fold_unary(*op, fold_expr(expr)?),
        ExprKind::Binary { left, op, right } => {
            fold_binary(fold_expr(left)?, *op, fold_expr(right)?)
        }
        _ => None,
    }
}

fn fold_unary(op: UnaryOp, value: Value) -> Option<Value> {
    match (op, value) {
        (UnaryOp::Minus, Value::Int(value)) => Some(Value::Int(-value)),
        (UnaryOp::Minus, Value::Float(value)) => Some(Value::Float(-value)),
        (UnaryOp::Not, Value::Bool(value)) => Some(Value::Bool(!value)),

        _ => None,
    }
}

fn fold_binary(left: Value, op: BinaryOp, right: Value) -> Option<Value> {
    match (left, right) {
        (Value::Int(a), Value::Int(b)) => fold_int(a, op, b),
        (Value::Int(a), Value::Float(b)) => fold_float(a as f64, op, b),
        (Value::Float(a), Value::Int(b)) => fold_float(a, op, b as f64),
        (Value::Float(a), Value::Float(b)) => fold_float(a, op, b),
        _ => None,
    }
}

fn fold_int(left: i64, op: BinaryOp, right: i64) -> Option<Value> {
    let value = match op {
        BinaryOp::Plus => left + right,
        BinaryOp::Minus => left - right,
        BinaryOp::Multiply => left * right,
        BinaryOp::Divide => left / right,
        _ => return None,
    };

    Some(Value::Int(value))
}

fn fold_float(left: f64, op: BinaryOp, right: f64) -> Option<Value> {
    let value = match op {
        BinaryOp::Plus => left + right,
        BinaryOp::Minus => left - right,
        BinaryOp::Multiply => left * right,
        BinaryOp::Divide => left / right,
        _ => return None,
    };

    Some(Value::Float(value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    use nyx_parser::ast::{Expr, ExprKind};
    use nyx_source::Span;
    use nyx_token::{Symbol, SymbolRegistry};

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

    #[test]
    fn test_fold_integer_literal() {
        assert_eq!(fold_expr(&int(10)), Some(Value::Int(10)));
    }

    #[test]
    fn test_fold_float_literal() {
        assert_eq!(fold_expr(&float(3.14)), Some(Value::Float(3.14)));
    }

    #[test]
    fn test_fold_boolean_literal() {
        assert_eq!(fold_expr(&boolean(true)), Some(Value::Bool(true)));
    }

    #[test]
    fn test_fold_integer_negation() {
        let expr = unary(UnaryOp::Minus, int(10));

        assert_eq!(fold_expr(&expr), Some(Value::Int(-10)));
    }

    #[test]
    fn test_fold_float_negation() {
        let expr = unary(UnaryOp::Minus, float(3.14));

        assert_eq!(fold_expr(&expr), Some(Value::Float(-3.14)));
    }

    #[rstest]
    #[case(boolean(true), Value::Bool(false))]
    #[case(boolean(false), Value::Bool(true))]
    #[case(unary(UnaryOp::Not, boolean(true)), Value::Bool(true))]
    #[case(unary(UnaryOp::Not, boolean(false)), Value::Bool(false))]
    fn test_fold_boolean_not(#[case] expr: Expr, #[case] expected: Value) {
        let expr = unary(UnaryOp::Not, expr);

        assert_eq!(fold_expr(&expr), Some(expected));
    }

    #[test]
    fn test_invalid_unary_operation_returns_none() {
        let expr = unary(UnaryOp::Not, int(10));

        assert_eq!(fold_expr(&expr), None);
    }

    #[rstest]
    #[case(BinaryOp::Plus, 10, 20, 30)]
    #[case(BinaryOp::Minus, 20, 10, 10)]
    #[case(BinaryOp::Multiply, 4, 5, 20)]
    #[case(BinaryOp::Divide, 20, 4, 5)]
    fn test_fold_integer_binary_operations(
        #[case] op: BinaryOp,
        #[case] left: i64,
        #[case] right: i64,
        #[case] expected: i64,
    ) {
        let expr = binary(int(left), op, int(right));

        assert_eq!(fold_expr(&expr), Some(Value::Int(expected)));
    }

    #[rstest]
    #[case(BinaryOp::Plus, 10.0, 2.5, 12.5)]
    #[case(BinaryOp::Minus, 10.0, 2.5, 7.5)]
    #[case(BinaryOp::Multiply, 4.0, 2.5, 10.0)]
    #[case(BinaryOp::Divide, 10.0, 2.0, 5.0)]
    fn test_fold_float_binary_operations(
        #[case] op: BinaryOp,
        #[case] left: f64,
        #[case] right: f64,
        #[case] expected: f64,
    ) {
        let expr = binary(float(left), op, float(right));

        assert_eq!(fold_expr(&expr), Some(Value::Float(expected)));
    }

    #[test]
    fn test_fold_int_float_expression() {
        let expr = binary(int(10), BinaryOp::Plus, float(2.5));

        assert_eq!(fold_expr(&expr), Some(Value::Float(12.5)));
    }

    #[test]
    fn test_fold_float_int_expression() {
        let expr = binary(float(2.5), BinaryOp::Multiply, int(4));

        assert_eq!(fold_expr(&expr), Some(Value::Float(10.0)));
    }

    #[test]
    fn test_fold_nested_expression() {
        // (2 + 3) * 4
        let expr = binary(
            binary(int(2), BinaryOp::Plus, int(3)),
            BinaryOp::Multiply,
            int(4),
        );

        assert_eq!(fold_expr(&expr), Some(Value::Int(20)));
    }

    #[test]
    fn test_non_numeric_binary_expression_returns_none() {
        let expr = binary(boolean(true), BinaryOp::Plus, boolean(false));

        assert_eq!(fold_expr(&expr), None);
    }

    #[test]
    fn test_identifier_returns_none() {
        let mut registry = SymbolRegistry::new();
        let name = registry.intern("name");

        assert_eq!(fold_expr(&identifier(name)), None);
    }

    #[test]
    fn test_fold_complex_nested_expression() {
        // Equivalent to:
        //
        // (
        //     (((20 - 5) - 3) + (2 + (3 + 4)) + (-1.5))
        //     * -(8 / 2)
        // )
        // /
        // (
        //     (2.0 + 3) * (10 - 8.0)
        // )
        //
        // Left associative:
        //     (20 - 5) - 3
        //
        // Right nested:
        //     2 + (3 + 4)
        //
        // Expected:
        //     (12 + 9 - 1.5) * -4 / (5.0 * 2.0)
        //     19.5 * -4 / 10
        //     -7.8

        let expr = binary(
            binary(
                binary(
                    binary(
                        binary(int(20), BinaryOp::Minus, int(5)),
                        BinaryOp::Minus,
                        int(3),
                    ),
                    BinaryOp::Plus,
                    binary(
                        int(2),
                        BinaryOp::Plus,
                        binary(int(3), BinaryOp::Plus, int(4)),
                    ),
                ),
                BinaryOp::Plus,
                unary(UnaryOp::Minus, float(1.5)),
            ),
            BinaryOp::Multiply,
            unary(UnaryOp::Minus, binary(int(8), BinaryOp::Divide, int(2))),
        );

        let denominator = binary(
            binary(float(2.0), BinaryOp::Plus, int(3)),
            BinaryOp::Multiply,
            binary(int(10), BinaryOp::Minus, float(8.0)),
        );

        let expr = binary(expr, BinaryOp::Divide, denominator);

        assert_eq!(fold_expr(&expr), Some(Value::Float(-7.8)));
    }
}
