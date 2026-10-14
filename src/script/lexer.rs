//! Turns script source text into tokens.

use super::ScriptError;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

/// A single lexical element of a script.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    Num(i32),
    Str(String),
    Ident(String),
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    /// `=`
    Eq,
    /// `==`
    EqEq,
    /// `!=`
    NotEq,
    Lt,
    Le,
    Gt,
    Ge,
    AndAnd,
    OrOr,
    Bang,
    LParen,
    RParen,
    Comma,
    /// End of a statement: a newline or a `;`.
    Eol,
}

/// A token together with the line it was found on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spanned {
    pub token: Token,
    pub line: usize,
}

/// Splits `source` into tokens. The result always ends with an `Eol`.
pub fn tokenize(source: &str) -> Result<Vec<Spanned>, ScriptError> {
    let mut tokens = Vec::new();
    let mut chars = source.chars().peekable();
    let mut line = 1;

    while let Some(&c) = chars.peek() {
        match c {
            '\n' => {
                chars.next();
                tokens.push(Spanned {
                    token: Token::Eol,
                    line,
                });
                line += 1;
            }
            ';' => {
                chars.next();
                tokens.push(Spanned {
                    token: Token::Eol,
                    line,
                });
            }
            c if c.is_whitespace() => {
                chars.next();
            }
            '#' => {
                // a comment runs to the end of the line
                while let Some(&c) = chars.peek() {
                    if c == '\n' {
                        break;
                    }
                    chars.next();
                }
            }
            other => {
                return Err(ScriptError::new(
                    line,
                    &format!("unexpected character '{}'", other),
                ));
            }
        }
    }

    tokens.push(Spanned {
        token: Token::Eol,
        line,
    });
    Ok(tokens)
}
