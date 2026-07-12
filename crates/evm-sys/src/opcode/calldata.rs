use crate::U256;
use evm_sys_macros::evm_opcode;

/// CALLDATALOAD — load 32 bytes of calldata at `offset`.
#[evm_opcode(CALLDATALOAD)]
pub unsafe fn calldataload(offset: U256) -> U256 {
    unsafe { core::hint::unreachable_unchecked() }
}

/// CALLDATASIZE — byte length of the calldata.
#[evm_opcode(CALLDATASIZE)]
pub unsafe fn calldatasize() -> U256 {
    unsafe { core::hint::unreachable_unchecked() }
}

/// CALLDATACOPY — copy `len` bytes of calldata at `src` into memory at `dst`.
#[evm_opcode(CALLDATACOPY)]
pub unsafe fn calldatacopy(dst: u32, src: u32, len: u32) {
    unsafe { core::hint::unreachable_unchecked() }
}

/// RETURNDATASIZE — byte length of the last sub-call's return data.
#[evm_opcode(RETURNDATASIZE)]
pub unsafe fn returndatasize() -> U256 {
    unsafe { core::hint::unreachable_unchecked() }
}

/// RETURNDATACOPY — copy `len` bytes of return data at `src` into memory at `dst`.
#[evm_opcode(RETURNDATACOPY)]
pub unsafe fn returndatacopy(dst: u32, src: u32, len: u32) {
    unsafe { core::hint::unreachable_unchecked() }
}
