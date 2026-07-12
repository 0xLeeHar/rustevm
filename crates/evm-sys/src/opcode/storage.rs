use crate::U256;
use evm_sys_macros::evm_opcode;

/// SLOAD — read a storage slot.
#[evm_opcode(SLOAD)]
pub unsafe fn sload(slot: U256) -> U256 {
    unsafe { core::hint::unreachable_unchecked() }
}

/// SSTORE — write a storage slot.
#[evm_opcode(SSTORE)]
pub unsafe fn sstore(slot: U256, value: U256) {
    unsafe { core::hint::unreachable_unchecked() }
}

/// TLOAD — read transient storage (Cancun+).
#[evm_opcode(TLOAD)]
pub unsafe fn tload(slot: U256) -> U256 {
    unsafe { core::hint::unreachable_unchecked() }
}

/// TSTORE — write transient storage (Cancun+).
#[evm_opcode(TSTORE)]
pub unsafe fn tstore(slot: U256, value: U256) {
    unsafe { core::hint::unreachable_unchecked() }
}
