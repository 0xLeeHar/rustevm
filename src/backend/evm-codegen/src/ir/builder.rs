//! Building a function by hand.
//!
//! This is the surface build-order step 2 exists for: EIR has to be writable
//! before rustc is involved, so that a MIR→bytecode bug can be bisected by
//! feeding hand-written EIR through the same pipeline (§16). It is held to the
//! same standard as the rest of the API rather than treated as a test fixture.
//!
//! Errors are **sticky**, exactly as in [`Asm`](crate::Asm): the first failure
//! is recorded, later calls become no-ops, and [`finish`](FunctionBuilder::finish)
//! reports it. That keeps construction free of `?` on every line, and nothing
//! downstream can act on a bad function because `finish` is the only way to get
//! one out.
//!
//! The consequence to know about: after an error, the [`Value`]s handed back
//! are placeholders. They are safe — `finish` will not return a `Function` —
//! but they are not meaningful, so do not reach into a builder's state to
//! inspect them mid-construction.

use evm_isa::{OpCategory, OpForm, by_mnemonic, op_form};

use super::IrError;
use super::entity::{Block, Func, Inst, Value};
use super::func::{BlockKind, Function};
use super::inst::{BlockCall, Cmp, InstData, Op, Terminator};
use super::types::{Repair, Signature, Type};
use crate::asm::Immediate;

/// Builds one [`Function`].
pub struct FunctionBuilder {
    func: Function,
    current: Option<Block>,
    last: Option<Inst>,
    error: Option<IrError>,
}

impl FunctionBuilder {
    /// Starts a function whose entry block already carries one parameter per
    /// signature parameter, and switches to it.
    pub fn new(name: &str, sig: Signature) -> Self {
        let func = Function::new(name, sig);
        let entry = func.entry();
        FunctionBuilder {
            func,
            current: Some(entry),
            last: None,
            error: None,
        }
    }

    pub fn entry(&self) -> Block {
        self.func.entry()
    }

    /// The `n`th parameter of the function.
    ///
    /// Panics if there is no such parameter — the signature is right there, so
    /// this is an indexing mistake rather than a malformed program, and the
    /// sticky-error channel has nothing meaningful to hand back.
    pub fn param(&self, n: usize) -> Value {
        self.block_param(self.entry(), n)
    }

    pub fn block_param(&self, block: Block, n: usize) -> Value {
        let params = self.func.block_params(block);
        match params.get(n) {
            Some(value) => *value,
            None => panic!("{block} has {} parameters, asked for #{n}", params.len()),
        }
    }

    pub fn create_block(&mut self) -> Block {
        self.func.create_block()
    }

    /// A block taking `params` — the merge point that replaces a phi node.
    pub fn create_block_with(&mut self, params: &[Type]) -> Block {
        let block = self.func.create_block();
        for ty in params {
            self.func.append_param(block, *ty);
        }
        block
    }

    pub fn set_block_kind(&mut self, block: Block, kind: BlockKind) -> &mut Self {
        self.func.set_block_kind(block, kind);
        self
    }

    /// Direct subsequent instructions into `block`.
    pub fn switch_to(&mut self, block: Block) -> &mut Self {
        self.current = Some(block);
        self.last = None;
        self
    }

    /// Attach a comment to the instruction just emitted.
    ///
    /// The shim puts the MIR statement it lowered here, which is what makes an
    /// EIR dump readable beside a MIR dump (§16).
    pub fn note(&mut self, text: &str) -> &mut Self {
        if let Some(inst) = self.last {
            self.func.set_note(inst, text);
        }
        self
    }

