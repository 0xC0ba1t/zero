//! Compiles tokens into a `Program`.

use super::ast::*;
use super::lexer::{tokenize, Spanned, Token};
use super::ScriptError;
use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

/// Variables that exist before the script starts. They take the first slots.
const CONSTANTS: [(&str, i32); 6] = [
    ("UP", 256),
    ("DOWN", 257),
    ("ENTER", 10),
    ("ESC", 27),
    ("WIDTH", 80),
    ("HEIGHT", 25),
];

/// Words that cannot be used as variable names.
const KEYWORDS: [&str; 15] = [
    "let", "print", "text", "plot", "cls", "color", "sleep", "if", "else", "while", "end", "sub",
    "call", "return", "exit",
];

fn is_keyword(name: &str) -> bool {
    KEYWORDS.iter().any(|keyword| *keyword == name)
}

/// A block that has been opened but not closed with `end` yet.
enum Block {
    If { stmt: usize, else_stmt: Option<usize> },
    While { stmt: usize },
    Sub { stmt: usize },
}

struct Compiler {
    /// variable names; the position is the slot
    names: Vec<String>,
    stmts: Vec<Stmt>,
    /// the source line of every statement
    lines: Vec<usize>,
    blocks: Vec<Block>,
    /// every sub with the index of its first statement
    subs: Vec<(String, usize)>,
    /// `call` statements waiting for their sub: (statement, name, line)
    calls: Vec<(usize, String, usize)>,
    /// the statement being compiled
    toks: Vec<Spanned>,
    pos: usize,
    line: usize,
}

impl Compiler {
    fn new() -> Compiler {
        let mut compiler = Compiler {
            names: Vec::new(),
            stmts: Vec::new(),
            lines: Vec::new(),
            blocks: Vec::new(),
            subs: Vec::new(),
            calls: Vec::new(),
            toks: Vec::new(),
            pos: 0,
            line: 0,
        };
        for (name, _) in CONSTANTS.iter() {
            compiler.names.push(String::from(*name));
        }
        compiler
    }

    fn error(&self, message: &str) -> ScriptError {
        ScriptError::new(self.line, message)
    }

    /// Appends a statement and returns its index.
    fn emit(&mut self, stmt: Stmt) -> usize {
        self.stmts.push(stmt);
        self.lines.push(self.line);
        self.stmts.len() - 1
    }

    /// Returns the slot of a variable, creating it on first use.
    fn intern(&mut self, name: &str) -> usize {
        if let Some(slot) = self.names.iter().position(|existing| existing == name) {
            return slot;
        }
        self.names.push(String::from(name));
        self.names.len() - 1
    }

    fn peek(&self) -> Option<&Token> {
        self.toks.get(self.pos).map(|spanned| &spanned.token)
    }

