//! ZeroScript 2: the scripting language of zero, grown up.
//!
//! It still runs the old programs' syntax (`let`, `if ... end`, `while`,
//! `sub`/`call`, `dim`, `print`) and adds what real programs need:
//!
//! * values: integers, strings, lists, `nil`;
//! * functions with parameters, local variables, recursion and `return value`;
//! * `for i = 1 to 10 step 2`, `for x in list`, `break`, `continue`, `elif`;
//! * `+=`, `-=`, `*=`, `/=`, `and`/`or`/`not` (and `&&`, `||`, `!`);
//! * a standard library (strings, lists, maths) and a host for everything
//!   else: the desktop gives scripts windows, drawing, the mouse and sound.

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::rc::Rc;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::cell::RefCell;

#[derive(Clone)]
pub enum Value {
    Nil,
    Int(i64),
    Str(Rc<str>),
    List(Rc<RefCell<Vec<Value>>>),
}

impl Value {
    pub fn int(&self) -> i64 {
        match self {
            Value::Int(n) => *n,
            Value::Str(s) => s.trim().parse().unwrap_or(0),
            Value::List(l) => l.borrow().len() as i64,
            Value::Nil => 0,
        }
    }

    pub fn truthy(&self) -> bool {
        match self {
            Value::Int(n) => *n != 0,
            Value::Str(s) => !s.is_empty(),
            Value::List(l) => !l.borrow().is_empty(),
            Value::Nil => false,
        }
    }

    pub fn str(s: &str) -> Value {
        Value::Str(Rc::from(s))
    }

    pub fn text(&self) -> String {
        match self {
            Value::Int(n) => n.to_string(),
            Value::Str(s) => String::from(&**s),
            Value::Nil => String::from("nil"),
            Value::List(l) => {
                let items: Vec<String> = l.borrow().iter().map(|v| match v {
                    Value::Str(s) => alloc::format!("\"{}\"", s),
                    other => other.text(),
                }).collect();
                alloc::format!("[{}]", items.join(", "))
            }
        }
    }

    fn eq(&self, o: &Value) -> bool {
        match (self, o) {
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::Str(a), Value::Str(b)) => a == b,
            (Value::Nil, Value::Nil) => true,
            (Value::List(a), Value::List(b)) => Rc::ptr_eq(a, b),
            (Value::Int(a), Value::Str(b)) | (Value::Str(b), Value::Int(a)) => b.chars().count() == 1 && b.chars().next().map(|c| c as i64) == Some(*a),
            _ => false,
        }
    }
}

pub struct Error {
    pub line: usize,
    pub message: String,
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        if self.line > 0 { write!(f, "line {}: {}", self.line, self.message) } else { write!(f, "{}", self.message) }
    }
}

fn err(line: usize, m: &str) -> Error {
    Error { line, message: String::from(m) }
}

// ---------------------------------------------------------------- lexer

#[derive(Clone, PartialEq, Debug)]
enum Tok {
    Num(i64),
    Str(String),
    Name(String),
    Op(&'static str),
    Newline,
}

const OPS: [&str; 28] = ["==", "!=", "<=", ">=", "&&", "||", "+=", "-=", "*=", "/=", "%=", "+", "-", "*", "/", "%", "<", ">", "=", "(", ")", "[", "]", ",", "!", ".", ":", ";"];

fn lex(src: &str) -> Result<Vec<(Tok, usize)>, Error> {
    let mut out = Vec::new();
    let chars: Vec<char> = src.chars().collect();
    let mut i = 0;
    let mut line = 1;
    while i < chars.len() {
        let c = chars[i];
        if c == '\n' {
            out.push((Tok::Newline, line));
            line += 1;
            i += 1;
            continue;
        }
        if c == '#' {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c.is_ascii_digit() {
            let start = i;
            if c == '0' && i + 1 < chars.len() && (chars[i + 1] == 'x' || chars[i + 1] == 'X') {
                i += 2;
                while i < chars.len() && chars[i].is_ascii_hexdigit() {
                    i += 1;
                }
                let s: String = chars[start + 2..i].iter().collect();
                out.push((Tok::Num(i64::from_str_radix(&s, 16).map_err(|_| err(line, "bad hex number"))?), line));
                continue;
            }
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '_') {
                i += 1;
            }
            let s: String = chars[start..i].iter().filter(|c| **c != '_').collect();
            out.push((Tok::Num(s.parse().map_err(|_| err(line, "number too big"))?), line));
            continue;
        }
        if c == '"' {
            i += 1;
            let mut s = String::new();
            while i < chars.len() && chars[i] != '"' {
                if chars[i] == '\n' {
                    return Err(err(line, "text is not closed with \""));
                }
                if chars[i] == '\\' && i + 1 < chars.len() {
                    i += 1;
                    s.push(match chars[i] {
                        'n' => '\n',
                        't' => '\t',
                        other => other,
                    });
                } else {
                    s.push(chars[i]);
                }
                i += 1;
            }
            i += 1;
            out.push((Tok::Str(s), line));
            continue;
        }
        if c == '\'' && i + 2 < chars.len() && chars[i + 2] == '\'' {
            out.push((Tok::Num(chars[i + 1] as i64), line));
            i += 3;
            continue;
        }
        if c.is_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            out.push((Tok::Name(chars[start..i].iter().collect()), line));
            continue;
        }
        let two: String = chars[i..(i + 2).min(chars.len())].iter().collect();
        if let Some(op) = OPS.iter().find(|o| o.len() == 2 && **o == two) {
            out.push((Tok::Op(op), line));
            i += 2;
            continue;
        }
        let one = c.to_string();
        if let Some(op) = OPS.iter().find(|o| o.len() == 1 && **o == one) {
            if *op == ";" {
                out.push((Tok::Newline, line));
            } else {
                out.push((Tok::Op(op), line));
            }
            i += 1;
            continue;
        }
        return Err(err(line, &alloc::format!("unexpected '{}'", c)));
    }
    out.push((Tok::Newline, line));
    Ok(out)
}

