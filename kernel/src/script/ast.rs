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
    /// `down(key)`: 1 while the key is held down.
    Down,
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
    /// `array[index]`
    Index(usize, Box<Expr>),
}

/// One piece of what `print` and `text` output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    Text(String),
    Value(Expr),
}

/// A statement. Jump targets are indices into `Program::stmts`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stmt {
    /// `name = expr`
    Assign(usize, Expr),
    /// `print item, item, ...`
    Print(Vec<Item>),
    /// `text x, y, item, ...`
    Text(Expr, Expr, Vec<Item>),
    /// `plot x, y, char`
    Plot(Expr, Expr, Expr),
    /// `array[index] = value`
    AssignIndex(usize, Expr, Expr),
    /// `emote n`: tells the buddy how the game is going (1 score, 2 hurt, 3 win, 4 danger, 5 lose)
    Emote(Expr),
    /// `rect x, y, w, h`: a filled rectangle in pixels (the screen is 640x400)
    Rect(Expr, Expr, Expr, Expr),
    /// `disc x, y, r`: a filled circle in pixels
    Disc(Expr, Expr, Expr),
    Cls,
    Color(Expr),
    Sleep(Expr),
    /// `if cond`: jumps to `otherwise` when the condition is zero.
    If { cond: Expr, otherwise: usize },
    /// `else`: reached at the end of the `if` branch, skips to the `end`.
    Else { end: usize },
    EndIf,
    /// `while cond`: jumps to `end` (just past the loop) when the condition is zero.
    While { cond: Expr, end: usize },
    /// `end` of a loop: jumps back to the `while`.
    EndWhile { start: usize },
    /// `sub name`: when run in order, skips over the body.
    Sub { end: usize },
    /// `end` of a sub: returns to the caller.
    EndSub,
    /// `call name`: jumps to the first statement of the sub.
    Call(usize),
    Return,
    Exit,
}

/// A compiled script.
#[derive(Debug)]
pub struct Program {
    pub stmts: Vec<Stmt>,
    /// The source line of every statement, for error messages.
    pub lines: Vec<usize>,
    /// Variable names; the position is the variable's slot.
    pub names: Vec<String>,
    /// Initial values of the first slots (the predefined constants).
    pub constants: Vec<i32>,
    /// Arrays: name and length.
    pub arrays: Vec<(String, usize)>,
}
