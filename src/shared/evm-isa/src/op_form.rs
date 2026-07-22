use crate::spec::OpSpec;

/// `PUSH1`-`PUSH32`, `DUP1`-`DUP16`, `SWAP1`-`SWAP16` are 64 opcodes but
/// really three families with an immediate/depth parameter. `evm-asm`
/// computes which one to emit from a value's byte width rather than treating
/// each as an unrelated opcode.
///
/// This gets no binding in `evm-sys` — the compiler owns stack manipulation,
/// and a user calling `dup3()` directly would corrupt the scheduler's
/// invariants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpForm {
    Simple,
    /// Immediate width in bytes, `0..=32` (`Push(0)` is `PUSH0`).
    Push(u8),
    /// Stack depth, `1..=16`.
    Dup(u8),
    /// Stack depth, `1..=16`.
    Swap(u8),
}

/// Recovers the structural family of an opcode from its byte value.
///
/// `category: StackManip` alone can't distinguish `PUSH` from `DUP` from
/// `SWAP` (all three share it), so this exists rather than forcing every
/// consumer to parse the mnemonic string.
pub const fn op_form(spec: &OpSpec) -> OpForm {
    match spec.byte {
        0x5f => OpForm::Push(0),
        0x60..=0x7f => OpForm::Push(spec.byte - 0x5f),
        0x80..=0x8f => OpForm::Dup(spec.byte - 0x7f),
        0x90..=0x9f => OpForm::Swap(spec.byte - 0x8f),
        _ => OpForm::Simple,
    }
}
