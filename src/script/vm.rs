//! Runs compiled programs.

use super::ast::*;
use super::host::Host;
use super::ScriptError;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::fmt::Write;

/// How deeply subs may call each other.
const MAX_CALL_DEPTH: usize = 64;

struct Vm<'a> {
    program: &'a Program,
    vars: Vec<i32>,
    /// return addresses of the subs that are running
    calls: Vec<usize>,
    /// index of the statement to run next
    pc: usize,
    /// state of the random number generator
    rng: u32,
}

/// Runs `program` until it finishes, fails or is interrupted.
pub fn run(program: &Program, host: &mut dyn Host) -> Result<(), ScriptError> {
    let mut vm = Vm::new(program, host.ticks());
    loop {
        match vm.step(host) {
            Ok(true) => {}
            Ok(false) => return Ok(()),
            Err(message) => {
                let line = program.lines.get(vm.pc).copied().unwrap_or(0);
                return Err(ScriptError::new(line, message));
            }
        }
    }
}

impl<'a> Vm<'a> {
    fn new(program: &'a Program, seed: u64) -> Vm<'a> {
        let mut vars = vec![0; program.names.len()];
        for (slot, value) in program.constants.iter().enumerate() {
            vars[slot] = *value;
        }
        Vm {
            program,
            vars,
            calls: Vec::new(),
            pc: 0,
            // xorshift must not start at zero
            rng: (seed as u32).wrapping_mul(2_654_435_761) | 1,
        }
    }

    /// Returns the next pseudo random number (xorshift32).
    fn next_random(&mut self) -> u32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        x
    }

    /// Calls a built-in function. The compiler already checked the argument count.
    fn call(&mut self, func: Func, args: &[Expr], host: &mut dyn Host) -> Result<i32, &'static str> {
        match func {
            Func::Rand => {
                let limit = self.eval(&args[0], host)?;
                if limit <= 0 {
                    return Err("rand needs a positive limit");
                }
                Ok((self.next_random() % limit as u32) as i32)
            }
            Func::Abs => Ok(self.eval(&args[0], host)?.wrapping_abs()),
            Func::Min => {
                let a = self.eval(&args[0], host)?;
                let b = self.eval(&args[1], host)?;
                Ok(a.min(b))
            }
            Func::Max => {
                let a = self.eval(&args[0], host)?;
                let b = self.eval(&args[1], host)?;
                Ok(a.max(b))
            }
            Func::Key => Ok(host.key()),
            Func::GetKey => Ok(host.wait_key()),
            Func::Ticks => Ok(host.ticks() as i32),
        }
    }

    /// Evaluates an expression.
    fn eval(&mut self, expr: &Expr, host: &mut dyn Host) -> Result<i32, &'static str> {
        Ok(match expr {
            Expr::Num(value) => *value,
            Expr::Var(slot) => self.vars[*slot],
            Expr::Neg(inner) => self.eval(inner, host)?.wrapping_neg(),
            Expr::Not(inner) => (self.eval(inner, host)? == 0) as i32,
            Expr::Bin(op, left, right) => {
                let l = self.eval(left, host)?;
                match op {
                    // `&&` and `||` only evaluate the right side when needed
                    BinOp::And => {
                        if l == 0 {
                            0
                        } else {
                            (self.eval(right, host)? != 0) as i32
                        }
                    }
                    BinOp::Or => {
                        if l != 0 {
                            1
                        } else {
                            (self.eval(right, host)? != 0) as i32
                        }
                    }
                    _ => {
                        let r = self.eval(right, host)?;
                        match op {
                            BinOp::Add => l.wrapping_add(r),
                            BinOp::Sub => l.wrapping_sub(r),
                            BinOp::Mul => l.wrapping_mul(r),
                            BinOp::Div => {
                                if r == 0 {
                                    return Err("division by zero");
                                }
                                l.wrapping_div(r)
                            }
                            BinOp::Rem => {
                                if r == 0 {
                                    return Err("division by zero");
                                }
                                l.wrapping_rem(r)
                            }
                            BinOp::Eq => (l == r) as i32,
                            BinOp::Ne => (l != r) as i32,
                            BinOp::Lt => (l < r) as i32,
                            BinOp::Le => (l <= r) as i32,
                            BinOp::Gt => (l > r) as i32,
                            BinOp::Ge => (l >= r) as i32,
                            BinOp::And | BinOp::Or => 0,
                        }
                    }
                }
            }
            Expr::Call(func, args) => self.call(*func, args, host)?,
        })
    }

    /// Renders the items of a `print` or `text` statement into one string.
    fn render(&mut self, items: &[Item], host: &mut dyn Host) -> Result<String, &'static str> {
        let mut text = String::new();
        for item in items {
            match item {
                Item::Text(literal) => text.push_str(literal),
                Item::Value(expr) => {
                    let value = self.eval(expr, host)?;
                    let _ = write!(text, "{}", value);
                }
            }
        }
        Ok(text)
    }

    /// Executes one statement. Returns `Ok(false)` once the program is done.
    fn step(&mut self, host: &mut dyn Host) -> Result<bool, &'static str> {
        if host.interrupted() {
            return Err("interrupted");
        }
        let program = self.program;
        let stmt = match program.stmts.get(self.pc) {
            Some(stmt) => stmt,
            None => return Ok(false),
        };
        let mut next = self.pc + 1;

        match stmt {
            Stmt::Exit => return Ok(false),
            Stmt::Assign(slot, expr) => {
                let value = self.eval(expr, host)?;
                self.vars[*slot] = value;
            }
            Stmt::Print(items) => {
                let text = self.render(items, host)?;
                host.print(&text);
            }
            Stmt::If { cond, otherwise } => {
                if self.eval(cond, host)? == 0 {
                    next = *otherwise;
                }
            }
            Stmt::Else { end } => next = *end,
            Stmt::EndIf => {}
            Stmt::While { cond, end } => {
                if self.eval(cond, host)? == 0 {
                    next = *end;
                }
            }
            Stmt::EndWhile { start } => next = *start,
            Stmt::Sub { end } => next = *end,
            Stmt::Call(target) => {
                if self.calls.len() >= MAX_CALL_DEPTH {
                    return Err("call stack overflow");
                }
                self.calls.push(self.pc + 1);
                next = *target;
            }
            Stmt::EndSub | Stmt::Return => match self.calls.pop() {
                Some(return_to) => next = return_to,
                // `return` outside of a sub ends the program
                None => return Ok(false),
            },
            Stmt::Cls => host.clear(),
            Stmt::Plot(x, y, ch) => {
                let x = self.eval(x, host)?;
                let y = self.eval(y, host)?;
                let ch = self.eval(ch, host)?;
                host.plot(x, y, ch as u8);
            }
            Stmt::Text(x, y, items) => {
                let x = self.eval(x, host)?;
                let y = self.eval(y, host)?;
                let text = self.render(items, host)?;
                host.text(x, y, &text);
            }
            Stmt::Color(expr) => {
                let color = self.eval(expr, host)?;
                host.set_color(color as u8);
            }
            Stmt::Sleep(expr) => {
                let ms = self.eval(expr, host)?;
                if ms > 0 {
                    host.sleep(ms as u32);
                }
            }
        }

        self.pc = next;
        Ok(true)
    }
}
