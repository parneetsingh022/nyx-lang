// SPDX-License-Identifier: MIT OR Apache-2.0

//! Abstract Syntax Tree (AST) definitions for the nyx programming language.
//!
//! This module defines the core data structures that represent parsed source code
//! produced by the parser.
//!
//! # Spans & Diagnostics
//!
//! AST nodes generally pair their semantic definitions with a [`Span`]. This
//! preserves original source code locations for downstream compiler phases,
//! enabling precise diagnostic and error reporting.
//!
//! Note that [`PartialEq`] implementations across AST nodes intentionally compare
//! **only** semantic structure (ignoring spans) to simplify structural testing
//! and snapshot assertions.
//!
//! # Pretty Printing & Debugging
//!
//! Standard [`fmt::Debug`] formatting prints raw symbol IDs for interned identifiers.
//! For human-readable output during testing or debugging, AST nodes provide custom
//! `debug_with` helper methods that resolve interned [`Symbol`] IDs against a
//! [`SymbolRegistry`].

use std::fmt;

use nyx_source::Span;
use nyx_token::{Symbol, SymbolRegistry, Token, TokenKind};

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Precedence {
    Lowest = 0,
    Assignment = 10, // =

    Term = 60,   // +, -
    Factor = 70, // *, /
    Prefix = 80, // -, !
    Power = 90,  // **
}

impl Precedence {
    /// Helper to convert enum level into left and right Pratt binding powers.
    /// - For Left-associative:  left = base, right = base + 1
    /// - For Right-associative: left = base + 1, right = base
    pub fn left_assoc(self) -> (u8, u8) {
        let base = self as u8;
        (base, base + 1)
    }

    pub fn right_assoc(self) -> (u8, u8) {
        let base = self as u8;
        (base + 1, base)
    }
}

// =============================================================================
// Operators
// =============================================================================

/// Represents unary operations in expressions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Minus,
    Not,
}

impl UnaryOp {
    /// Returns the right binding power of the unary operator.
    /// Prefix operators bind very tightly, higher than multiplication and division.
    pub fn binding_power(self) -> u8 {
        match self {
            Self::Minus | Self::Not => Precedence::Prefix as u8,
        }
    }

    pub fn from_token(token: Token) -> Option<UnaryOp> {
        let op = match token.kind {
            TokenKind::Minus => Self::Minus,
            TokenKind::Bang => Self::Not,
            _ => return None,
        };

        Some(op)
    }
}

/// Represents binary arithmetic operations in expressions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    /// Addition (`+`)
    Plus,
    /// Subtraction (`-`)
    Minus,
    /// Multiplication (`*`)
    Multiply,
    /// Division (`/`)
    Divide,
    /// Power (`**`)
    Power,
    /// Assignment (`=`)
    Assignment,
}

impl BinaryOp {
    pub fn binding_power(self) -> (u8, u8) {
        match self {
            // Right Associative
            Self::Assignment => Precedence::Assignment.right_assoc(),
            // Left Associative
            Self::Plus | Self::Minus => Precedence::Term.left_assoc(),
            Self::Multiply | Self::Divide => Precedence::Factor.left_assoc(),
            Self::Power => Precedence::Power.right_assoc(),
        }
    }

    pub fn from_token(token: Token) -> Option<BinaryOp> {
        let op = match token.kind {
            TokenKind::Plus => BinaryOp::Plus,
            TokenKind::Minus => BinaryOp::Minus,
            TokenKind::Star => BinaryOp::Multiply,
            TokenKind::StarStar => BinaryOp::Power,
            TokenKind::Slash => BinaryOp::Divide,
            TokenKind::Eq => BinaryOp::Assignment,
            _ => return None,
        };

        Some(op)
    }
}

// =============================================================================
// Identifiers
// =============================================================================

/// Represents an identifier stored in the abstract syntax tree.
///
/// An identifier contains both:
///
/// - the interned [`Symbol`] used to efficiently identify and compare its name;
/// - the source [`Span`] covering the identifier's occurrence in the source code.
///
/// Keeping the span alongside the symbol allows later compiler stages to produce
/// diagnostics that point directly to the identifier. This is particularly useful
/// for identifiers that are not wrapped in another spanned AST node, such as the
/// declared name in a `let` statement.
///
/// Identifier expressions may continue storing only a [`Symbol`] inside
/// `ExprKind::Identifier`, because the surrounding `Expr` already carries the
/// expression's span.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpannedIdentifier {
    /// The interned symbol representing the identifier's textual name.
    symbol: Symbol,

    /// The location of this identifier occurrence in the source file.
    span: Span,
}

