//! What an operation can observe or disturb.
//!
//! Nothing reads this yet — `evm-opt` is design-doc step 6. It exists now
//! because §15's passes (redundant storage elimination, `MLOAD`-after-`MSTORE`
//! forwarding, CSE, DCE) all need to know which operations may be reordered
//! past which, and retrofitting that answer onto an instruction set later means
//! auditing every op under time pressure. Classifying each one as it is added
//! is nearly free.
//!
//! It lives here rather than in `evm-isa` for two reasons. `evm-isa`'s own spec
//! says it is pure data with no behaviour, and it is read by a stable-only
//! proc-macro crate that has no use for an optimizer's lattice. More to the
//! point, the unit an optimizer reasons about is an *EIR instruction*, not an
//! EVM opcode — `Add` with a `Mask` repair is two opcodes, and a `Call` is a
//! whole jump sequence. Nothing is restated, though: [`Effects::of_opcode`]
//! derives its answer from the table.
//!
//! The rule this is all for, written down now so §15 cannot get it wrong later:
//!
//! > An instruction may be removed only if its results are unused **and** its
//! > effects are pure. Terminators are never removed. A branch to a panicking
//! > block disappears only when its condition folds to a constant — a proof,
//! > not a heuristic.
//!
//! §15 puts it more bluntly: on-chain, silently not reverting is lost funds.

use std::fmt;

use evm_isa::{OpCategory, OpSpec};

/// A set of effects.
///
/// A hand-rolled bitset over a `u16` rather than a `bitflags` dependency —
/// `evm-codegen` depends only on `evm-isa` (§18) and nine flags do not justify
/// changing that.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct Effects(u16);

impl Effects {
    /// Depends on nothing but its operands, and disturbs nothing. Freely
    /// reorderable, foldable, and CSE-able.
    pub const PURE: Effects = Effects(0);

    pub const READS_MEMORY: Effects = Effects(1 << 0);
    pub const WRITES_MEMORY: Effects = Effects(1 << 1);
    pub const READS_STORAGE: Effects = Effects(1 << 2);
    pub const WRITES_STORAGE: Effects = Effects(1 << 3);
    pub const READS_TRANSIENT: Effects = Effects(1 << 4);
    pub const WRITES_TRANSIENT: Effects = Effects(1 << 5);

    /// Reads the execution environment — `TIMESTAMP`, `CALLER`, `GAS`,
    /// `BALANCE`. No side effect, but not constant-foldable, and not CSE-able
    /// across anything [`EXTERNAL`](Effects::EXTERNAL): `GAS` differs every
    /// time, and a balance can change under a re-entrant call.
    pub const READS_CONTEXT: Effects = Effects(1 << 6);

    /// Emits a log. Observable off-chain, so never removable, but it cannot be
    /// observed by anything else in this contract.
    pub const LOGS: Effects = Effects(1 << 7);

    /// Hands control to code we know nothing about — `CALL`, `CREATE`,
    /// `DELEGATECALL`. May re-enter this contract and do anything, so it is a
    /// barrier to everything.
    pub const EXTERNAL: Effects = Effects(1 << 8);

    /// Every effect. The conservative answer, and the fallback for an opcode
    /// this module has not classified — a new table entry is impure until
    /// someone says otherwise, never accidentally pure.
    pub const ALL: Effects = Effects(0x01ff);

    pub const fn union(self, other: Effects) -> Effects {
        Effects(self.0 | other.0)
    }

    /// Whether `self` has every effect in `other`.
    pub const fn contains(self, other: Effects) -> bool {
        self.0 & other.0 == other.0
    }

    /// Whether any effect in `other` is present.
    pub const fn intersects(self, other: Effects) -> bool {
        self.0 & other.0 != 0
    }

    pub const fn is_pure(self) -> bool {
        self.0 == 0
    }

    /// Whether removing this is allowed once its results are unused.
    pub const fn is_removable(self) -> bool {
        self.is_pure()
    }