// ---------------------------------------------------------------- syntax tree

#[derive(Clone)]
enum Expr {
    Lit(Value),
    Var(Rc<str>),
    List(Vec<Expr>),
    Index(Box<Expr>, Box<Expr>),
    Call(Rc<str>, Vec<Expr>),
    Un(&'static str, Box<Expr>),
    Bin(&'static str, Box<Expr>, Box<Expr>),
}

#[derive(Clone)]
enum Stmt {
    Expr(Expr, usize),
    Assign(Rc<str>, Option<&'static str>, Expr, usize),
    SetIndex(Expr, Expr, Option<&'static str>, Expr, usize),
    If(Vec<(Expr, Vec<Stmt>)>, Vec<Stmt>, usize),
    While(Expr, Vec<Stmt>, usize),
    For(Rc<str>, Expr, Expr, Option<Expr>, Vec<Stmt>, usize),
    ForIn(Rc<str>, Expr, Vec<Stmt>, usize),
    Return(Option<Expr>, usize),
    Break,
    Continue,
    Exit,
    Print(Vec<Expr>, usize),
    Global(Vec<Rc<str>>),
}

struct Func {
    params: Vec<Rc<str>>,
    body: Rc<Vec<Stmt>>,
    /// Old style `sub`: no locals, everything is global.
    sub: bool,
}

struct Parser {
    toks: Vec<(Tok, usize)>,
    pos: usize,
    funcs: BTreeMap<String, Func>,
}

impl Parser {
    fn peek(&self) -> &Tok {
        &self.toks[self.pos.min(self.toks.len() - 1)].0
    }

    fn line(&self) -> usize {
        self.toks[self.pos.min(self.toks.len() - 1)].1
    }

    fn next(&mut self) -> Tok {
        let t = self.peek().clone();
        self.pos += 1;
        t
    }

    fn is_op(&self, op: &str) -> bool {
        matches!(self.peek(), Tok::Op(o) if *o == op)
    }

    fn is_name(&self, n: &str) -> bool {
        matches!(self.peek(), Tok::Name(x) if x == n)
    }

    fn expect_op(&mut self, op: &str) -> Result<(), Error> {
        if self.is_op(op) {
            self.pos += 1;
            Ok(())
        } else {
            Err(err(self.line(), &alloc::format!("expected '{}'", op)))
        }
    }

    fn name(&mut self) -> Result<Rc<str>, Error> {
        match self.next() {
            Tok::Name(n) => Ok(Rc::from(n.as_str())),
            _ => Err(err(self.line(), "expected a name")),
        }
    }

    fn skip_newlines(&mut self) {
        while matches!(self.peek(), Tok::Newline) && self.pos < self.toks.len() {
            self.pos += 1;
        }
    }

    fn end_of_statement(&mut self) -> Result<(), Error> {
        match self.peek() {
            Tok::Newline => {
                self.pos += 1;
                Ok(())
            }
            _ => Err(err(self.line(), "unexpected text after the statement")),
        }
    }

    /// Statements until one of the given words (not consumed).
    fn block(&mut self, until: &[&str]) -> Result<Vec<Stmt>, Error> {
        let mut out = Vec::new();
        loop {
            self.skip_newlines();
            if self.pos >= self.toks.len() - 1 {
                if until.is_empty() {
                    return Ok(out);
                }
                return Err(err(self.line(), &alloc::format!("missing '{}'", until[0])));
            }
            if let Tok::Name(n) = self.peek() {
                if until.contains(&n.as_str()) {
                    return Ok(out);
                }
            }
            if let Some(s) = self.statement()? {
                out.push(s);
            }
        }
    }

    fn statement(&mut self) -> Result<Option<Stmt>, Error> {
        let line = self.line();
        let word = match self.peek() {
            Tok::Name(n) => n.clone(),
            _ => String::new(),
        };
        match word.as_str() {
            "let" => {
                self.pos += 1;
                return self.statement();
            }
            "if" => {
                self.pos += 1;
                let mut arms = Vec::new();
                let cond = self.expr()?;
                self.skip_then();
                let body = self.block(&["elif", "else", "end"])?;
                arms.push((cond, body));
                let mut otherwise = Vec::new();
                loop {
                    if self.is_name("elif") {
                        self.pos += 1;
                        let c = self.expr()?;
                        self.skip_then();
                        let b = self.block(&["elif", "else", "end"])?;
                        arms.push((c, b));
                    } else if self.is_name("else") {
                        self.pos += 1;
                        if self.is_name("if") {
                            // `else if` on one line
                            self.pos += 1;
                            let c = self.expr()?;
                            self.skip_then();
                            let b = self.block(&["elif", "else", "end"])?;
                            arms.push((c, b));
                            continue;
                        }
                        otherwise = self.block(&["end"])?;
                    } else {
                        break;
                    }
                }
                self.pos += 1; // end
                return Ok(Some(Stmt::If(arms, otherwise, line)));
            }
            "while" => {
                self.pos += 1;
                let cond = self.expr()?;
                if self.is_name("do") {
                    self.pos += 1;
                }
                let body = self.block(&["end"])?;
                self.pos += 1;
                return Ok(Some(Stmt::While(cond, body, line)));
            }
            "for" => {
                self.pos += 1;
                let var = self.name()?;
                if self.is_name("in") {
                    self.pos += 1;
                    let list = self.expr()?;
                    let body = self.block(&["end"])?;
                    self.pos += 1;
                    return Ok(Some(Stmt::ForIn(var, list, body, line)));
                }
                self.expect_op("=")?;
                let from = self.expr()?;
                if !self.is_name("to") {
                    return Err(err(line, "expected 'to' in the for loop"));
                }
                self.pos += 1;
                let to = self.expr()?;
                let step = if self.is_name("step") {
                    self.pos += 1;
                    Some(self.expr()?)
                } else {
                    None
                };
                let body = self.block(&["end"])?;
                self.pos += 1;
                return Ok(Some(Stmt::For(var, from, to, step, body, line)));
            }
            "func" | "function" | "def" | "sub" => {
                self.pos += 1;
                let name = self.name()?;
                let mut params = Vec::new();
                if self.is_op("(") {
                    self.pos += 1;
                    while !self.is_op(")") {
                        params.push(self.name()?);
                        if self.is_op(",") {
                            self.pos += 1;
                        }
                    }
                    self.pos += 1;
                }
                let body = self.block(&["end"])?;
                self.pos += 1;
                self.funcs.insert(String::from(&*name), Func { params, body: Rc::new(body), sub: word == "sub" });
                return Ok(None);
            }
            "call" => {
                self.pos += 1;
                let name = self.name()?;
                let mut args = Vec::new();
                if self.is_op("(") {
                    args = self.args()?;
                }
                self.end_of_statement()?;
                return Ok(Some(Stmt::Expr(Expr::Call(name, args), line)));
            }
            "return" => {
                self.pos += 1;
                let v = if matches!(self.peek(), Tok::Newline) { None } else { Some(self.expr()?) };
                return Ok(Some(Stmt::Return(v, line)));
            }
            "break" => {
                self.pos += 1;
                return Ok(Some(Stmt::Break));
            }
            "continue" => {
                self.pos += 1;
                return Ok(Some(Stmt::Continue));
            }
            "exit" => {
                self.pos += 1;
                return Ok(Some(Stmt::Exit));
            }
            "global" => {
                self.pos += 1;
                let mut names = alloc::vec![self.name()?];
                while self.is_op(",") {
                    self.pos += 1;
                    names.push(self.name()?);
                }
                return Ok(Some(Stmt::Global(names)));
            }
            "print" => {
                self.pos += 1;
                let mut items = Vec::new();
                if !matches!(self.peek(), Tok::Newline) {
                    items.push(self.expr()?);
                    while self.is_op(",") {
                        self.pos += 1;
                        items.push(self.expr()?);
                    }
                }
                return Ok(Some(Stmt::Print(items, line)));
            }
            "dim" => {
                self.pos += 1;
                let name = self.name()?;
                self.expect_op("[")?;
                let n = self.expr()?;
                self.expect_op("]")?;
                return Ok(Some(Stmt::Assign(name, None, Expr::Call(Rc::from("__dim"), alloc::vec![n]), line)));
            }
            _ => {}
        }
        // assignment or expression
        let target = self.expr()?;
        let assign_op = match self.peek() {
            Tok::Op(o) if ["=", "+=", "-=", "*=", "/=", "%="].contains(o) => Some(*o),
            _ => None,
        };
        if let Some(op) = assign_op {
            self.pos += 1;
            let value = self.expr()?;
            let compound = if op == "=" { None } else { Some(&op[..1]) };
            let compound: Option<&'static str> = compound.map(|c| match c {
                "+" => "+",
                "-" => "-",
                "*" => "*",
                "/" => "/",
                _ => "%",
            });
            return match target {
                Expr::Var(n) => Ok(Some(Stmt::Assign(n, compound, value, line))),
                Expr::Index(list, idx) => Ok(Some(Stmt::SetIndex(*list, *idx, compound, value, line))),
                _ => Err(err(line, "can only assign to a variable or a list item")),
            };
        }
        // old style statements without parentheses: `cls`, `color 3`, `rect 1, 2, 3, 4`
        if let Expr::Var(n) = &target {
            if !matches!(self.peek(), Tok::Newline) {
                let mut args = alloc::vec![self.expr()?];
                while self.is_op(",") {
                    self.pos += 1;
                    args.push(self.expr()?);
                }
                return Ok(Some(Stmt::Expr(Expr::Call(n.clone(), args), line)));
            }
            return Ok(Some(Stmt::Expr(Expr::Call(n.clone(), Vec::new()), line)));
        }
        Ok(Some(Stmt::Expr(target, line)))
    }

    fn skip_then(&mut self) {
        if self.is_name("then") || self.is_op(":") {
            self.pos += 1;
        }
    }

    fn args(&mut self) -> Result<Vec<Expr>, Error> {
        self.expect_op("(")?;
        let mut args = Vec::new();
        while !self.is_op(")") {
            args.push(self.expr()?);
            if self.is_op(",") {
                self.pos += 1;
            } else if !self.is_op(")") {
                return Err(err(self.line(), "expected ',' or ')'"));
            }
        }
        self.pos += 1;
        Ok(args)
    }

    fn expr(&mut self) -> Result<Expr, Error> {
        self.or()
    }

    fn or(&mut self) -> Result<Expr, Error> {
        let mut l = self.and()?;
        while self.is_op("||") || self.is_name("or") {
            self.pos += 1;
            let r = self.and()?;
            l = Expr::Bin("or", Box::new(l), Box::new(r));
        }
        Ok(l)
    }

    fn and(&mut self) -> Result<Expr, Error> {
        let mut l = self.cmp()?;
        while self.is_op("&&") || self.is_name("and") {
            self.pos += 1;
            let r = self.cmp()?;
            l = Expr::Bin("and", Box::new(l), Box::new(r));
        }
        Ok(l)
    }

    fn cmp(&mut self) -> Result<Expr, Error> {
        let mut l = self.add()?;
        loop {
            let op = match self.peek() {
                Tok::Op(o) if ["==", "!=", "<", "<=", ">", ">="].contains(o) => *o,
                _ => break,
            };
            self.pos += 1;
            let r = self.add()?;
            l = Expr::Bin(op, Box::new(l), Box::new(r));
        }
        Ok(l)
    }

    fn add(&mut self) -> Result<Expr, Error> {
        let mut l = self.mul()?;
        loop {
            let op = match self.peek() {
                Tok::Op("+") => "+",
                Tok::Op("-") => "-",
                _ => break,
            };
            self.pos += 1;
            let r = self.mul()?;
            l = Expr::Bin(op, Box::new(l), Box::new(r));
        }
        Ok(l)
    }

    fn mul(&mut self) -> Result<Expr, Error> {
        let mut l = self.unary()?;
        loop {
            let op = match self.peek() {
                Tok::Op("*") => "*",
                Tok::Op("/") => "/",
                Tok::Op("%") => "%",
                _ => break,
            };
            self.pos += 1;
            let r = self.unary()?;
            l = Expr::Bin(op, Box::new(l), Box::new(r));
        }
        Ok(l)
    }

    fn unary(&mut self) -> Result<Expr, Error> {
        if self.is_op("-") {
            self.pos += 1;
            return Ok(Expr::Un("-", Box::new(self.unary()?)));
        }
        if self.is_op("!") || self.is_name("not") {
            self.pos += 1;
            return Ok(Expr::Un("!", Box::new(self.unary()?)));
        }
        self.postfix()
    }

    fn postfix(&mut self) -> Result<Expr, Error> {
        let mut e = self.atom()?;
        while self.is_op("[") {
            self.pos += 1;
            let i = self.expr()?;
            self.expect_op("]")?;
            e = Expr::Index(Box::new(e), Box::new(i));
        }
        Ok(e)
    }

    fn atom(&mut self) -> Result<Expr, Error> {
        let line = self.line();
        match self.next() {
            Tok::Num(n) => Ok(Expr::Lit(Value::Int(n))),
            Tok::Str(s) => Ok(Expr::Lit(Value::str(&s))),
            Tok::Op("(") => {
                let e = self.expr()?;
                self.expect_op(")")?;
                Ok(e)
            }
            Tok::Op("[") => {
                let mut items = Vec::new();
                while !self.is_op("]") {
                    self.skip_newlines();
                    items.push(self.expr()?);
                    self.skip_newlines();
                    if self.is_op(",") {
                        self.pos += 1;
                    }
                    self.skip_newlines();
                }
                self.pos += 1;
                Ok(Expr::List(items))
            }
            Tok::Name(n) => match n.as_str() {
                "true" => Ok(Expr::Lit(Value::Int(1))),
                "false" => Ok(Expr::Lit(Value::Int(0))),
                "nil" => Ok(Expr::Lit(Value::Nil)),
                _ => {
                    if self.is_op("(") {
                        let args = self.args()?;
                        Ok(Expr::Call(Rc::from(n.as_str()), args))
                    } else {
                        Ok(Expr::Var(Rc::from(n.as_str())))
                    }
                }
            },
            _ => Err(err(line, "expected a value")),
        }
    }
}

// ---------------------------------------------------------------- running

/// What a script can ask of the world around it.
pub trait Host {
    /// A function the language does not know itself (drawing, input, ...).
    /// Returns `None` if the host does not know it either.
    fn call(&mut self, name: &str, args: &[Value]) -> Option<Result<Value, String>>;
    fn print(&mut self, text: &str);
    /// Called often: return true to stop the script.
    fn should_stop(&mut self) -> bool;
}

enum Flow {
    Normal,
    Break,
    Continue,
    Return(Value),
    Exit,
}

pub struct Program {
    main: Vec<Stmt>,
    funcs: BTreeMap<String, Func>,
}

pub fn compile(src: &str) -> Result<Program, Error> {
    let toks = lex(src)?;
    let mut p = Parser { toks, pos: 0, funcs: BTreeMap::new() };
    let main = p.block(&[])?;
    Ok(Program { main, funcs: p.funcs })
}

struct Frame {
    locals: BTreeMap<Rc<str>, Value>,
    globals_declared: Vec<Rc<str>>,
    sub: bool,
}

pub struct Vm<'a> {
    prog: &'a Program,
    globals: BTreeMap<Rc<str>, Value>,
    frames: Vec<Frame>,
    rng: u64,
    steps: u32,
    line: usize,
}

const CONSTANTS: [(&str, i64); 9] = [("UP", 256), ("DOWN", 257), ("LEFT", 258), ("RIGHT", 259), ("ENTER", 10), ("ESC", 27), ("SPACE", 32), ("WIDTH", 80), ("HEIGHT", 25)];

pub fn run(prog: &Program, host: &mut dyn Host) -> Result<(), Error> {
    let mut vm = Vm { prog, globals: BTreeMap::new(), frames: Vec::new(), rng: 0x9E37_79B9_7F4A_7C15 ^ crate::time::uptime_ms(), steps: 0, line: 0 };
    for (n, v) in CONSTANTS {
        vm.globals.insert(Rc::from(n), Value::Int(v));
    }
    match vm.block(&prog.main, host) {
        Ok(_) => Ok(()),
        Err(e) => Err(e),
    }
}

impl<'a> Vm<'a> {
    fn e(&self, m: &str) -> Error {
        err(self.line, m)
    }

