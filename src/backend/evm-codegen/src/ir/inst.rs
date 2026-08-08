//! Operations, instructions, and terminators.
//!
//! The set is small on purpose. [`Op::Opcode`] carries any EVM opcode that
//! needs no special treatment — `ADDRESS`, `CALLER`, `KECCAK256`, `CALL`,
//! `LOG0`-`LOG4` — so the enum only names operations the compiler has an
//! opinion about: the ones needing width repair, the ones with more than one
//! result, and the ones whose EVM encoding disagrees with how they read.
//!
//! Terminators are not instructions. They live in a field on the block, which
//! turns "every block ends in exactly one terminator" into a check on one
//! `Option` and makes the stackifier's block lowering obviously total.

use evm_isa::OpSpec;

use super::effect::Effects;
use super::entity::{Block, Func, Value};
use super::types::{Repair, Type};
use crate::asm::Immediate;

/// An integer comparison.
///
/// Signedness comes from the *operand type*, not from the comparison — see the
/// representation invariant in [`types`](super::types). The EVM has `LT`, `GT`,
/// `SLT`, `SGT` and `EQ` and none of `Ne`, `Le`, `Ge`; those three are
/// synthesized with `ISZERO` by the stackifier (design doc §14).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Cmp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

/// What an instruction does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    // --- constants ---
    /// A literal. No arguments; the width comes from the instruction's type,
    /// and `verify` rejects a value too wide for it.
    Const(Immediate),

    // --- integer arithmetic, wrapping at the result width ---
    Add,
    Sub,
    Mul,
    /// EVM semantics: `x / 0 == 0`.
    ///
    /// Rust's div-by-zero panic is an ordinary `brif` to a panicking block that
    /// the frontend emits, *not* something implicit here. Making `Div` trap
    /// would turn arithmetic into control flow and put a second, invisible edge
    /// in every block containing one.
    Div,
    /// Remainder, with the same zero-divisor caveat as [`Div`](Op::Div).
    Rem,
    AddMod,
    MulMod,
    Exp,

    // --- checked arithmetic: two results, (value: ty, overflow: BOOL) ---
    /// `rustc_codegen_ssa`'s `checked_binop` (§14). Signedness from the type.
    AddOverflow,
    SubOverflow,
    MulOverflow,

    // --- bitwise ---
    And,
    Or,
    Xor,
    /// Bitwise complement.
    ///
    /// EVM `NOT` flips all 256 bits, so an unsigned result needs a mask (§14).
    /// A signed one does not: flipping a sign-extended word leaves a
    /// sign-extended word.
    Not,
    /// `args = [value, shift]` — **reading order**. The EVM's `SHL`/`SHR`/`SAR`
    /// take `(shift, value)`, and reconciling the two happens in exactly one
    /// place in the stackifier rather than at every construction site.
    Shl,
    Shr,
    Sar,
    /// EVM `BYTE`. `args = [index, value]`.
    ExtractByte,

    // --- casts ---
    /// Truncate, zero-extend, sign-extend, or reinterpret signedness.
    ///
    /// One operation for all four: the source type is on the operand, the
    /// destination on the result, and [`Repair::for_convert`] decides what that
    /// combination costs. There is no way to pick the wrong one.
    Convert,

    // --- comparison ---
    /// Result is always [`Type::BOOL`], which the EVM's comparisons already
    /// produce as 0 or 1.
    Icmp(Cmp),

    // --- selection ---
    /// `args = [cond, then, otherwise]`, lowered branch-free.
    ///
    /// The trick — `m = 0 - cond`, then `(t & m) | (e & !m)` — is only valid
    /// because [`Type::BOOL`]'s invariant guarantees `cond` is 0 or 1.
    Select,

    // --- memory ---
    /// Reserve `bytes` in the current frame; the result is a [`Type::Ptr`].
    ///
    /// May be handed to callees, whose frames sit above this one. Must not
    /// outlive the frame — which Rust's borrow checker guarantees upstream, and
    /// which is what makes reclaiming frames on return safe.
    Alloca {
        bytes: u32,
    },
    /// `args = [ptr]`.
    Load,
    /// `args = [ptr, value]`, no results.
    Store,
    /// `args = [dst, src, len]`. `MCOPY`, so Cancun and later only.
    MemCopy,

    // --- storage ---
    SLoad,
    SStore,
    TLoad,
    TStore,

    // --- the escape hatch ---
    /// A raw EVM opcode inlined here — the `#[evm_opcode]` path (§13).
    ///
    /// `args[i]` is the i-th value the opcode pops, so `args[0]` is the topmost
    /// operand and the order matches `evm-sys`'s bindings. Results follow
    /// `spec.stack_out`.
    ///
    /// The builder rejects three families: `PUSH`/`DUP`/`SWAP`/`POP` because
    /// the stackifier owns the operand stack, `JUMP`/`JUMPI`/`JUMPDEST`/`PC`
    /// because EIR owns control flow, and every terminating opcode because
    /// those are [`Terminator`]s.
    Opcode(&'static OpSpec),

    // --- calls ---
    /// An **internal** call: `JUMP`-based, per §5. Not a terminator — control
    /// returns to the next instruction.
    ///
    /// External calls are `Opcode(CALL)` and stay mentally separate, because
    /// they are ABI-bound, expensive, and can re-enter.
    Call {
        callee: Func,
    },
}

