use crate::category::OpCategory;
use crate::fork::Fork;
use crate::gas::GasClass;

/// A complete description of one EVM opcode.
///
/// Every operand the opcode pops is a parameter in `stack_in`, including
/// memory offsets and lengths (`KECCAK256` pops `(offset, len)` →
/// `stack_in: 2`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpSpec {
    pub mnemonic: &'static str,
    pub byte: u8,
    pub stack_in: u8,
    pub stack_out: u8,
    pub terminating: bool,
    pub gas: GasClass,
    pub category: OpCategory,
    pub min_fork: Fork,
}

impl OpSpec {
    /// Foldable at compile time: deterministic given its stack inputs alone,
    /// no side effects. `ADD` is pure; `TIMESTAMP` (reads external state,
    /// zero stack inputs but a non-constant result) is not.
    pub const fn is_pure(&self) -> bool {
        matches!(
            self.category,
            OpCategory::Arithmetic | OpCategory::Comparison | OpCategory::Bitwise
        )
    }

    /// `SLOAD`/`TLOAD` read state: same category as their write counterparts,
    /// distinguished here by producing a stack output.
    pub const fn reads_state(&self) -> bool {
        matches!(self.category, OpCategory::Storage) && self.stack_out != 0
    }

    /// `SSTORE`/`TSTORE` write state: same category as their read
    /// counterparts, distinguished here by producing no stack output.
    pub const fn writes_state(&self) -> bool {
        matches!(self.category, OpCategory::Storage) && self.stack_out == 0
    }

    /// `PUSH`/`DUP`/`SWAP`/`POP` — pure stack-shape manipulation.
    pub const fn is_stack_manip(&self) -> bool {
        matches!(self.category, OpCategory::StackManip)
    }
}