    fn rand(&mut self) -> u64 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        self.rng
    }

    fn get(&self, name: &Rc<str>) -> Value {
        if let Some(f) = self.frames.last() {
            if !f.sub {
                if let Some(v) = f.locals.get(name) {
                    return v.clone();
                }
            }
        }
        self.globals.get(name).cloned().unwrap_or(Value::Nil)
    }

    fn set(&mut self, name: &Rc<str>, v: Value) {
        if let Some(f) = self.frames.last_mut() {
            if !f.sub && !f.globals_declared.contains(name) {
                f.locals.insert(name.clone(), v);
                return;
            }
        }
        self.globals.insert(name.clone(), v);
    }

    fn block(&mut self, stmts: &[Stmt], host: &mut dyn Host) -> Result<Flow, Error> {
        for s in stmts {
            match self.stmt(s, host)? {
                Flow::Normal => {}
                other => return Ok(other),
            }
        }
        Ok(Flow::Normal)
    }

    fn tick(&mut self, host: &mut dyn Host) -> Result<(), Error> {
        self.steps += 1;
        if self.steps % 256 == 0 {
            if host.should_stop() {
                return Err(self.e("stopped"));
            }
            crate::sched::yield_if_due();
        }
        Ok(())
    }

    fn stmt(&mut self, s: &Stmt, host: &mut dyn Host) -> Result<Flow, Error> {
        self.tick(host)?;
        match s {
            Stmt::Expr(e, line) => {
                self.line = *line;
                self.eval(e, host)?;
            }
            Stmt::Assign(name, op, e, line) => {
                self.line = *line;
                let v = self.eval(e, host)?;
                let v = match op {
                    Some(o) => {
                        let cur = self.get(name);
                        self.binop(o, cur, v)?
                    }
                    None => v,
                };
                self.set(name, v);
            }
            Stmt::SetIndex(list, idx, op, e, line) => {
                self.line = *line;
                let l = self.eval(list, host)?;
                let i = self.eval(idx, host)?.int();
                let v = self.eval(e, host)?;
                let Value::List(l) = l else { return Err(self.e("only lists have items")) };
                let mut l = l.borrow_mut();
                let n = l.len() as i64;
                let i = if i < 0 { i + n } else { i };
                if i < 0 || i >= n {
                    return Err(self.e(&alloc::format!("list index {} is out of range (the list has {} items)", i, n)));
                }
                let nv = match op {
                    Some(o) => self.binop(o, l[i as usize].clone(), v)?,
                    None => v,
                };
                l[i as usize] = nv;
            }
            Stmt::If(arms, otherwise, line) => {
                self.line = *line;
                for (c, body) in arms {
                    if self.eval(c, host)?.truthy() {
                        return self.block(body, host);
                    }
                }
                return self.block(otherwise, host);
            }
            Stmt::While(c, body, line) => loop {
                self.line = *line;
                if !self.eval(c, host)?.truthy() {
                    break;
                }
                match self.block(body, host)? {
                    Flow::Break => break,
                    Flow::Return(v) => return Ok(Flow::Return(v)),
                    Flow::Exit => return Ok(Flow::Exit),
                    _ => {}
                }
                self.tick(host)?;
            },
            Stmt::For(var, from, to, step, body, line) => {
                self.line = *line;
                let mut i = self.eval(from, host)?.int();
                let end = self.eval(to, host)?.int();
                let st = match step {
                    Some(s) => self.eval(s, host)?.int(),
                    None => 1,
                };
                if st == 0 {
                    return Err(self.e("step cannot be 0"));
                }
                while (st > 0 && i <= end) || (st < 0 && i >= end) {
                    self.set(var, Value::Int(i));
                    match self.block(body, host)? {
                        Flow::Break => break,
                        Flow::Return(v) => return Ok(Flow::Return(v)),
                        Flow::Exit => return Ok(Flow::Exit),
                        _ => {}
                    }
                    i += st;
                    self.tick(host)?;
                }
            }
            Stmt::ForIn(var, list, body, line) => {
                self.line = *line;
                let items: Vec<Value> = match self.eval(list, host)? {
                    Value::List(l) => l.borrow().clone(),
                    Value::Str(s) => s.chars().map(|c| Value::str(&c.to_string())).collect(),
                    Value::Int(n) => (0..n).map(Value::Int).collect(),
                    Value::Nil => Vec::new(),
                };
                for it in items {
                    self.set(var, it);
                    match self.block(body, host)? {
                        Flow::Break => break,
                        Flow::Return(v) => return Ok(Flow::Return(v)),
                        Flow::Exit => return Ok(Flow::Exit),
                        _ => {}
                    }
                }
            }
            Stmt::Return(v, line) => {
                self.line = *line;
                let v = match v {
                    Some(e) => self.eval(e, host)?,
                    None => Value::Nil,
                };
                return Ok(Flow::Return(v));
            }
            Stmt::Break => return Ok(Flow::Break),
            Stmt::Continue => return Ok(Flow::Continue),
            Stmt::Exit => return Ok(Flow::Exit),
            Stmt::Print(items, line) => {
                self.line = *line;
                let mut s = String::new();
                for it in items {
                    s.push_str(&self.eval(it, host)?.text());
                }
                host.print(&s);
            }
            Stmt::Global(names) => {
                if let Some(f) = self.frames.last_mut() {
                    f.globals_declared.extend(names.iter().cloned());
                }
            }
        }
        Ok(Flow::Normal)
    }