    /// Emit an instruction. One method per operation; see [`InsBuilder`].
    pub fn ins(&mut self) -> InsBuilder<'_> {
        InsBuilder { builder: self }
    }

    // --- terminators ---

    pub fn jump(&mut self, to: Block, args: &[Value]) {
        self.terminate(Terminator::Jump(BlockCall::new(to, args.iter().copied())));
    }

    pub fn brif(&mut self, cond: Value, then: Block, then_args: &[Value], otherwise: Block, else_args: &[Value]) {
        self.terminate(Terminator::Brif {
            cond,
            then: BlockCall::new(then, then_args.iter().copied()),
            otherwise: BlockCall::new(otherwise, else_args.iter().copied()),
        });
    }

    pub fn switch(&mut self, on: Value, cases: &[(Immediate, Block)], default: Block) {
        self.terminate(Terminator::Switch {
            on,
            cases: cases
                .iter()
                .map(|(value, block)| (*value, BlockCall::new(*block, [])))
                .collect(),
            default: BlockCall::new(default, []),
        });
    }

    pub fn ret(&mut self, values: &[Value]) {
        self.terminate(Terminator::Return(values.to_vec()));
    }

    pub fn stop(&mut self) {
        self.terminate(Terminator::Stop);
    }

    pub fn return_data(&mut self, ptr: Value, len: Value) {
        self.terminate(Terminator::ReturnData { ptr, len });
    }

    pub fn revert(&mut self, ptr: Value, len: Value) {
        self.terminate(Terminator::Revert { ptr, len });
    }

    pub fn selfdestruct(&mut self, beneficiary: Value) {
        self.terminate(Terminator::SelfDestruct { beneficiary });
    }

    pub fn invalid(&mut self) {
        self.terminate(Terminator::Invalid);
    }

    pub fn unreachable(&mut self) {
        self.terminate(Terminator::Unreachable);
    }

    /// The only way to obtain a [`Function`].
    pub fn finish(self) -> Result<Function, IrError> {
        match self.error {
            Some(error) => Err(error),
            None => Ok(self.func),
        }
    }

    // --- internals ---

    fn terminate(&mut self, term: Terminator) {
        if self.error.is_some() {
            return;
        }
        let Some(block) = self.current else {
            self.fail(IrError::NoCurrentBlock);
            return;
        };
        if self.func.terminator(block).is_some() {
            self.fail(IrError::AlreadyTerminated { block });
            return;
        }
        self.func.set_terminator(block, term);
        // A terminated block cannot take more instructions; requiring an
        // explicit `switch_to` next is what stops one silently landing after a
        // `ret` and being unreachable.
        self.current = None;
        self.last = None;
    }

    /// Append an instruction, or record the first reason it could not be.
    fn emit(&mut self, op: Op, ty: Type, repair: Repair, args: &[Value], result_types: &[Type]) -> Vec<Value> {
        if self.error.is_some() {
            return placeholder(result_types.len());
        }
        let Some(block) = self.current else {
            self.fail(IrError::NoCurrentBlock);
            return placeholder(result_types.len());
        };
        if self.func.terminator(block).is_some() {
            self.fail(IrError::AlreadyTerminated { block });
            return placeholder(result_types.len());
        }

        let data = InstData {
            op,
            ty,
            repair,
            args: args.to_vec(),
            results: Vec::new(),
            note: None,
        };
        let inst = self.func.append_inst(block, data, result_types);
        self.last = Some(inst);
        self.func.results(inst).to_vec()
    }

    fn fail(&mut self, error: IrError) {
        self.error.get_or_insert(error);
    }
}

/// Placeholder results for an instruction that was not emitted.
///
/// Meaningless, and safe only because the builder is already in its error
/// state, so `finish` cannot hand out a `Function` that contains them.
fn placeholder(count: usize) -> Vec<Value> {
    vec![Value::new(0); count]
}

/// One method per operation. Obtained from [`FunctionBuilder::ins`].
///
/// Every method fills in the [`Repair`] itself, from the operation and the
/// result type — a frontend never writes one, which is what makes §14's
/// "repair to Rust width" step unforgettable.
pub struct InsBuilder<'a> {
    builder: &'a mut FunctionBuilder,
}

