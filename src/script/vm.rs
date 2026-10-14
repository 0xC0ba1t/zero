//! Runs compiled programs.

use super::ast::*;
use super::host::Host;
use super::ScriptError;
use alloc::vec;
use alloc::vec::Vec;

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
            _ => return Err("not implemented yet"),
        }

        self.pc = next;
        Ok(true)
    }
}
