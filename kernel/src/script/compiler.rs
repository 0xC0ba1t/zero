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
            "if" => {
                let cond = self.expr()?;
                self.end_of_statement()?;
                let stmt = self.emit(Stmt::If { cond, otherwise: 0 });
                self.blocks.push(Block::If {
                    stmt,
                    else_stmt: None,
                });
                Ok(())
            }
            "else" => {
                self.end_of_statement()?;
                let (if_stmt, has_else) = match self.blocks.last() {
                    Some(Block::If { stmt, else_stmt }) => (*stmt, else_stmt.is_some()),
                    _ => return Err(self.error("else without a matching if")),
                };
                if has_else {
                    return Err(self.error("this if already has an else"));
                }
                let else_stmt = self.emit(Stmt::Else { end: 0 });
                // when the condition is false, skip to the statement after the `else`
                if let Stmt::If { otherwise, .. } = &mut self.stmts[if_stmt] {
                    *otherwise = else_stmt + 1;
                }
                if let Some(Block::If { else_stmt: slot, .. }) = self.blocks.last_mut() {
                    *slot = Some(else_stmt);
                }
                Ok(())
            }
            "while" => {
                let cond = self.expr()?;
                self.end_of_statement()?;
                let stmt = self.emit(Stmt::While { cond, end: 0 });
                self.blocks.push(Block::While { stmt });
                Ok(())
            }
            "end" => {
                self.end_of_statement()?;
                match self.blocks.pop() {
                    Some(Block::If { stmt, else_stmt }) => {
                        let end = self.emit(Stmt::EndIf);
                        match else_stmt {
                            Some(else_stmt) => {
                                if let Stmt::Else { end: target } = &mut self.stmts[else_stmt] {
                                    *target = end;
                                }
                            }
                            None => {
                                if let Stmt::If { otherwise, .. } = &mut self.stmts[stmt] {
                                    *otherwise = end;
                                }
                            }
                        }
                    }
                    Some(Block::While { stmt }) => {
                        self.emit(Stmt::EndWhile { start: stmt });
                        let after = self.stmts.len();
                        if let Stmt::While { end, .. } = &mut self.stmts[stmt] {
                            *end = after;
                        }
                    }
                    Some(Block::Sub { stmt }) => {
                        self.emit(Stmt::EndSub);
                        let after = self.stmts.len();
                        if let Stmt::Sub { end } = &mut self.stmts[stmt] {
                            *end = after;
                        }
                    }
                    None => {
                        return Err(self.error("end without a matching if, while or sub"));
                    }
                }
                Ok(())
            }
            "sub" => {
                let name = self.expect_ident("a sub name")?;
                self.end_of_statement()?;
                if !self.blocks.is_empty() {
                    return Err(self.error("subs can only be defined at the top level"));
                }
                if self.subs.iter().any(|(existing, _)| *existing == name) {
                    return Err(self.error(&format!("a sub named '{}' already exists", name)));
                }
                let stmt = self.emit(Stmt::Sub { end: 0 });
                self.subs.push((name, stmt + 1));
                self.blocks.push(Block::Sub { stmt });
                Ok(())
            }
            "call" => {
                let name = self.expect_ident("a sub name")?;
                self.end_of_statement()?;
                let stmt = self.emit(Stmt::Call(0));
                // the sub may be defined further down, so resolve it at the end
                self.calls.push((stmt, name, self.line));
                Ok(())
            }
            "return" => {
                self.end_of_statement()?;
                self.emit(Stmt::Return);
                Ok(())
            }
            "exit" => {
                self.end_of_statement()?;
                self.emit(Stmt::Exit);
                Ok(())
            }
            "cls" => {
                self.end_of_statement()?;
                self.emit(Stmt::Cls);
                Ok(())
            }
            "plot" => {
                let x = self.expr()?;
                self.expect(&Token::Comma, ",")?;
                let y = self.expr()?;
                self.expect(&Token::Comma, ",")?;
                let ch = self.expr()?;
                self.end_of_statement()?;
                self.emit(Stmt::Plot(x, y, ch));
                Ok(())
            }
            "text" => {
                let x = self.expr()?;
                self.expect(&Token::Comma, ",")?;
                let y = self.expr()?;
                self.expect(&Token::Comma, ",")?;
                let items = self.items()?;
                self.end_of_statement()?;
                self.emit(Stmt::Text(x, y, items));
                Ok(())
            }
            "color" => {
                let color = self.expr()?;
                self.end_of_statement()?;
                self.emit(Stmt::Color(color));
                Ok(())
            }
            "sleep" => {
                let ms = self.expr()?;
                self.end_of_statement()?;
                self.emit(Stmt::Sleep(ms));
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

    fn finish(mut self) -> Result<Program, ScriptError> {
        if let Some(block) = self.blocks.last() {
            let (stmt, what) = match block {
                Block::If { stmt, .. } => (*stmt, "if"),
                Block::While { stmt } => (*stmt, "while"),
                Block::Sub { stmt } => (*stmt, "sub"),
            };
            let line = self.lines[stmt];
            return Err(ScriptError::new(
                line,
                &format!("this {} is never closed with end", what),
            ));
        }

        for (stmt, name, line) in &self.calls {
            let target = match self.subs.iter().find(|(existing, _)| existing == name) {
                Some((_, start)) => *start,
                None => {
                    return Err(ScriptError::new(
                        *line,
                        &format!("no sub named '{}'", name),
                    ));
                }
            };
            self.stmts[*stmt] = Stmt::Call(target);
        }

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

#[cfg(test)]
use alloc::string::ToString;

#[cfg(test)]
fn compile_error(source: &str) -> ScriptError {
    compile(source).unwrap_err()
}

#[test_case]
fn test_compile_counts_statements() {
    let program = compile("let a = 1; print a; if a\nprint 2\nend").unwrap();
    // assign, print, if, print, endif
    assert_eq!(program.stmts.len(), 5);
    assert_eq!(program.lines.len(), 5);
}

#[test_case]
fn test_compile_gives_variables_slots_after_the_constants() {
    let program = compile("x = 1\ny = x").unwrap();
    assert_eq!(program.names.len(), CONSTANTS.len() + 2);
    assert_eq!(program.constants.len(), CONSTANTS.len());
}

#[test_case]
fn test_compile_resolves_forward_calls() {
    let program = compile("call later\nsub later\nprint 1\nend").unwrap();
    // the sub body starts right after the `sub` statement
    assert_eq!(program.stmts[0], Stmt::Call(2));
}

#[test_case]
fn test_compile_errors() {
    assert_eq!(compile_error("frobnicate").to_string(), "line 1: unknown statement 'frobnicate'");
    assert_eq!(compile_error("print (1 + 2").message, "expected )");
    assert_eq!(compile_error("print 1 +").message, "expression expected");
    assert_eq!(compile_error("print 1 2").message, "unexpected extra text");
    assert_eq!(compile_error("let if = 3").message, "'if' is a keyword");
    assert_eq!(compile_error("print foo(1)").message, "unknown function 'foo'");
    assert_eq!(compile_error("print rand()").message, "rand takes 1 argument(s)");
}

#[test_case]
fn test_compile_block_errors() {
    assert_eq!(compile_error("print 1\nwhile 1\nprint 1").line, 2);
    assert_eq!(compile_error("if 1\nprint 1").message, "this if is never closed with end");
    assert_eq!(compile_error("end").message, "end without a matching if, while or sub");
    assert_eq!(compile_error("else").message, "else without a matching if");
    assert_eq!(compile_error("if 1\nelse\nelse\nend").message, "this if already has an else");
    assert_eq!(compile_error("call nope").message, "no sub named 'nope'");
    assert_eq!(compile_error("sub a\nend\nsub a\nend").message, "a sub named 'a' already exists");
    assert_eq!(
        compile_error("if 1\nsub a\nend\nend").message,
        "subs can only be defined at the top level"
    );
}
