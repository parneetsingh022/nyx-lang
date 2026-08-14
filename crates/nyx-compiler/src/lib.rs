mod bytecode;
mod compiler;
mod disassembler;
mod fold;
mod opcodes;

pub use bytecode::ByteCode;
pub use compiler::Compiler;
pub use disassembler::Disassembler;
pub use opcodes::OpCode;

pub(crate) use fold::fold_expr;

use nyx_token::Symbol;

#[derive(Debug, Clone)]
pub enum Value {
    Int(i64),
    Float(f64),
    Bool(bool),
    Ident(Symbol),
}
