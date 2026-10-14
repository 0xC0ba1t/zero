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
            '0'..='9' => {
                let mut value: i32 = 0;
                while let Some(&digit) = chars.peek() {
                    match digit.to_digit(10) {
                        Some(digit) => {
                            let next = value
                                .checked_mul(10)
                                .and_then(|value| value.checked_add(digit as i32));
                            value = match next {
                                Some(next) => next,
                                None => return Err(ScriptError::new(line, "number too large")),
                            };
                            chars.next();
                        }
                        None => break,
                    }
                }
                tokens.push(Spanned {
                    token: Token::Num(value),
                    line,
                });
            }
            c if c.is_ascii_alphabetic() || c == '_' => {
                let mut name = String::new();
                while let Some(&c) = chars.peek() {
                    if c.is_ascii_alphanumeric() || c == '_' {
                        name.push(c);
                        chars.next();
                    } else {
                        break;
                    }
                }
                tokens.push(Spanned {
                    token: Token::Ident(name),
                    line,
                });
            }
            '+' | '-' | '*' | '/' | '%' | '(' | ')' | ',' => {
                chars.next();
                let token = match c {
                    '+' => Token::Plus,
                    '-' => Token::Minus,
                    '*' => Token::Star,
                    '/' => Token::Slash,
                    '%' => Token::Percent,
                    '(' => Token::LParen,
                    ')' => Token::RParen,
                    _ => Token::Comma,
                };
                tokens.push(Spanned { token, line });
            }
            '=' | '!' | '<' | '>' => {
                chars.next();
                // `==`, `!=`, `<=` and `>=` have a second character
                let with_eq = chars.peek() == Some(&'=');
                if with_eq {
                    chars.next();
                }
                let token = match (c, with_eq) {
                    ('=', false) => Token::Eq,
                    ('=', true) => Token::EqEq,
                    ('!', false) => Token::Bang,
                    ('!', true) => Token::NotEq,
                    ('<', false) => Token::Lt,
                    ('<', true) => Token::Le,
                    ('>', false) => Token::Gt,
                    _ => Token::Ge,
                };
                tokens.push(Spanned { token, line });
            }
            '&' | '|' => {
                chars.next();
                if chars.next() != Some(c) {
                    return Err(ScriptError::new(
                        line,
                        &format!("expected {0}{0} (a single {0} is not an operator)", c),
                    ));
                }
                let token = if c == '&' { Token::AndAnd } else { Token::OrOr };
                tokens.push(Spanned { token, line });
            }
            '"' => {
                chars.next();
                let mut text = String::new();
                loop {
                    match chars.next() {
                        Some('"') => break,
                        Some('\n') | None => {
                            return Err(ScriptError::new(line, "unterminated string"));
                        }
                        Some('\\') => match chars.next() {
                            Some('n') => text.push('\n'),
                            Some('"') => text.push('"'),
                            Some('\\') => text.push('\\'),
                            _ => return Err(ScriptError::new(line, "unknown escape in string")),
                        },
                        Some(c) => text.push(c),
                    }
                }
                tokens.push(Spanned {
                    token: Token::Str(text),
                    line,
                });
            }
            '\'' => {
                // a character literal such as 'q' is just a number
                chars.next();
                let ch = match chars.next() {
                    Some(ch) if ch != '\'' && ch != '\n' => ch,
                    _ => return Err(ScriptError::new(line, "bad character literal")),
                };
                if chars.next() != Some('\'') {
                    return Err(ScriptError::new(line, "unterminated character literal"));
                }
                tokens.push(Spanned {
                    token: Token::Num(ch as i32),
                    line,
                });
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