    fn binop(&self, op: &str, a: Value, b: Value) -> Result<Value, Error> {
        Ok(match (op, &a, &b) {
            ("+", Value::Str(_), _) | ("+", _, Value::Str(_)) => Value::str(&alloc::format!("{}{}", a.text(), b.text())),
            ("+", Value::List(x), Value::List(y)) => {
                let mut v = x.borrow().clone();
                v.extend(y.borrow().iter().cloned());
                Value::List(Rc::new(RefCell::new(v)))
            }
            ("*", Value::Str(s), Value::Int(n)) => Value::str(&s.repeat((*n).clamp(0, 10000) as usize)),
            ("==", _, _) => Value::Int(a.eq(&b) as i64),
            ("!=", _, _) => Value::Int(!a.eq(&b) as i64),
            ("<" | "<=" | ">" | ">=", Value::Str(x), Value::Str(y)) => {
                let c = x.cmp(y);
                Value::Int(match op {
                    "<" => c.is_lt(),
                    "<=" => c.is_le(),
                    ">" => c.is_gt(),
                    _ => c.is_ge(),
                } as i64)
            }
            _ => {
                let (x, y) = (a.int(), b.int());
                Value::Int(match op {
                    "+" => x.wrapping_add(y),
                    "-" => x.wrapping_sub(y),
                    "*" => x.wrapping_mul(y),
                    "/" => {
                        if y == 0 {
                            return Err(self.e("division by zero"));
                        }
                        x.wrapping_div(y)
                    }
                    "%" => {
                        if y == 0 {
                            return Err(self.e("division by zero"));
                        }
                        x.wrapping_rem(y)
                    }
                    "<" => (x < y) as i64,
                    "<=" => (x <= y) as i64,
                    ">" => (x > y) as i64,
                    ">=" => (x >= y) as i64,
                    _ => return Err(self.e("unknown operator")),
                })
            }
        })
    }

