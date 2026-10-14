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

    fn statement(&mut self, toks: Vec<Spanned>) -> Result<(), ScriptError> {
        self.line = toks[0].line;
        self.toks = toks;
        self.pos = 0;
        Err(self.error("statements are not supported yet"))
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
