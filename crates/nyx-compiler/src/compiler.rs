use crate::ByteCode;
use crate::fold_expr;
use crate::opcodes::{BinaryOpCode, UnaryOpCode};
use nyx_parser::ast::{BinaryOp, Expr, ExprKind, SpannedIdentifier, Stmt, StmtKind, UnaryOp};
use nyx_token::Symbol;

pub struct Compiler<'a> {
    stmts: &'a [Stmt],
    bytecode: ByteCode,
}

impl<'a> Compiler<'a> {
    pub fn new(stmts: &'a [Stmt]) -> Self {
        Self {
            stmts,
            bytecode: ByteCode::default(),
        }
    }

    pub fn compile(&mut self) {
        for stmt in self.stmts {
            match stmt.kind() {
                StmtKind::Let { name, expr } => self.compile_let_stmt(name, expr),
                _ => todo!("{:?} is not implemented yet", stmt.kind()),
            }
        }
    }

    pub fn bytecode(&mut self) -> ByteCode {
        std::mem::take(&mut self.bytecode)
    }

    fn compile_let_stmt(&mut self, name: &SpannedIdentifier, expr: &Expr) {
        self.compile_expr(expr);

        let index = self.bytecode.register_global(name.symbol());
        self.bytecode.emit_define_global(index);
    }

    fn compile_expr(&mut self, expr: &Expr) {
        if let Some(value) = fold_expr(expr) {
            let index = self.bytecode.store_const(value);
            self.bytecode.emit_load_constant(index);
            return;
        }

        match expr.kind() {
            ExprKind::IntLiteral(_) | ExprKind::FloatLiteral(_) | ExprKind::Bool(_) => {
                unreachable!("literals are always folded")
            }

            ExprKind::Identifier(symbol) => self.compile_identifier(symbol),
            ExprKind::Binary { left, op, right } => self.compile_binary_expr(left, op, right),
            ExprKind::Unary { op, expr } => self.compile_unary_expr(op, expr),
            ExprKind::Call { .. } => todo!("function call is not implemented yet!"),
        }
    }

    fn compile_identifier(&mut self, symbol: &Symbol) {
        let index = self
            .bytecode
            .globals()
            .get(symbol)
            .copied()
            .expect("referenced undefined global");

        self.bytecode.emit_load_global(index);
    }

    fn compile_binary_expr(&mut self, left: &Expr, op: &BinaryOp, right: &Expr) {
        self.compile_expr(left);
        self.compile_expr(right);

        match op {
            BinaryOp::Plus => self.bytecode.emit_binary_opcode(BinaryOpCode::Add),
            BinaryOp::Minus => self.bytecode.emit_binary_opcode(BinaryOpCode::Sub),
            BinaryOp::Multiply => self.bytecode.emit_binary_opcode(BinaryOpCode::Mul),
            BinaryOp::Divide => self.bytecode.emit_binary_opcode(BinaryOpCode::Div),
            BinaryOp::Assignment => todo!("assignment is not implemented yet!"),
        }
    }

    fn compile_unary_expr(&mut self, op: &UnaryOp, expr: &Expr) {
        self.compile_expr(expr);

        match op {
            UnaryOp::Minus => self.bytecode.emit_unary_opcode(UnaryOpCode::Neg),
            UnaryOp::Not => self.bytecode.emit_unary_opcode(UnaryOpCode::Not),
        }
    }
}