    fn eval(&mut self, e: &Expr, host: &mut dyn Host) -> Result<Value, Error> {
        Ok(match e {
            Expr::Lit(v) => v.clone(),
            Expr::Var(n) => self.get(n),
            Expr::List(items) => {
                let mut v = Vec::with_capacity(items.len());
                for it in items {
                    v.push(self.eval(it, host)?);
                }
                Value::List(Rc::new(RefCell::new(v)))
            }
            Expr::Index(l, i) => {
                let l = self.eval(l, host)?;
                let i = self.eval(i, host)?.int();
                match l {
                    Value::List(l) => {
                        let l = l.borrow();
                        let n = l.len() as i64;
                        let k = if i < 0 { i + n } else { i };
                        if k < 0 || k >= n {
                            return Err(self.e(&alloc::format!("list index {} is out of range (the list has {} items)", i, n)));
                        }
                        l[k as usize].clone()
                    }
                    Value::Str(s) => s.chars().nth(i.max(0) as usize).map(|c| Value::str(&c.to_string())).unwrap_or(Value::str("")),
                    _ => return Err(self.e("only lists and text can be indexed")),
                }
            }
            Expr::Un(op, x) => {
                let v = self.eval(x, host)?;
                match *op {
                    "-" => Value::Int(v.int().wrapping_neg()),
                    _ => Value::Int(!v.truthy() as i64),
                }
            }
            Expr::Bin(op, a, b) => match *op {
                "and" => {
                    let l = self.eval(a, host)?;
                    if !l.truthy() { Value::Int(0) } else { Value::Int(self.eval(b, host)?.truthy() as i64) }
                }
                "or" => {
                    let l = self.eval(a, host)?;
                    if l.truthy() { Value::Int(1) } else { Value::Int(self.eval(b, host)?.truthy() as i64) }
                }
                _ => {
                    let l = self.eval(a, host)?;
                    let r = self.eval(b, host)?;
                    self.binop(op, l, r)?
                }
            },
            Expr::Call(name, args) => {
                let mut vals = Vec::with_capacity(args.len());
                for a in args {
                    vals.push(self.eval(a, host)?);
                }
                self.call(name, vals, host)?
            }
        })
    }

