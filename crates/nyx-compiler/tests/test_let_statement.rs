use nyx_compiler::{Compiler, Disassembler};
use nyx_diagnostic::ParserError;
use nyx_lexer::Lexer;
use nyx_parser::{Parser, ast::Stmt};
use nyx_source::SourceFile;
use nyx_token::Token;

fn compile_source(source: &str) -> String {
    let source_file = SourceFile::new("test.nyx", source.to_owned());

    let mut lexer = Lexer::new(source_file.clone());

    let tokens = lexer
        .by_ref()
        .collect::<Result<Vec<Token>, _>>()
        .expect("lexing failed");

    let lexer_errors = lexer.take_errors();
    assert!(
        lexer_errors.is_empty(),
        "lexer reported errors: {lexer_errors:#?}"
    );

    let parser = Parser::new(&tokens, &lexer.symbol_registry, source_file);

    let statements = parser
        .collect::<Result<Vec<Stmt>, ParserError>>()
        .expect("parsing failed");

    let mut compiler = Compiler::new(&statements);
    compiler.compile();

    let bytecode = compiler.bytecode();

    Disassembler::new(&bytecode, &lexer.symbol_registry).to_string()
}

#[test]
fn test_compile_integer_let_binding() {
    insta::assert_snapshot!(compile_source("let name = 10;"));
}

#[test]
fn test_compile_float_let_binding() {
    insta::assert_snapshot!(compile_source("let pi = 3.14;"));
}

#[test]
fn test_compile_true_let_binding() {
    insta::assert_snapshot!(compile_source("let enabled = true;"));
}

#[test]
fn test_compile_false_let_binding() {
    insta::assert_snapshot!(compile_source("let enabled = false;"));
}

#[test]
fn test_compile_identifier_reference() {
    insta::assert_snapshot!(compile_source(
        r#"
        let x = 10;
        let y = x;
        "#
    ));
}

#[test]
fn test_compile_addition() {
    insta::assert_snapshot!(compile_source("let result = 10 + 20;"));
}

#[test]
fn test_compile_subtraction() {
    insta::assert_snapshot!(compile_source("let result = 20 - 10;"));
}

#[test]
fn test_compile_multiplication() {
    insta::assert_snapshot!(compile_source("let result = 10 * 20;"));
}

#[test]
fn test_compile_division() {
    insta::assert_snapshot!(compile_source("let result = 20 / 10;"));
}

#[test]
fn test_compile_unary_negation() {
    insta::assert_snapshot!(compile_source("let value = -10;"));
}

#[test]
fn test_compile_logical_not() {
    insta::assert_snapshot!(compile_source("let value = !true;"));
}

#[test]
fn test_compile_multiple_let_bindings() {
    insta::assert_snapshot!(compile_source(
        r#"
        let x = 10;
        let y = 20;
        let z = 30;
        "#
    ));
}

#[test]
fn test_compile_expression_using_globals() {
    insta::assert_snapshot!(compile_source(
        r#"
        let x = 10;
        let y = 20;
        let result = x + y;
        "#
    ));
}

#[test]
fn test_compile_nested_binary_expression() {
    insta::assert_snapshot!(compile_source("let result = 10 + 20 * 30;"));
}

#[test]
fn test_compile_nested_unary_expression() {
    insta::assert_snapshot!(compile_source("let result = -(-10);"));
}

#[test]
fn test_compile_deduplicates_multiple_constant_types() {
    insta::assert_snapshot!(compile_source(
        r#"
        let a = 10;
        let b = 10;
        let c = 3.14;
        let d = 3.14;
        let e = true;
        let f = true;
        let g = false;
        let h = false;
        "#
    ));
}

#[test]
fn test_compile_reuses_constant_across_expressions() {
    insta::assert_snapshot!(compile_source(
        r#"
        let x = 10;
        let y = x + 10;
        let z = y + 10;
        "#
    ));
}

#[test]
fn test_integer_overflow_is_not_constant_folded() {
    insta::assert_snapshot!(compile_source("let result = 9223372036854775807 + 1;"));
}

#[test]
fn test_integer_division_by_zero_is_not_constant_folded() {
    insta::assert_snapshot!(compile_source("let result = 10 / 0;"));
}

#[test]
fn test_float_division_by_zero_is_not_constant_folded() {
    insta::assert_snapshot!(compile_source("let result = 10.0 / 0.0;"));
}

#[test]
fn test_integer_underflow_is_not_constant_folded() {
    insta::assert_snapshot!(compile_source("let result = -9223372036854775807 - 2;"));
}

#[test]
fn test_integer_multiplication_overflow_is_not_constant_folded() {
    insta::assert_snapshot!(compile_source("let result = 9223372036854775807 * 2;"));
}
