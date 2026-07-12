use crate::U256;
use evm_sys_macros::evm_opcode;

/// MLOAD — load 32 bytes from memory at `offset`.
#[evm_opcode(MLOAD)]
pub unsafe fn mload(offset: u32) -> U256 {
    unsafe { core::hint::unreachable_unchecked() }
}

/// MSTORE — store a 32-byte word to memory at `offset`.
#[evm_opcode(MSTORE)]
pub unsafe fn mstore(offset: u32, value: U256) {
    unsafe { core::hint::unreachable_unchecked() }
}

/// MSTORE8 — store a single byte (low 8 bits of `value`) at `offset`.
#[evm_opcode(MSTORE8)]
pub unsafe fn mstore8(offset: u32, value: U256) {
    unsafe { core::hint::unreachable_unchecked() }
}

/// MCOPY — copy `len` bytes from `src` to `dst` (Cancun+).
#[evm_opcode(MCOPY)]
pub unsafe fn mcopy(dst: u32, src: u32, len: u32) {
    unsafe { core::hint::unreachable_unchecked() }
}

/// MSIZE — size of active memory in bytes, rounded up to the next 32-byte word.
#[evm_opcode(MSIZE)]
pub unsafe fn msize() -> U256 {
    unsafe { core::hint::unreachable_unchecked() }
}
