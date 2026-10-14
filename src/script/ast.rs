//! The compiled form of a script.

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;

/// A binary operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

/// A built-in function.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Func {
    Rand,
    Abs,
    Min,
    Max,
    Key,
    GetKey,
    Ticks,
}

/// An expression. Variables are resolved to slot numbers while compiling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    Num(i32),
    Var(usize),
    Neg(Box<Expr>),
    Not(Box<Expr>),
    Bin(BinOp, Box<Expr>, Box<Expr>),
    Call(Func, Vec<Expr>),
}
