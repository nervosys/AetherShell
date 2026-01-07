// Minimal AST to satisfy eval.rs (and leave room for a parser later)
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Stmt {
    Let {
        name: String,
        value: Expr,
        /// `let mut` support
        is_mut: bool,
    },
    Expr(Expr),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Expr {
    // literals
    LitInt(i64),
    LitFloat(f64),
    LitStr(String),
    LitBool(bool),
    Null,

    // variables
    Ident(String),

    // collections
    Array(Vec<Expr>),
    Record(Vec<(String, Expr)>),

    // lambda
    Lambda {
        params: Vec<String>,
        body: Box<Expr>,
    },

    // call
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,            // positional
        named: Vec<(String, Expr)>, // named: key = expr
    },

    // pipeline: left | right (where right is typically a Call)
    Pipe {
        left: Box<Expr>,
        right: Box<Expr>,
    },

    // operators
    Binary {
        left: Box<Expr>,
        op: BinOp,
        right: Box<Expr>,
    },
    Unary {
        op: UnOp,
        expr: Box<Expr>,
    },

    // member access: record.field
    MemberAccess {
        object: Box<Expr>,
        field: String,
    },

    // pattern matching
    Match {
        scrutinee: Box<Expr>,
        arms: Vec<MatchArm>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub guard: Option<Box<Expr>>, // optional `if condition`
    pub body: Box<Expr>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Pattern {
    // Wildcard: _
    Wildcard,
    // Variable binding: x
    Ident(String),
    // Literal patterns
    LitInt(i64),
    LitStr(String),
    LitBool(bool),
    Null,
    // Constructor pattern: Some(x), None
    Constructor { name: String, args: Vec<Pattern> },
    // Array pattern: [a, b, c]
    Array(Vec<Pattern>),
    // Record pattern: {x, y} or {x: px, y: py}
    Record(Vec<(String, Pattern)>),
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize, PartialEq)]
pub enum UnOp {
    Neg,
    Not,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize, PartialEq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Pow,
    Eq,
    Ne,
    Lt,
    Lte,
    Gt,
    Gte,
    And,
    Or,
}