impl SpannedIdentifier {
    /// Creates an identifier from an interned symbol and its source span.
    pub fn new(symbol: Symbol, span: Span) -> Self {
        Self { symbol, span }
    }

    /// Returns the interned symbol representing the identifier's name.
    pub fn symbol(self) -> Symbol {
        self.symbol
    }

    /// Returns the source span covering this identifier.
    pub fn span(self) -> Span {
        self.span
    }
}

// =============================================================================
// Expressions
// =============================================================================

/// The semantic variant of an expression in the abstract syntax tree (AST).
#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    IntLiteral(i64),
    FloatLiteral(f64),
    Identifier(Symbol),
    Bool(bool),
    Binary {
        left: Box<Expr>,
        op: BinaryOp,
        right: Box<Expr>,
    },
    Unary {
        op: UnaryOp,
        expr: Box<Expr>,
    },
    Call {
        callee: Box<Expr>,
        arguments: Vec<Expr>,
    },
}

/// Represents an expression in the abstract syntax tree (AST).
///
/// An `Expr` pairs an expression variant ([`ExprKind`]), which defines
/// its semantic structure, with a source [`Span`] for error reporting and
/// source mapping.
#[derive(Debug, Clone)]
pub struct Expr {
    kind: ExprKind,
    span: Span,
}

impl Expr {
    pub fn new(kind: ExprKind, span: Span) -> Self {
        Self { kind, span }
    }

    pub fn span(&self) -> Span {
        self.span
    }

    pub fn kind(&self) -> &ExprKind {
        &self.kind
    }

    pub fn set_span(&mut self, span: Span) {
        self.span = span;
    }

    pub fn debug_with<'a>(&'a self, reg: &'a SymbolRegistry) -> ExprDebug<'a> {
        ExprDebug { expr: self, reg }
    }

    /// Returns whether this expression can appear as a standalone statement.
    ///
    /// Expressions that are not valid in statement position are rejected to avoid
    /// evaluating and discarding a result unintentionally.
    pub fn is_valid_expr_statement(&self) -> bool {
        self.is_assignment() || self.is_call()
    }

    /// Returns `true` if this expression is an assignment binary operation.
    fn is_assignment(&self) -> bool {
        matches!(
            self.kind(), // Now `self` is an Expr!
            ExprKind::Binary {
                op: BinaryOp::Assignment,
                ..
            }
        )
    }
    /// Returns `true` if this expression is a function or method call.
    fn is_call(&self) -> bool {
        matches!(self.kind(), ExprKind::Call { .. })
    }
}

// We intentionally compare only the ExprKind here so parser tests can
// assert AST structure without needing exact span/location matching.
impl PartialEq for Expr {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind
    }
}

// =============================================================================
// Statements
// =============================================================================

/// The semantic variant of a statement in the abstract syntax tree (AST).
#[derive(Debug, Clone, PartialEq)]
pub enum StmtKind {
    /// Declares a new variable and initializes it with an expression.
    ///
    /// For example, the statement:
    ///
    /// ```text
    /// let answer = 42;
    /// ```
    ///
    /// stores the declared variable name in `name` and the initializer
    /// expression in `expr`.
    Let {
        /// The interned symbol with span representing the declared variable name.
        name: SpannedIdentifier,

        /// The expression used to initialize the variable.
        expr: Expr,
    },

    /// A conditional `if` statement with an optional `else` branch.
    ///
    /// The `then_branch` represents the block executed if the condition evaluates
    /// to true. The `else_branch` is used to handle both standard `else` fallbacks
    /// and chained `else if` conditions.
    ///
    /// For a standard `if` statement:
    ///
    /// ```text
    /// if x > 0 {
    ///     print("positive");
    /// }
    /// ```
    ///
    /// To support `else if` chaining without requiring a separate AST node type,
    /// the `else_branch` simply contains another `If` statement. This creates a
    /// naturally nested recursive structure:
    ///
    /// ```text
    /// if x > 0 {
    ///     print("positive");
    /// } else if x < 0 {
    ///     print("negative");
    /// } else {
    ///     print("zero");
    /// }
    /// ```
    ///
    /// In the example above, the first `If` node's `else_branch` holds the second `If` node.
    If {
        /// The boolean condition to evaluate.
        condition: Expr,

        /// The statement executed if the condition is true.
        /// this will always be parsed as a `StmtKind::Block`.
        then_branch: Box<Stmt>,

        /// The optional statement executed if the condition is false.
        /// - For an `else { ... }`, this contains a `StmtKind::Block`.
        /// - For an `else if ...`, this contains another `StmtKind::If`.
        /// - If there is no else branch, this is `None`.
        else_branch: Option<Box<Stmt>>,
    },