    /// The effects of a raw EVM opcode, for the `Op::Opcode` escape hatch.
    ///
    /// Mostly a function of [`OpCategory`], but **not entirely**, and the
    /// exceptions are the whole reason this is a function rather than a lookup:
    ///
    /// - The `*COPY` family (`CALLDATACOPY`, `CODECOPY`, `RETURNDATACOPY`,
    ///   `EXTCODECOPY`) is categorised `Context` because of where its *source*
    ///   comes from, but every one of them **writes memory**. Trusting the
    ///   category here would let an optimizer move an `MLOAD` across a
    ///   `CODECOPY`.
    /// - `KECCAK256` is categorised `Memory` and reads it, which is right, but
    ///   is easy to mistake for an arithmetic op.
    /// - `LOG0`-`LOG4` take their payload from memory, so they read it too.
    pub fn of_opcode(spec: &OpSpec) -> Effects {
        match spec.byte {
            // --- memory, where the byte is more honest than the category ---
            0x20 => Effects::READS_MEMORY,                               // KECCAK256
            0x51 => Effects::READS_MEMORY,                               // MLOAD
            0x52 | 0x53 => Effects::WRITES_MEMORY,                       // MSTORE, MSTORE8
            0x59 => Effects::READS_MEMORY,                               // MSIZE
            0x5e => Effects::READS_MEMORY.union(Effects::WRITES_MEMORY), // MCOPY
            // CALLDATACOPY, CODECOPY, EXTCODECOPY, RETURNDATACOPY
            0x37 | 0x39 | 0x3c | 0x3e => Effects::READS_CONTEXT.union(Effects::WRITES_MEMORY),

            // --- storage: one category, four very different effects ---
            0x54 => Effects::READS_STORAGE,    // SLOAD
            0x55 => Effects::WRITES_STORAGE,   // SSTORE
            0x5c => Effects::READS_TRANSIENT,  // TLOAD
            0x5d => Effects::WRITES_TRANSIENT, // TSTORE

            _ => match spec.category {
                OpCategory::Arithmetic | OpCategory::Comparison | OpCategory::Bitwise => Effects::PURE,
                OpCategory::StackManip => Effects::PURE,
                OpCategory::Context | OpCategory::Block => Effects::READS_CONTEXT,
                OpCategory::Memory => Effects::READS_MEMORY.union(Effects::WRITES_MEMORY),
                OpCategory::Storage => Effects::READS_STORAGE.union(Effects::WRITES_STORAGE),
                // A log's payload is a memory range.
                OpCategory::Log => Effects::LOGS.union(Effects::READS_MEMORY),
                OpCategory::Call => Effects::EXTERNAL
                    .union(Effects::READS_MEMORY)
                    .union(Effects::WRITES_MEMORY),
                // `Control` here means PC and the like; EIR owns jumps, and
                // `Terminating` opcodes are `Terminator`s — neither reaches
                // `Op::Opcode`. Answer conservatively rather than inventing a
                // classification for something that should not appear.
                OpCategory::Control | OpCategory::Terminating => Effects::ALL,
            },
        }
    }
}

impl fmt::Display for Effects {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        const NAMES: [(Effects, &str); 9] = [
            (Effects::READS_MEMORY, "rmem"),
            (Effects::WRITES_MEMORY, "wmem"),
            (Effects::READS_STORAGE, "rsto"),
            (Effects::WRITES_STORAGE, "wsto"),
            (Effects::READS_TRANSIENT, "rtra"),
            (Effects::WRITES_TRANSIENT, "wtra"),
            (Effects::READS_CONTEXT, "rctx"),
            (Effects::LOGS, "log"),
            (Effects::EXTERNAL, "extern"),
        ];

        if self.is_pure() {
            return write!(f, "pure");
        }
        let mut first = true;
        for (effect, name) in NAMES {
            if self.contains(effect) {
                if !first {
                    write!(f, "|")?;
                }
                write!(f, "{name}")?;
                first = false;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