    /// Consumes the next token if it is `token`.
    fn eat(&mut self, token: &Token) -> bool {
        if self.peek() == Some(token) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn expect(&mut self, token: &Token, what: &str) -> Result<(), ScriptError> {
        if self.eat(token) {
            Ok(())
        } else {
            Err(self.error(&format!("expected {}", what)))
        }
    }

    fn expect_ident(&mut self, what: &str) -> Result<String, ScriptError> {
        let next = self.peek().cloned();
        match next {
            Some(Token::Ident(name)) => {
                self.pos += 1;
                Ok(name)
            }
            _ => Err(self.error(&format!("expected {}", what))),
        }
    }

    /// Fails if there is anything left on the line.
    fn end_of_statement(&self) -> Result<(), ScriptError> {
        if self.pos < self.toks.len() {
            Err(self.error("unexpected extra text"))
        } else {
            Ok(())
        }
    }

    // ---- expressions, from the loosest to the tightest binding operator ----

    fn expr(&mut self) -> Result<Expr, ScriptError> {
        self.or_expr()
    }

    fn or_expr(&mut self) -> Result<Expr, ScriptError> {
        let mut left = self.and_expr()?;
        while self.eat(&Token::OrOr) {
            let right = self.and_expr()?;
            left = Expr::Bin(BinOp::Or, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn and_expr(&mut self) -> Result<Expr, ScriptError> {
        let mut left = self.comparison()?;
        while self.eat(&Token::AndAnd) {
            let right = self.comparison()?;
            left = Expr::Bin(BinOp::And, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn comparison(&mut self) -> Result<Expr, ScriptError> {
        let mut left = self.additive()?;
        loop {
            let op = match self.peek() {
                Some(Token::EqEq) => BinOp::Eq,
                Some(Token::NotEq) => BinOp::Ne,
                Some(Token::Lt) => BinOp::Lt,
                Some(Token::Le) => BinOp::Le,
                Some(Token::Gt) => BinOp::Gt,
                Some(Token::Ge) => BinOp::Ge,
                _ => break,
            };
            self.pos += 1;
            let right = self.additive()?;
            left = Expr::Bin(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn additive(&mut self) -> Result<Expr, ScriptError> {
        let mut left = self.multiplicative()?;
        loop {
            let op = match self.peek() {
                Some(Token::Plus) => BinOp::Add,
                Some(Token::Minus) => BinOp::Sub,
                _ => break,
            };
            self.pos += 1;
            let right = self.multiplicative()?;
            left = Expr::Bin(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn multiplicative(&mut self) -> Result<Expr, ScriptError> {
        let mut left = self.unary()?;
        loop {
            let op = match self.peek() {
                Some(Token::Star) => BinOp::Mul,
                Some(Token::Slash) => BinOp::Div,
                Some(Token::Percent) => BinOp::Rem,
                _ => break,
            };
            self.pos += 1;
            let right = self.unary()?;
            left = Expr::Bin(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn unary(&mut self) -> Result<Expr, ScriptError> {
        if self.eat(&Token::Minus) {
            return Ok(Expr::Neg(Box::new(self.unary()?)));
        }
        if self.eat(&Token::Bang) {
            return Ok(Expr::Not(Box::new(self.unary()?)));
        }
        self.primary()
    }

    fn primary(&mut self) -> Result<Expr, ScriptError> {
        let token = match self.toks.get(self.pos) {
            Some(spanned) => spanned.token.clone(),
            None => return Err(self.error("expression expected")),
        };
        self.pos += 1;

        match token {
            Token::Num(value) => Ok(Expr::Num(value)),
            Token::LParen => {
                let inner = self.expr()?;
                self.expect(&Token::RParen, ")")?;
                Ok(inner)
            }
            Token::Ident(name) => self.name_or_call(name),
            _ => Err(self.error("expression expected")),
        }
    }

    fn name_or_call(&mut self, name: String) -> Result<Expr, ScriptError> {
        if is_keyword(&name) {
            return Err(self.error(&format!("'{}' is a keyword", name)));
        }
        if self.peek() == Some(&Token::LParen) {
            self.pos += 1;
            return self.call_args(name);
        }
        let slot = self.intern(&name);
        Ok(Expr::Var(slot))
    }

    /// Parses the arguments of a built-in function call; the `(` is already consumed.
    fn call_args(&mut self, name: String) -> Result<Expr, ScriptError> {
        let (func, arity) = match name.as_str() {
            "rand" => (Func::Rand, 1),
            "abs" => (Func::Abs, 1),
            "min" => (Func::Min, 2),
            "max" => (Func::Max, 2),
            "key" => (Func::Key, 0),
            "getkey" => (Func::GetKey, 0),
            "ticks" => (Func::Ticks, 0),
            _ => return Err(self.error(&format!("unknown function '{}'", name))),
        };

        let mut args = Vec::new();
        if !self.eat(&Token::RParen) {
            loop {
                args.push(self.expr()?);
                if self.eat(&Token::Comma) {
                    continue;
                }
                self.expect(&Token::RParen, ")")?;
                break;
            }
        }
        if args.len() != arity {
            return Err(self.error(&format!("{} takes {} argument(s)", name, arity)));
        }
        Ok(Expr::Call(func, args))
    }

    /// Parses the comma separated things that `print` and `text` output.
    fn items(&mut self) -> Result<Vec<Item>, ScriptError> {
        let mut items = Vec::new();
        if self.pos >= self.toks.len() {
            return Ok(items);
        }
        loop {
            let next = self.peek().cloned();
            let item = match next {
                Some(Token::Str(text)) => {
                    self.pos += 1;
                    Item::Text(text)
                }
                _ => Item::Value(self.expr()?),
            };
            items.push(item);
            if !self.eat(&Token::Comma) {
                break;
            }
        }
        Ok(items)
    }

    // ---- statements ----

    /// Compiles `name = expr`; the name and the `=` are already consumed.
    fn assign(&mut self, name: String) -> Result<(), ScriptError> {
        if is_keyword(&name) {
            return Err(self.error(&format!("'{}' is a keyword", name)));
        }
        let slot = self.intern(&name);
        let value = self.expr()?;
        self.end_of_statement()?;
        self.emit(Stmt::Assign(slot, value));
        Ok(())
    }

    /// Compiles one statement (the tokens of a single line, without the `Eol`).
    fn statement(&mut self, toks: Vec<Spanned>) -> Result<(), ScriptError> {
        self.line = toks[0].line;
        self.toks = toks;
        self.pos = 1;

        let word = match self.toks[0].token.clone() {
            Token::Ident(word) => word,
            _ => return Err(self.error("a statement must start with a keyword or variable name")),
        };

        match word.as_str() {
            "let" => {
                let name = self.expect_ident("a variable name")?;
                self.expect(&Token::Eq, "=")?;
                self.assign(name)
            }
            "print" => {
                let items = self.items()?;
                self.end_of_statement()?;
                self.emit(Stmt::Print(items));
                Ok(())
            }
            _ => {
                if self.peek() == Some(&Token::Eq) {
                    self.pos += 1;
                    // `word` is still borrowed by the match above, so hand over a copy
                    self.assign(word.clone())
                } else {
                    Err(self.error(&format!("unknown statement '{}'", word)))
                }
            }
        }
    }

    fn finish(self) -> Result<Program, ScriptError> {
        Ok(Program {
            stmts: self.stmts,
            lines: self.lines,
            names: self.names,
            constants: CONSTANTS.iter().map(|constant| constant.1).collect(),
        })
    }
}

/// Compiles a whole script.
pub fn compile(source: &str) -> Result<Program, ScriptError> {
    let tokens = tokenize(source)?;
    let mut compiler = Compiler::new();
    let mut current: Vec<Spanned> = Vec::new();

    for spanned in tokens {
        if spanned.token == Token::Eol {
            if !current.is_empty() {
                compiler.statement(core::mem::take(&mut current))?;
            }
        } else {
            current.push(spanned);
        }
    }
    compiler.finish()
}