    /// Evaluates an expression as a standalone statement.
    ///
    /// Expression statements allow side-effecting expressions—such as function
    /// calls or assignments—to be executed where a statement is expected.
    ///
    /// For example, the statement:
    ///
    /// ```text
    /// print("Hello, world!");
    /// ```
    ///
    /// stores the evaluated expression in `expr`.
    ExprStmt {
        /// The inner expression to be evaluated.
        expr: Expr,
    },

    /// A sequence of statements grouped together as a single statement.
    ///
    /// Block statements are enclosed in curly braces (`{}`) and
    /// introduce a new lexical scope.
    ///
    /// For example, the statement:
    ///
    /// ```text
    /// {
    ///     let a = 1;
    ///     print(a);
    /// }
    /// ```
    ///
    /// stores the list of enclosed statements in `stmts`.
    Block {
        /// The sequence of statements contained within the block.
        stmts: Vec<Stmt>,
    },
}

/// A parsed statement in the abstract syntax tree.
///
/// It stores the statement itself in [`StmtKind`] and the part of the source
/// code that produced it in [`Span`].
///
/// The span is used when reporting errors or mapping the statement back to
/// the original source.
#[derive(Debug, Clone)]
pub struct Stmt {
    kind: StmtKind,
    span: Span,
}

impl Stmt {
    pub fn new(kind: StmtKind, span: Span) -> Self {
        Self { kind, span }
    }

    pub fn kind(&self) -> &StmtKind {
        &self.kind
    }

    pub fn span(&self) -> Span {
        self.span
    }

    pub fn debug_with<'a>(&'a self, reg: &'a SymbolRegistry) -> StmtDebug<'a> {
        StmtDebug { stmt: self, reg }
    }
}

// We intentionally compare only the StmtKind here so parser tests can
// assert AST structure without needing exact span/location matching.
impl PartialEq for Stmt {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind
    }
}

// =============================================================================
// AST Debug Helpers
// =============================================================================

pub struct ExprDebug<'a> {
    expr: &'a Expr,
    reg: &'a SymbolRegistry,
}

impl fmt::Debug for ExprDebug<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.expr.kind {
            ExprKind::IntLiteral(value) => f.debug_tuple("IntLiteral").field(value).finish(),
            ExprKind::FloatLiteral(value) => f.debug_tuple("FloatLiteral").field(value).finish(),
            ExprKind::Identifier(symbol) => {
                let name = self.reg.resolve(*symbol);

                f.debug_tuple("Identifier").field(&name).finish()
            }
            ExprKind::Bool(kind) => f.debug_tuple("Boolean").field(kind).finish(),
            ExprKind::Binary { left, op, right } => f
                .debug_struct("Binary")
                .field("left", &left.debug_with(self.reg))
                .field("op", op)
                .field("right", &right.debug_with(self.reg))
                .finish(),
            ExprKind::Unary { op, expr } => f
                .debug_struct("Unary")
                .field("op", op)
                .field("expr", &expr.debug_with(self.reg))
                .finish(),
            ExprKind::Call { callee, arguments } => {
                let arguments = arguments
                    .iter()
                    .map(|argument| argument.debug_with(self.reg))
                    .collect::<Vec<_>>();

                f.debug_struct("Call")
                    .field("callee", &callee.debug_with(self.reg))
                    .field("arguments", &arguments)
                    .finish()
            }
        }
    }
}

pub struct StmtDebug<'a> {
    stmt: &'a Stmt,
    reg: &'a SymbolRegistry,
}

impl fmt::Debug for StmtDebug<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.stmt.kind {
            StmtKind::Let { name, expr } => f
                .debug_struct("Let")
                .field("name", &self.reg.resolve(name.symbol()))
                .field("expr", &expr.debug_with(self.reg))
                .finish(),
            StmtKind::If {
                condition,
                then_branch,
                else_branch,
            } => f
                .debug_struct("If")
                .field("condition", &condition.debug_with(self.reg))
                .field("then_branch", &then_branch.debug_with(self.reg))
                .field(
                    "else_branch",
                    &else_branch.as_ref().map(|stmt| stmt.debug_with(self.reg)),
                )
                .finish(),
            StmtKind::ExprStmt { expr } => f
                .debug_struct("ExprStmt")
                .field("expr", &expr.debug_with(self.reg))
                .finish(),
            StmtKind::Block { stmts } => {
                let debug_stmts: Vec<_> =
                    stmts.iter().map(|stmt| stmt.debug_with(self.reg)).collect();

                f.debug_struct("Block")
                    .field("stmts", &debug_stmts)
                    .finish()
            }
        }
    }
}