impl InsBuilder<'_> {
    // --- constants ---

    pub fn iconst(self, ty: Type, value: impl Into<Immediate>) -> Value {
        self.unary_op(Op::Const(value.into()), ty, &[])
    }

    // --- arithmetic ---

    pub fn add(self, ty: Type, a: Value, b: Value) -> Value {
        self.unary_op(Op::Add, ty, &[a, b])
    }

    pub fn sub(self, ty: Type, a: Value, b: Value) -> Value {
        self.unary_op(Op::Sub, ty, &[a, b])
    }

    pub fn mul(self, ty: Type, a: Value, b: Value) -> Value {
        self.unary_op(Op::Mul, ty, &[a, b])
    }

    /// EVM semantics: dividing by zero yields zero. Rust's panic is a `brif`
    /// the caller emits.
    pub fn div(self, ty: Type, a: Value, b: Value) -> Value {
        self.unary_op(Op::Div, ty, &[a, b])
    }

    pub fn rem(self, ty: Type, a: Value, b: Value) -> Value {
        self.unary_op(Op::Rem, ty, &[a, b])
    }

    pub fn exp(self, ty: Type, base: Value, exponent: Value) -> Value {
        self.unary_op(Op::Exp, ty, &[base, exponent])
    }

    // --- checked arithmetic: (result, overflow) ---

    pub fn add_overflow(self, ty: Type, a: Value, b: Value) -> (Value, Value) {
        self.checked(Op::AddOverflow, ty, a, b)
    }

    pub fn sub_overflow(self, ty: Type, a: Value, b: Value) -> (Value, Value) {
        self.checked(Op::SubOverflow, ty, a, b)
    }

    pub fn mul_overflow(self, ty: Type, a: Value, b: Value) -> (Value, Value) {
        self.checked(Op::MulOverflow, ty, a, b)
    }

    // --- bitwise ---

    pub fn and(self, ty: Type, a: Value, b: Value) -> Value {
        self.unary_op(Op::And, ty, &[a, b])
    }

    pub fn or(self, ty: Type, a: Value, b: Value) -> Value {
        self.unary_op(Op::Or, ty, &[a, b])
    }

    pub fn xor(self, ty: Type, a: Value, b: Value) -> Value {
        self.unary_op(Op::Xor, ty, &[a, b])
    }

    pub fn not(self, ty: Type, value: Value) -> Value {
        self.unary_op(Op::Not, ty, &[value])
    }

    /// `value` then `shift` — reading order, not the EVM's.
    pub fn shl(self, ty: Type, value: Value, shift: Value) -> Value {
        self.unary_op(Op::Shl, ty, &[value, shift])
    }

    pub fn shr(self, ty: Type, value: Value, shift: Value) -> Value {
        self.unary_op(Op::Shr, ty, &[value, shift])
    }

    pub fn sar(self, ty: Type, value: Value, shift: Value) -> Value {
        self.unary_op(Op::Sar, ty, &[value, shift])
    }

    // --- casts and comparison ---

    /// Truncate, extend, or reinterpret. The repair comes from
    /// [`Repair::for_convert`], so the direction cannot be got wrong.
    pub fn convert(self, to: Type, value: Value) -> Value {
        let from = self.builder.func.value_type(value);
        let repair = Repair::for_convert(from, to);
        self.builder.emit(Op::Convert, to, repair, &[value], &[to])[0]
    }

    /// Signedness comes from the operands' type; the result is a `BOOL`.
    pub fn icmp(self, cmp: Cmp, a: Value, b: Value) -> Value {
        self.unary_op(Op::Icmp(cmp), Type::BOOL, &[a, b])
    }

    pub fn select(self, ty: Type, cond: Value, then: Value, otherwise: Value) -> Value {
        self.unary_op(Op::Select, ty, &[cond, then, otherwise])
    }

    // --- memory ---

    pub fn alloca(self, bytes: u32) -> Value {
        self.unary_op(Op::Alloca { bytes }, Type::PTR, &[])
    }

    pub fn load(self, ty: Type, ptr: Value) -> Value {
        self.unary_op(Op::Load, ty, &[ptr])
    }

    pub fn store(self, ptr: Value, value: Value) {
        let ty = self.builder.func.value_type(value);
        self.builder.emit(Op::Store, ty, Repair::None, &[ptr, value], &[]);
    }

    pub fn memcopy(self, dst: Value, src: Value, len: Value) {
        self.builder
            .emit(Op::MemCopy, Type::PTR, Repair::None, &[dst, src, len], &[]);
    }

    // --- storage ---

    pub fn sload(self, ty: Type, key: Value) -> Value {
        self.unary_op(Op::SLoad, ty, &[key])
    }

    pub fn sstore(self, key: Value, value: Value) {
        let ty = self.builder.func.value_type(value);
        self.builder.emit(Op::SStore, ty, Repair::None, &[key, value], &[]);
    }

    pub fn tload(self, ty: Type, key: Value) -> Value {
        self.unary_op(Op::TLoad, ty, &[key])
    }

    pub fn tstore(self, key: Value, value: Value) {
        let ty = self.builder.func.value_type(value);
        self.builder.emit(Op::TStore, ty, Repair::None, &[key, value], &[]);
    }

    // --- the escape hatch and calls ---

    /// Inline a raw EVM opcode, producing one result.
    ///
    /// Rejects the three families EIR reserves to itself: `PUSH`/`DUP`/`SWAP`/
    /// `POP` (the stackifier owns the operand stack), `JUMP`/`JUMPI`/
    /// `JUMPDEST`/`PC` (EIR owns control flow), and the terminating opcodes
    /// (those are [`Terminator`]s). Also rejects a mnemonic the table has never
    /// heard of, and one that postdates nothing here — fork gating is the
    /// assembler's job and happens at emission.
    pub fn opcode(self, mnemonic: &str, ty: Type, args: &[Value]) -> Value {
        match self.raw_opcode(mnemonic, ty, args, 1) {
            Some(results) => results[0],
            None => Value::new(0),
        }
    }

    /// A raw opcode with no result — `LOG1`, `MSTORE8`, and the like.
    pub fn opcode_void(self, mnemonic: &str, args: &[Value]) {
        self.raw_opcode(mnemonic, Type::U256, args, 0);
    }

    /// An internal call. `sig` is the callee's, so the builder does not need
    /// the module to know how many results to mint.
    pub fn call(self, callee: Func, sig: &Signature, args: &[Value]) -> Vec<Value> {
        let ty = sig.returns.first().copied().unwrap_or(Type::U256);
        self.builder
            .emit(Op::Call { callee }, ty, Repair::None, args, &sig.returns)
    }

    // --- internals ---

    /// Emit with the repair the operation implies for its result type.
    fn unary_op(self, op: Op, ty: Type, args: &[Value]) -> Value {
        let repair = op.default_repair(ty);
        let results = self.builder.emit(op, ty, repair, args, &[ty]);
        results[0]
    }

    fn checked(self, op: Op, ty: Type, a: Value, b: Value) -> (Value, Value) {
        let repair = op.default_repair(ty);
        let results = self.builder.emit(op, ty, repair, &[a, b], &[ty, Type::BOOL]);
        (results[0], results[1])
    }

    fn raw_opcode(self, mnemonic: &str, ty: Type, args: &[Value], results: usize) -> Option<Vec<Value>> {
        let Some(spec) = by_mnemonic(mnemonic) else {
            self.builder.fail(IrError::UnknownMnemonic(mnemonic.to_string()));
            return None;
        };

        let reserved = if op_form(spec) != OpForm::Simple || spec.category == OpCategory::StackManip {
            Some("the stackifier owns the operand stack")
        } else if spec.category == OpCategory::Control {
            Some("EIR owns control flow")
        } else if spec.terminating {
            Some("terminating opcodes are terminators")
        } else {
            None
        };
        if let Some(reason) = reserved {
            self.builder.fail(IrError::ReservedOpcode {
                mnemonic: spec.mnemonic,
                reason,
            });
            return None;
        }

        let types = vec![ty; results];
        Some(self.builder.emit(Op::Opcode(spec), ty, Repair::None, args, &types))
    }
}

#[cfg(test)]
mod tests;