impl Op {
    /// The repair this operation's raw result needs to satisfy `ty`'s
    /// invariant.
    ///
    /// The general rule is that a narrow result needs bringing back into range
    /// — mask if unsigned, sign-extend if signed. The exceptions below are
    /// cases where the operands' own well-formedness already proves the result
    /// is in range, and each one is a proof rather than a guess: getting one
    /// wrong the other way is a silent miscompile.
    ///
    /// [`Convert`](Op::Convert) is absent because its repair depends on the
    /// *source* type, which an operation alone does not know; the builder calls
    /// [`Repair::for_convert`] for it.
    pub fn default_repair(self, ty: Type) -> Repair {
        let int = ty.as_int();

        match self {
            // A comparison yields 0 or 1, which is well-formed at any width.
            Op::Icmp(_) => Repair::None,

            // `verify` rejects a constant that does not fit its type, so by
            // the time anything looks at one it is in range.
            Op::Const(_) => Repair::None,

            // Both results come from elsewhere already well-formed: `Select`
            // returns one of its operands, `Call` returns what the callee's
            // own repairs produced, and an `Alloca` offset is a frame address.
            Op::Select | Op::Call { .. } | Op::Alloca { .. } => Repair::None,

            // Bitwise combination of two in-range operands stays in range, for
            // either signedness: above the width, unsigned operands are all
            // zero, and signed operands are all copies of their sign bits — so
            // the result's high bits agree with its own bit `w-1` either way.
            Op::And | Op::Or | Op::Xor => Repair::None,

            // `NOT` of a sign-extended word is sign-extended; of a zero-extended
            // word it is all ones above the width, so unsigned needs the mask.
            Op::Not if int.signed => Repair::None,

            // Unsigned division and remainder cannot leave the range their
            // operands were already in: `a / b <= a` and `a % b < b`.
            Op::Div | Op::Rem if !int.signed => Repair::None,

            // Logical right shift of an in-range unsigned value stays in range.
            Op::Shr if !int.signed => Repair::None,

            // Everything else can carry bits out of the type's range.
            _ if int.signed => Repair::sext(int.bits),
            _ => Repair::mask(int.bits),
        }
    }

    /// What this operation can observe or disturb. See [`Effects`].
    pub fn effects(self) -> Effects {
        match self {
            Op::Const(_)
            | Op::Add
            | Op::Sub
            | Op::Mul
            | Op::Div
            | Op::Rem
            | Op::AddMod
            | Op::MulMod
            | Op::Exp
            | Op::AddOverflow
            | Op::SubOverflow
            | Op::MulOverflow
            | Op::And
            | Op::Or
            | Op::Xor
            | Op::Not
            | Op::Shl
            | Op::Shr
            | Op::Sar
            | Op::ExtractByte
            | Op::Convert
            | Op::Icmp(_)
            | Op::Select => Effects::PURE,

            // The offset it returns is a function of the frame layout alone,
            // but two allocas must not collapse into one under CSE.
            Op::Alloca { .. } => Effects::WRITES_MEMORY,

            Op::Load => Effects::READS_MEMORY,
            Op::Store => Effects::WRITES_MEMORY,
            Op::MemCopy => Effects::READS_MEMORY.union(Effects::WRITES_MEMORY),

            Op::SLoad => Effects::READS_STORAGE,
            Op::SStore => Effects::WRITES_STORAGE,
            Op::TLoad => Effects::READS_TRANSIENT,
            Op::TStore => Effects::WRITES_TRANSIENT,

            Op::Opcode(spec) => Effects::of_opcode(spec),

            // Conservative until there is an interprocedural analysis to say
            // otherwise. A callee may store, log, or call out.
            Op::Call { .. } => Effects::ALL,
        }
    }

