use crate::U256;
use evm_sys_macros::evm_opcode;

/// CALL. Returns 1 on success, 0 on revert (as the opcode does).
///   gas, addr, value: pushed as words
///   in_offset/in_len: calldata region in memory
///   out_offset/out_len: where to write returndata
#[evm_opcode(CALL)]
pub unsafe fn call(
    gas: U256,
    addr: U256,
    value: U256,
    in_offset: u32,
    in_len: u32,
    out_offset: u32,
    out_len: u32,
) -> u32 {
    unsafe { core::hint::unreachable_unchecked() }
}

/// STATICCALL — like CALL but no value, disallows state changes.
#[evm_opcode(STATICCALL)]
pub unsafe fn staticcall(
    gas: U256,
    addr: U256,
    in_offset: u32,
    in_len: u32,
    out_offset: u32,
    out_len: u32,
) -> u32 {
    unsafe { core::hint::unreachable_unchecked() }
}

/// DELEGATECALL — runs target code in caller's context.
#[evm_opcode(DELEGATECALL)]
pub unsafe fn delegatecall(
    gas: U256,
    addr: U256,
    in_offset: u32,
    in_len: u32,
    out_offset: u32,
    out_len: u32,
) -> u32 {
    unsafe { core::hint::unreachable_unchecked() }
}

/// CALLCODE — like CALL but runs target code in caller's storage context.
#[evm_opcode(CALLCODE)]
pub unsafe fn callcode(
    gas: U256,
    addr: U256,
    value: U256,
    in_offset: u32,
    in_len: u32,
    out_offset: u32,
    out_len: u32,
) -> u32 {
    unsafe { core::hint::unreachable_unchecked() }
}

/// CREATE — deploy a new contract; returns new contract address or 0 on failure.
#[evm_opcode(CREATE)]
pub unsafe fn create(value: U256, offset: u32, size: u32) -> U256 {
    unsafe { core::hint::unreachable_unchecked() }
}

/// CREATE2 — deploy with deterministic address derived from `salt`.
#[evm_opcode(CREATE2)]
pub unsafe fn create2(value: U256, offset: u32, size: u32, salt: U256) -> U256 {
    unsafe { core::hint::unreachable_unchecked() }
}
