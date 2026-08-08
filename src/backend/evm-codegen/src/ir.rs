//! EIR — the machine-independent IR the backend pipeline is built around.
//!
//! Typed, SSA, block-structured. It sits between the rustc shim above and
//! `stackify` below (design doc §13), and it is deliberately a *pure data
//! structure*: it has no notion of where a value lives at runtime. That is
//! `stackify`'s business, and keeping the two apart is what lets a smarter
//! stack allocator drop in later without touching anything here.
//!
//! Three properties are worth stating up front, because the rest of the module
//! only makes sense in their light.
//!
//! **Values carry a Rust type, signedness included, and every instruction
//! carries the [`Repair`] that restores it.** §14 describes lowering as three
//! steps and warns that the second — "repair to Rust width" — is "the step
//! you'll forget and it silently miscompiles". Here it is a field the builder
//! fills in, so forgetting it is unspellable. See [`types`] for the
//! representation invariant this buys and what it pays for.
//!
//! **SSA with block parameters, not phi nodes.** `rustc_codegen_ssa` runs its
//! own `non_ssa_locals` analysis and hands a backend a mix: locals with one
//! dominating definition arrive as values, everything else as an `alloca` with
//! loads and stores. Both forms are needed regardless, and the value half is
//! already SSA — anything else means converting away from what rustc gives and
//! back again for §15's passes. Parameters rather than phis because
//! predecessor-order and phi-operand-order drifting apart is a classic silent
//! miscompile, and because promoting an `alloca` out of the frame later needs
//! somewhere to put the merge.
//!
//! **It is meant to be written by hand.** Per the build order, EIR exists and
//! compiles before rustc is involved, so that when MIR→bytecode goes wrong you
//! can feed hand-written EIR through the same pipeline and find out whether the
//! bug is above or below this layer (§16). [`builder`] is that surface, and it
//! is held to the same standard as the rest of the API rather than treated as a
//! test fixture.
//!
//! All opcode data still comes from `evm-isa`; nothing here restates a byte
//! value, an arity, or a fork gate.

pub mod builder;
pub mod effect;
pub mod entity;
pub mod func;
pub mod inst;
pub mod module;
pub mod types;

pub use builder::{FunctionBuilder, InsBuilder};
pub use effect::Effects;
pub use entity::{Block, Func, Inst, Value};
pub use func::{BlockData, BlockKind, Function, ValueData, ValueDef};
pub use inst::{BlockCall, Cmp, InstData, Op, Terminator};
pub use module::Module;
pub use types::{IntTy, Repair, Signature, Type};

use std::fmt;

/// What can go wrong building or checking EIR.
///
/// Reported by [`FunctionBuilder::finish`] and [`Module::verify`]. More
/// variants arrive with the per-function verifier; the ones here are what
/// construction alone can detect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IrError {
    /// An instruction was emitted with no block to put it in — after a
    /// terminator, and before the next `switch_to`.
    NoCurrentBlock,
    /// A second terminator, or an instruction after one.
    AlreadyTerminated { block: Block },
    /// No such mnemonic in `evm-isa`'s table.
    UnknownMnemonic(String),
    /// An opcode EIR reserves to itself rather than exposing raw.
    ReservedOpcode {
        mnemonic: &'static str,
        reason: &'static str,
    },
    /// A body whose signature disagrees with its declaration.
    SignatureMismatch { func: Func, name: Box<str> },
    /// A second body for one declaration.
    AlreadyDefined { func: Func, name: Box<str> },
    /// A function declared but never defined.
    Undefined { func: Func, name: Box<str> },
    /// A module with no entry point.
    NoEntry,
}

impl fmt::Display for IrError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IrError::NoCurrentBlock => {
                write!(
                    f,
                    "no block is being built — the last one was terminated; `switch_to` another"
                )
            }
            IrError::AlreadyTerminated { block } => write!(f, "{block} already ends in a terminator"),
            IrError::UnknownMnemonic(mnemonic) => write!(f, "unknown opcode `{mnemonic}`"),
            IrError::ReservedOpcode { mnemonic, reason } => {
                write!(f, "`{mnemonic}` cannot be emitted as a raw opcode: {reason}")
            }
            IrError::SignatureMismatch { func, name } => {
                write!(
                    f,
                    "the body given for `{name}` ({func}) does not match its declared signature"
                )
            }
            IrError::AlreadyDefined { func, name } => write!(f, "`{name}` ({func}) is already defined"),
            IrError::Undefined { func, name } => write!(f, "`{name}` ({func}) was declared but never defined"),
            IrError::NoEntry => write!(f, "the module has no entry function"),
        }
    }
}

impl std::error::Error for IrError {}