    /// The mnemonic used in a dump.
    pub fn name(self) -> &'static str {
        match self {
            Op::Const(_) => "const",
            Op::Add => "add",
            Op::Sub => "sub",
            Op::Mul => "mul",
            Op::Div => "div",
            Op::Rem => "rem",
            Op::AddMod => "addmod",
            Op::MulMod => "mulmod",
            Op::Exp => "exp",
            Op::AddOverflow => "add.overflow",
            Op::SubOverflow => "sub.overflow",
            Op::MulOverflow => "mul.overflow",
            Op::And => "and",
            Op::Or => "or",
            Op::Xor => "xor",
            Op::Not => "not",
            Op::Shl => "shl",
            Op::Shr => "shr",
            Op::Sar => "sar",
            Op::ExtractByte => "byte",
            Op::Convert => "convert",
            Op::Icmp(Cmp::Eq) => "eq",
            Op::Icmp(Cmp::Ne) => "ne",
            Op::Icmp(Cmp::Lt) => "lt",
            Op::Icmp(Cmp::Le) => "le",
            Op::Icmp(Cmp::Gt) => "gt",
            Op::Icmp(Cmp::Ge) => "ge",
            Op::Select => "select",
            Op::Alloca { .. } => "alloca",
            Op::Load => "load",
            Op::Store => "store",
            Op::MemCopy => "memcopy",
            Op::SLoad => "sload",
            Op::SStore => "sstore",
            Op::TLoad => "tload",
            Op::TStore => "tstore",
            Op::Opcode(spec) => spec.mnemonic,
            Op::Call { .. } => "call",
        }
    }

    /// How many results this produces. Two only for checked arithmetic.
    pub fn result_count(self) -> usize {
        match self {
            Op::AddOverflow | Op::SubOverflow | Op::MulOverflow => 2,
            Op::Store | Op::SStore | Op::TStore | Op::MemCopy => 0,
            Op::Opcode(spec) => spec.stack_out as usize,
            // `Call` depends on the callee's signature, which `Op` cannot see;
            // `verify` checks it against the module.
            Op::Call { .. } => usize::MAX,
            _ => 1,
        }
    }
}

/// One instruction: an operation, its type, its repair, and its wiring.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstData {
    pub op: Op,
    /// The type of the first result. Checked arithmetic's second result is
    /// always [`Type::BOOL`].
    pub ty: Type,
    pub repair: Repair,
    pub args: Vec<Value>,
    pub results: Vec<Value>,
    /// Free text a dump prints as a trailing comment.
    ///
    /// The rustc shim puts the MIR statement it lowered here, which is what
    /// lets an EIR dump be read beside a MIR dump line for line (§16).
    pub note: Option<Box<str>>,
}

/// A branch target and the values it passes to that block's parameters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockCall {
    pub block: Block,
    pub args: Vec<Value>,
}

impl BlockCall {
    pub fn new(block: Block, args: impl IntoIterator<Item = Value>) -> Self {
        BlockCall {
            block,
            args: args.into_iter().collect(),
        }
    }
}

/// How a block ends. Exactly one per block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Terminator {
    Jump(BlockCall),
    Brif {
        cond: Value,
        then: BlockCall,
        otherwise: BlockCall,
    },
    /// `BuilderMethods::switch` (§13). Lowered as a linear `EQ` chain.
    Switch {
        on: Value,
        cases: Vec<(Immediate, BlockCall)>,
        default: BlockCall,
    },

    /// Return from an internal call — jump back through the frame's saved
    /// return address. Not an EVM halt.
    Return(Vec<Value>),

    // --- EVM halts: these end the frame, not just the function ---
    Stop,
    ReturnData {
        ptr: Value,
        len: Value,
    },
    Revert {
        ptr: Value,
        len: Value,
    },
    SelfDestruct {
        beneficiary: Value,
    },
    /// The `INVALID` opcode: burn all remaining gas and abort. An observable
    /// halt that the program means to execute.
    Invalid,
    /// "Control never reaches here" — a claim, not an instruction.
    ///
    /// Lowers to `INVALID` too, but is kept distinct because an optimizer may
    /// reason from `Unreachable` and must not reason from
    /// [`Invalid`](Terminator::Invalid). Conflating the two is how a revert
    /// eventually gets deleted.
    Unreachable,
}

impl Terminator {
    /// The blocks this can transfer control to, with their edge arguments.
    pub fn block_calls(&self) -> Vec<&BlockCall> {
        match self {
            Terminator::Jump(target) => vec![target],
            Terminator::Brif { then, otherwise, .. } => vec![then, otherwise],
            Terminator::Switch { cases, default, .. } => {
                let mut calls: Vec<&BlockCall> = cases.iter().map(|(_, call)| call).collect();
                calls.push(default);
                calls
            }
            _ => Vec::new(),
        }
    }

    /// Values this reads. Edge arguments are included.
    pub fn args(&self) -> Vec<Value> {
        let mut args: Vec<Value> = match self {
            Terminator::Brif { cond, .. } => vec![*cond],
            Terminator::Switch { on, .. } => vec![*on],
            Terminator::Return(values) => values.clone(),
            Terminator::ReturnData { ptr, len } | Terminator::Revert { ptr, len } => vec![*ptr, *len],
            Terminator::SelfDestruct { beneficiary } => vec![*beneficiary],
            _ => Vec::new(),
        };
        for call in self.block_calls() {
            args.extend_from_slice(&call.args);
        }
        args
    }

    /// Whether this ends the whole EVM frame rather than returning to a caller.
    pub fn is_halt(&self) -> bool {
        matches!(
            self,
            Terminator::Stop
                | Terminator::ReturnData { .. }
                | Terminator::Revert { .. }
                | Terminator::SelfDestruct { .. }
                | Terminator::Invalid
                | Terminator::Unreachable
        )
    }
}

#[cfg(test)]
mod tests;