    fn call(&mut self, name: &str, args: Vec<Value>, host: &mut dyn Host) -> Result<Value, Error> {
        if let Some(f) = self.prog.funcs.get(name) {
            if self.frames.len() > 200 {
                return Err(self.e("too much recursion"));
            }
            let mut frame = Frame { locals: BTreeMap::new(), globals_declared: Vec::new(), sub: f.sub };
            for (k, p) in f.params.iter().enumerate() {
                frame.locals.insert(p.clone(), args.get(k).cloned().unwrap_or(Value::Nil));
            }
            let body = f.body.clone();
            let line = self.line;
            self.frames.push(frame);
            let r = self.block(&body, host);
            self.frames.pop();
            self.line = line;
            return match r? {
                Flow::Return(v) => Ok(v),
                Flow::Exit => Err(err(0, "exit")),
                _ => Ok(Value::Nil),
            };
        }
        let a = |k: usize| args.get(k).cloned().unwrap_or(Value::Nil);
        let n = |k: usize| args.get(k).map(|v| v.int()).unwrap_or(0);
        Ok(match name {
            "__dim" => Value::List(Rc::new(RefCell::new(alloc::vec![Value::Int(0); n(0).clamp(0, 1_000_000) as usize]))),
            "len" => Value::Int(match a(0) {
                Value::Str(s) => s.chars().count() as i64,
                Value::List(l) => l.borrow().len() as i64,
                _ => 0,
            }),
            "str" => Value::str(&a(0).text()),
            "num" | "int" => Value::Int(a(0).int()),
            "chr" => Value::str(&char::from_u32(n(0) as u32).unwrap_or('?').to_string()),
            "ord" => Value::Int(a(0).text().chars().next().map(|c| c as i64).unwrap_or(0)),
            "upper" => Value::str(&a(0).text().to_ascii_uppercase()),
            "lower" => Value::str(&a(0).text().to_ascii_lowercase()),
            "trim" => Value::str(a(0).text().trim()),
            "sub" | "substr" => {
                let s: Vec<char> = a(0).text().chars().collect();
                let start = (n(1).max(0) as usize).min(s.len());
                let len = if args.len() > 2 { n(2).max(0) as usize } else { s.len() };
                Value::str(&s[start..(start + len).min(s.len())].iter().collect::<String>())
            }
            "find" => Value::Int(a(0).text().find(&*a(1).text()).map(|i| i as i64).unwrap_or(-1)),
            "split" => {
                let sep = if args.len() > 1 { a(1).text() } else { String::from(" ") };
                let parts: Vec<Value> = a(0).text().split(sep.as_str()).map(Value::str).collect();
                Value::List(Rc::new(RefCell::new(parts)))
            }
            "join" => {
                let sep = a(1).text();
                match a(0) {
                    Value::List(l) => Value::str(&l.borrow().iter().map(|v| v.text()).collect::<Vec<_>>().join(&sep)),
                    other => Value::str(&other.text()),
                }
            }
            "list" | "range" => {
                let (from, to) = if args.len() >= 2 { (n(0), n(1)) } else { (0, n(0)) };
                Value::List(Rc::new(RefCell::new((from..to.min(from + 1_000_000)).map(Value::Int).collect())))
            }
            "push" | "append" => {
                if let Value::List(l) = a(0) {
                    l.borrow_mut().push(a(1));
                }
                Value::Nil
            }
            "pop" => match a(0) {
                Value::List(l) => l.borrow_mut().pop().unwrap_or(Value::Nil),
                _ => Value::Nil,
            },
            "insert" => {
                if let Value::List(l) = a(0) {
                    let mut l = l.borrow_mut();
                    let i = (n(1).max(0) as usize).min(l.len());
                    l.insert(i, a(2));
                }
                Value::Nil
            }
            "remove" => match a(0) {
                Value::List(l) => {
                    let mut l = l.borrow_mut();
                    let i = n(1);
                    if i >= 0 && (i as usize) < l.len() {
                        l.remove(i as usize)
                    } else {
                        Value::Nil
                    }
                }
                _ => Value::Nil,
            },
            "copy" => match a(0) {
                Value::List(l) => Value::List(Rc::new(RefCell::new(l.borrow().clone()))),
                other => other,
            },
            "contains" => Value::Int(match a(0) {
                Value::List(l) => l.borrow().iter().any(|v| v.eq(&a(1))),
                Value::Str(s) => s.contains(&*a(1).text()),
                _ => false,
            } as i64),
            "abs" => Value::Int(n(0).wrapping_abs()),
            "min" => Value::Int(n(0).min(n(1))),
            "max" => Value::Int(n(0).max(n(1))),
            "clamp" => Value::Int(n(0).clamp(n(1).min(n(2)), n(2).max(n(1)))),
            "sign" => Value::Int(n(0).signum()),
            "sqrt" => Value::Int(isqrt(n(0))),
            "sin" => Value::Int(sin1000(n(0))),
            "cos" => Value::Int(sin1000(n(0) + 90)),
            "rand" => {
                let lim = if args.is_empty() { 100 } else { n(0) };
                if lim <= 0 {
                    return Err(self.e("rand needs a positive limit"));
                }
                Value::Int((self.rand() % lim as u64) as i64)
            }
            "ticks" => Value::Int(crate::time::uptime_ms() as i64),
            "type" => Value::str(match a(0) {
                Value::Int(_) => "number",
                Value::Str(_) => "text",
                Value::List(_) => "list",
                Value::Nil => "nil",
            }),
            _ => match host.call(name, &args) {
                Some(Ok(v)) => v,
                Some(Err(m)) => return Err(self.e(&m)),
                None => return Err(self.e(&alloc::format!("there is no function called '{}'", name))),
            },
        })
    }
}

fn isqrt(n: i64) -> i64 {
    if n <= 0 {
        return 0;
    }
    let mut x = n;
    let mut y = (x + 1) / 2;
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
}

/// sin of degrees, times 1000.
pub fn sin1000(d: i64) -> i64 {
    let d = d.rem_euclid(360);
    let (d, s) = if d > 180 { (d - 180, -1) } else { (d, 1) };
    s * 4000 * d * (180 - d) / (40500 - d * (180 - d))
}
