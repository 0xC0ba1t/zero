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

#[cfg(test)]
use super::mock::MockHost;
#[cfg(test)]
use alloc::string::ToString;

#[cfg(test)]
fn output_of(source: &str) -> String {
    let mut host = MockHost::default();
    super::run(source, &mut host).unwrap();
    host.output
}

#[cfg(test)]
fn error_of(source: &str) -> String {
    let mut host = MockHost::default();
    super::run(source, &mut host).unwrap_err().to_string()
}

#[test_case]
fn test_arithmetic_and_precedence() {
    assert_eq!(output_of("print 1 + 2 * 3"), "7\n");
    assert_eq!(output_of("print (1 + 2) * 3"), "9\n");
    assert_eq!(output_of("print 7 / 2"), "3\n");
    assert_eq!(output_of("print 7 % 4"), "3\n");
    assert_eq!(output_of("print -5 + 2"), "-3\n");
    assert_eq!(output_of("print - 2 * 3"), "-6\n");
}

#[test_case]
fn test_division_truncates_towards_zero() {
    assert_eq!(output_of("print -7 / 2"), "-3\n");
    assert_eq!(output_of("print -7 % 4"), "-3\n");
}

#[test_case]
fn test_arithmetic_wraps_instead_of_panicking() {
    assert_eq!(output_of("print 2147483647 + 1"), "-2147483648\n");
}

#[test_case]
fn test_comparisons_and_logic() {
    assert_eq!(output_of("print 1 < 2, \" \", 2 < 1"), "1 0\n");
    assert_eq!(output_of("print 3 == 3, 3 != 3, 3 <= 3, 3 >= 4"), "1010\n");
    assert_eq!(output_of("print 1 && 0 || 1"), "1\n");
    assert_eq!(output_of("print !0, !5"), "10\n");
}

#[test_case]
fn test_logic_short_circuits() {
    // the right side would divide by zero if it were evaluated
    assert_eq!(output_of("print 0 && 1 / 0"), "0\n");
    assert_eq!(output_of("print 1 || 1 / 0"), "1\n");
}

#[test_case]
fn test_literals_and_constants() {
    assert_eq!(output_of("print 'A'"), "65\n");
    assert_eq!(output_of("print WIDTH, \" \", HEIGHT, \" \", UP, \" \", DOWN"), "80 25 256 257\n");
    assert_eq!(output_of("print ENTER, \" \", ESC"), "10 27\n");
    assert_eq!(output_of("print"), "\n");
}

#[test_case]
fn test_builtin_functions() {
    assert_eq!(output_of("print min(3, 9), \" \", max(3, 9), \" \", abs(-4)"), "3 9 4\n");
    let source = "let i = 0\nlet ok = 1\nwhile i < 200\n let r = rand(10)\n if r < 0 || r >= 10\n ok = 0\n end\n i = i + 1\nend\nprint ok";
    assert_eq!(output_of(source), "1\n");
}

#[test_case]
fn test_variables_and_loops() {
    assert_eq!(output_of("let a = 5; a = a * 2; print a"), "10\n");
    assert_eq!(output_of("print never_set"), "0\n");
    let source = "let s = 0\nlet i = 1\nwhile i <= 10\n s = s + i\n i = i + 1\nend\nprint s";
    assert_eq!(output_of(source), "55\n");
    assert_eq!(output_of("let i = 0\nwhile i < 3\n i = i + 1\n print i\nend"), "1\n2\n3\n");
}

#[test_case]
fn test_if_and_else() {
    assert_eq!(output_of("if 1\n print \"yes\"\nelse\n print \"no\"\nend"), "yes\n");
    assert_eq!(output_of("if 0\n print \"yes\"\nelse\n print \"no\"\nend"), "no\n");
    assert_eq!(output_of("if 0\n print \"yes\"\nend\nprint \"done\""), "done\n");
}

#[test_case]
fn test_subs() {
    assert_eq!(output_of("sub hi\n print \"hi\"\nend\ncall hi\ncall hi"), "hi\nhi\n");
    assert_eq!(output_of("call later\nsub later\n print \"x\"\nend"), "x\n");
    assert_eq!(output_of("sub a\n print 1\n return\n print 2\nend\ncall a\nprint 3"), "1\n3\n");
    assert_eq!(
        output_of("sub a\n call b\n print \"a\"\nend\nsub b\n print \"b\"\nend\ncall a"),
        "b\na\n"
    );
}

#[test_case]
fn test_exit_and_comments() {
    assert_eq!(output_of("print 1\nexit\nprint 2"), "1\n");
    assert_eq!(output_of("# nothing but a comment\nprint 1 # and a trailing one\n"), "1\n");
}

#[test_case]
fn test_strings_and_escapes() {
    assert_eq!(output_of("print \"a\\nb\", \"\\\"q\\\"\""), "a\nb\"q\"\n");
}

#[test_case]
fn test_keys_and_drawing() {
    let mut host = MockHost::default();
    host.keys.push(119);
    super::run("print key()\nprint key()", &mut host).unwrap();
    assert_eq!(host.output, "119\n0\n");

    let mut host = MockHost::default();
    let source = "plot 1, 2, 'x'\ntext 3, 4, \"n=\", 5\ncolor 14\ncls\nsleep 100\nsleep 0";
    super::run(source, &mut host).unwrap();
    assert_eq!(host.plots, [(1, 2, 120)]);
    assert_eq!(host.texts, [(3, 4, String::from("n=5"))]);
    assert_eq!(host.colors, [14]);
    assert_eq!(host.clears, 1);
    assert_eq!(host.slept, 100);
}

#[test_case]
fn test_runtime_errors_report_the_line() {
    assert_eq!(error_of("print 1 / 0"), "line 1: division by zero");
    assert_eq!(error_of("print 1\nlet x = 4 % 0"), "line 2: division by zero");
    assert_eq!(error_of("print rand(0)"), "line 1: rand needs a positive limit");
}

#[test_case]
fn test_call_depth_is_limited() {
    assert_eq!(error_of("sub r\n call r\nend\ncall r"), "line 2: call stack overflow");
}

#[test_case]
fn test_scripts_can_be_interrupted() {
    let mut host = MockHost::default();
    host.interrupt_after = Some(50);
    let error = super::run("while 1\nend", &mut host).unwrap_err();
    assert_eq!(error.message, "interrupted");
}

#[test_case]
fn test_compile_errors_come_out_of_run() {
    assert_eq!(error_of("print 1 +"), "line 1: expression expected");
    assert_eq!(error_of("print \"abc"), "line 1: unterminated string");
}
