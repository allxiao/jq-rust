//! Abstract Syntax Tree for jq filter expressions

use super::token::Span;

/// A complete jq filter program
#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    /// Module metadata (if present)
    pub module: Option<Expr>,
    /// Import statements
    pub imports: Vec<Import>,
    /// Function definitions
    pub defs: Vec<FuncDef>,
    /// Main query expression
    pub query: Expr,
}

/// An import statement
#[derive(Debug, Clone, PartialEq)]
pub struct Import {
    /// Path to import
    pub path: String,
    /// Alias for the import
    pub alias: Option<String>,
    /// Whether this is a data import (import as $var)
    pub is_data: bool,
    /// Import metadata
    pub metadata: Option<Expr>,
    pub span: Span,
}

/// A function definition
#[derive(Debug, Clone, PartialEq)]
pub struct FuncDef {
    /// Function name
    pub name: String,
    /// Parameters (can be values or filter arguments)
    pub params: Vec<Param>,
    /// Function body
    pub body: Box<Expr>,
    pub span: Span,
}

/// A function parameter
#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    /// Parameter name
    pub name: String,
    /// Whether this is a binding ($var) or filter arg (name)
    pub is_binding: bool,
    pub span: Span,
}

/// Expression node
#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

impl Expr {
    pub fn new(kind: ExprKind, span: Span) -> Self {
        Expr { kind, span }
    }

    /// Create an identity expression
    pub fn identity(span: Span) -> Self {
        Expr::new(ExprKind::Identity, span)
    }

    /// Create a literal expression
    pub fn literal(value: Literal, span: Span) -> Self {
        Expr::new(ExprKind::Literal(value), span)
    }
}

/// Expression variants
#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    /// Identity: .
    Identity,

    /// Recursive descent: ..
    RecursiveDescent,

    /// Literal value: null, true, false, numbers, strings
    Literal(Literal),

    /// Field access: .foo
    Field(String),

    /// Index expression: .[expr]
    Index {
        expr: Box<Expr>,
        index: Box<Expr>,
        optional: bool,
    },

    /// Slice expression: .[start:end]
    Slice {
        expr: Box<Expr>,
        start: Option<Box<Expr>>,
        end: Option<Box<Expr>>,
        optional: bool,
    },

    /// Iterator: .[]
    Iterator {
        expr: Box<Expr>,
        optional: bool,
    },

    /// Pipe: expr | expr
    Pipe(Box<Expr>, Box<Expr>),

    /// Comma: expr, expr
    Comma(Box<Expr>, Box<Expr>),

    /// Conditional: if cond then then_branch else else_branch end
    Conditional {
        condition: Box<Expr>,
        then_branch: Box<Expr>,
        else_branch: Option<Box<Expr>>,
    },

    /// Try-catch: try expr catch handler
    TryCatch {
        expr: Box<Expr>,
        catch: Option<Box<Expr>>,
    },

    /// Reduce: reduce expr as $var (init; update)
    Reduce {
        expr: Box<Expr>,
        var: String,
        init: Box<Expr>,
        update: Box<Expr>,
    },

    /// Foreach: foreach expr as $var (init; update; extract)
    Foreach {
        expr: Box<Expr>,
        var: String,
        init: Box<Expr>,
        update: Box<Expr>,
        extract: Option<Box<Expr>>,
    },

    /// Function call: name or name(args)
    FunctionCall {
        name: String,
        args: Vec<Expr>,
    },

    /// Variable reference: $var
    Variable(String),

    /// Negation: -expr
    Negate(Box<Expr>),

    /// Optional: expr?
    Optional(Box<Expr>),

    /// Binary operation
    BinaryOp {
        op: BinaryOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },

    /// Array construction: [expr]
    Array(Option<Box<Expr>>),

    /// Object construction: {key: value, ...}
    Object(Vec<ObjectEntry>),

    /// Binding: expr as $var | body
    Binding {
        expr: Box<Expr>,
        pattern: Pattern,
        body: Box<Expr>,
    },

    /// Label: label $name | expr
    Label {
        name: String,
        body: Box<Expr>,
    },

    /// Break: break $name
    Break(String),

    /// $__loc__
    Loc,

    /// String with interpolation
    StringInterp(Vec<StringPart>),

    /// Format: @base64, @uri, etc.
    Format {
        format: String,
        expr: Option<Box<Expr>>,
    },

    /// Update assignment: expr |= expr
    Update {
        target: Box<Expr>,
        value: Box<Expr>,
    },

    /// Assignment with operator: expr += expr, etc.
    UpdateOp {
        op: BinaryOp,
        target: Box<Expr>,
        value: Box<Expr>,
    },

    /// Assignment: expr = expr
    Assign {
        target: Box<Expr>,
        value: Box<Expr>,
    },

    /// Alternative: expr // expr
    Alternative(Box<Expr>, Box<Expr>),

    /// Function definition (local)
    LocalDef {
        def: FuncDef,
        body: Box<Expr>,
    },

    /// Parenthesized expression
    Paren(Box<Expr>),
}

/// Literal values
#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
}

/// Binary operators
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    // Arithmetic
    Add,      // +
    Sub,      // -
    Mul,      // *
    Div,      // /
    Mod,      // %

    // Comparison
    Eq,       // ==
    Ne,       // !=
    Lt,       // <
    Le,       // <=
    Gt,       // >
    Ge,       // >=

    // Logical
    And,      // and
    Or,       // or
}

impl BinaryOp {
    /// Get precedence (higher = binds tighter)
    pub fn precedence(&self) -> u8 {
        match self {
            BinaryOp::Or => 1,
            BinaryOp::And => 2,
            BinaryOp::Eq | BinaryOp::Ne | BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge => 3,
            BinaryOp::Add | BinaryOp::Sub => 4,
            BinaryOp::Mul | BinaryOp::Div | BinaryOp::Mod => 5,
        }
    }

    /// Check if operator is left-associative
    pub fn is_left_assoc(&self) -> bool {
        true // All binary ops are left-associative
    }
}

/// An entry in an object literal
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectEntry {
    /// Key expression
    pub key: ObjectKey,
    /// Value expression
    pub value: Box<Expr>,
    pub span: Span,
}

/// Object key types
#[derive(Debug, Clone, PartialEq)]
pub enum ObjectKey {
    /// Identifier key: {foo: ...}
    Ident(String),
    /// String key: {"foo": ...}
    String(String),
    /// Expression key: {(expr): ...}
    Expr(Box<Expr>),
    /// Shorthand: {foo} (equivalent to {foo: .foo})
    Shorthand(String),
}

/// Pattern for destructuring
#[derive(Debug, Clone, PartialEq)]
pub struct Pattern {
    pub kind: PatternKind,
    pub span: Span,
}

/// Pattern variants
#[derive(Debug, Clone, PartialEq)]
pub enum PatternKind {
    /// Simple binding: $var
    Binding(String),
    /// Array pattern: [$a, $b]
    Array(Vec<Pattern>),
    /// Object pattern: {foo: $a, bar: $b}
    Object(Vec<(ObjectKey, Pattern)>),
}

/// Part of a string with interpolation
#[derive(Debug, Clone, PartialEq)]
pub enum StringPart {
    /// Literal text
    Text(String),
    /// Interpolated expression
    Interp(Box<Expr>),
}

impl Default for Expr {
    fn default() -> Self {
        Expr {
            kind: ExprKind::Identity,
            span: Span::default(),
        }
    }
}
