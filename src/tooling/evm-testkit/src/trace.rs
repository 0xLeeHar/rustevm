//! Instruction-level execution traces.
//!
//! Design doc §16: with no LLVM in the pipeline there is no IR to dump, so
//! revm execution is the debugger. A trace is what you read when the bytes
//! assembled fine and still did the wrong thing.
//!
//! Mnemonics come from `evm-isa` — the same table the assembler emitted from,
//! so a trace and the code that produced it can never disagree about what a
//! byte means.

use std::collections::HashMap;
use std::fmt;

use evm_isa::{OpCategory, by_byte};
use revm::inspector::Inspector;
use revm::interpreter::interpreter_types::Jumps;
use revm::interpreter::{CallInputs, CallOutcome, CreateInputs, CreateOutcome, Interpreter};
use revm::primitives::U256;

/// How many stack items a step renders before eliding the rest. The top of
/// the stack is what an instruction acts on; the depths below it are noise
/// in all but the rarest bug.
const STACK_SHOWN: usize = 5;

/// One executed instruction.
#[derive(Debug, Clone)]
pub struct Step {
    /// Call depth, normalised so the transaction's own frame is 0.
    pub depth: usize,
    /// Which frame this ran in. Two sibling calls sit at the same `depth` but
    /// get different `frame`s, which is what makes "the next instruction in
    /// *this* frame" answerable.
    pub frame: usize,
    /// Offset of this instruction in the frame's code.
    pub pc: usize,
    pub opcode: u8,
    /// Mnemonic per `evm-isa`, or `INVALID` for a byte the table has no entry
    /// for — which is exactly what the EVM will do with it.
    pub mnemonic: &'static str,
    /// Gas left *before* the instruction ran.
    pub gas_remaining: u64,
    /// Gas the instruction charged.
    pub gas_cost: u64,
    /// Stack *before* the instruction ran, deepest item first.
    pub stack: Vec<U256>,
}

impl fmt::Display for Step {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let indent = "  ".repeat(self.depth);
        write!(
            f,
            "{indent}{:04x}  {:<12} {:>9} {:>5}  ",
            self.pc, self.mnemonic, self.gas_remaining, self.gas_cost
        )?;

        if self.stack.is_empty() {
            return write!(f, "-");
        }
        // Reversed: an instruction's first operand is the last stack item, and
        // reading it left-to-right is how every EVM reference writes it.
        for (i, word) in self.stack.iter().rev().take(STACK_SHOWN).enumerate() {
            if i > 0 {
                write!(f, " ")?;
            }
            write!(f, "{word:#x}")?;
        }
        if self.stack.len() > STACK_SHOWN {
            write!(f, " …+{}", self.stack.len() - STACK_SHOWN)?;
        }
        Ok(())
    }
}

/// Every instruction one transaction executed, in order.
#[derive(Debug, Clone, Default)]
pub struct Trace {
    pub steps: Vec<Step>,
}

impl fmt::Display for Trace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "  pc  op                 gas  cost  stack (top first)")?;
        for step in &self.steps {
            writeln!(f, "{step}")?;
        }
        Ok(())
    }
}

/// The [`Inspector`] that records a [`Trace`].
#[derive(Default)]
pub(crate) struct Tracer {
    steps: Vec<Step>,
    depth: usize,
    /// Ids of the frames currently open, outermost first.
    frames: Vec<usize>,
    frames_seen: usize,
}

impl Tracer {
    fn enter_frame(&mut self) {
        self.frames_seen += 1;
        self.frames.push(self.frames_seen);
        self.depth += 1;
    }

    fn leave_frame(&mut self) {
        self.frames.pop();
        self.depth = self.depth.saturating_sub(1);
    }

    pub(crate) fn finish(mut self) -> Trace {
        self.charge_frame_openers();

        // revm may or may not open a frame for the transaction's own call
        // depending on how it was entered, so recorded depths are only
        // meaningful relative to each other. Rebase on the shallowest.
        let base = self.steps.iter().map(|s| s.depth).min().unwrap_or(0);
        Trace {
            steps: self
                .steps
                .into_iter()
                .map(|mut s| {
                    s.depth -= base;
                    s
                })
                .collect(),
        }
    }

    /// Re-derive the cost of every instruction that opens a child frame.
    ///
    /// `step_end` runs while the child still holds the gas its parent handed
    /// it, so a `CALL` measured that way looks like it cost the whole 63/64ths
    /// it forwarded. What it actually cost the caller is the gap to the next
    /// instruction *in the caller's frame*, once the child's leftovers have
    /// come back.
    ///
    /// Which opcodes those are is `evm-isa`'s [`OpCategory::Call`] — asking
    /// the table beats inferring it from whether a child frame happened to
    /// record any steps, because calling a codeless account records none and
    /// is charged all the same.
    fn charge_frame_openers(&mut self) {
        let mut next_in_frame: HashMap<usize, u64> = HashMap::new();
        for i in (0..self.steps.len()).rev() {
            let (opcode, frame, remaining) = (self.steps[i].opcode, self.steps[i].frame, self.steps[i].gas_remaining);
            let opens_frame = by_byte(opcode).is_some_and(|spec| spec.category == OpCategory::Call);

            if opens_frame && let Some(&resumed_at) = next_in_frame.get(&frame) {
                self.steps[i].gas_cost = remaining.saturating_sub(resumed_at);
            }
            next_in_frame.insert(frame, remaining);
        }
    }
}

impl<CTX> Inspector<CTX> for Tracer {
    fn step(&mut self, interp: &mut Interpreter, _ctx: &mut CTX) {
        let opcode = interp.bytecode.opcode();
        self.steps.push(Step {
            depth: self.depth,
            frame: self.frames.last().copied().unwrap_or(0),
            pc: interp.bytecode.pc(),
            opcode,
            mnemonic: by_byte(opcode).map_or("INVALID", |spec| spec.mnemonic),
            gas_remaining: interp.gas.remaining(),
            // Not known until the instruction has run; filled in by `step_end`.
            gas_cost: 0,
            stack: interp.stack.data().to_vec(),
        });
    }

    fn step_end(&mut self, interp: &mut Interpreter, _ctx: &mut CTX) {
        if let Some(step) = self.steps.last_mut() {
            step.gas_cost = step.gas_remaining.saturating_sub(interp.gas.remaining());
        }
    }

    fn call(&mut self, _ctx: &mut CTX, _inputs: &mut CallInputs) -> Option<CallOutcome> {
        self.enter_frame();
        None
    }

    fn call_end(&mut self, _ctx: &mut CTX, _inputs: &CallInputs, _outcome: &mut CallOutcome) {
        self.leave_frame();
    }

    fn create(&mut self, _ctx: &mut CTX, _inputs: &mut CreateInputs) -> Option<CreateOutcome> {
        self.enter_frame();
        None
    }

    fn create_end(&mut self, _ctx: &mut CTX, _inputs: &CreateInputs, _outcome: &mut CreateOutcome) {
        self.leave_frame();
    }
}
