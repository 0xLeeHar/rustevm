use crate::U256;
use evm_sys_macros::evm_opcode;

/// KECCAK256 — hash `len` bytes of EVM memory starting at `offset`.
/// Caller guarantees [offset, offset+len) is valid initialized memory.
#[evm_opcode(KECCAK256)]
pub unsafe fn keccak256(offset: u32, len: u32) -> U256 {
    unsafe { core::hint::unreachable_unchecked() }
}
