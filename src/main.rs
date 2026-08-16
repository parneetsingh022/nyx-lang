// SPDX-License-Identifier: MIT OR Apache-2.0

use std::{env, fs, process};

use nyx_compiler::{Compiler, Disassembler};
use nyx_diagnostic::ParserError;
use nyx_lexer::Lexer;
use nyx_parser::{Parser, ast::Stmt};
use nyx_source::SourceFile;
use nyx_token::Token;

fn main() {
    // Collect arguments from the command line
    let args: Vec<String> = env::args().collect();

    // Check if the file path argument is provided
    if args.len() < 2 {
        eprintln!("Usage: {} <file_path>", args[0]);
        process::exit(1);
    }

    let file_path = &args[1];

    // Read the file, exiting gracefully if the file cannot be read
    let code = fs::read_to_string(file_path).unwrap_or_else(|err| {
        eprintln!("Error reading file '{}': {}", file_path, err);
        process::exit(1);
    });

    let source_file = SourceFile::new(file_path, code);

    let mut lexer = Lexer::new(source_file.clone());
    let tokens: Vec<Token> = match lexer.by_ref().collect::<Result<Vec<_>, _>>() {
        Ok(tokens) => tokens,
        Err(err) => {
            print_collected_errors(&mut lexer);
            eprintln!("{:?}", miette::Report::new(err));
            std::process::exit(1);
        }
    };

    if print_collected_errors(&mut lexer) {
        std::process::exit(1);
    }

    let parser = Parser::new(&tokens, &lexer.symbol_registry, source_file.clone());

    let ast: Result<Vec<Stmt>, ParserError> = parser.collect();

    match ast {
        Ok(statements) => {
            for stmt in &statements {
                println!("{:#?}", stmt.debug_with(&lexer.symbol_registry));
            }

            let compiler = Compiler::new(&statements);
            let bytecode = compiler.compile();

            let dis = Disassembler::new(&bytecode, &lexer.symbol_registry);
            println!("{}", dis);
        }
        Err(err) => {
            eprintln!("{:?}", miette::Report::new(err));
        }
    }
}

/// Prints all error collected by lexer.
///
/// Returns true if any errors were printed.
fn print_collected_errors(lexer: &mut Lexer) -> bool {
    let errors = lexer.take_errors();
    if errors.is_empty() {
        return false;
    }

    eprintln!("Lexing failed with {} error(s):", errors.len());
    for err in errors {
        eprintln!("{:?}", miette::Report::new(err));
    }

    true
}
