use crate::U256;
use evm_sys_macros::evm_opcode;

/// LT — 1 if a < b (unsigned), else 0.
#[evm_opcode(LT)]
pub fn lt(a: U256, b: U256) -> U256 {
    unsafe { core::hint::unreachable_unchecked() }
}

/// GT — 1 if a > b (unsigned), else 0.
#[evm_opcode(GT)]
pub fn gt(a: U256, b: U256) -> U256 {
    unsafe { core::hint::unreachable_unchecked() }
}

/// SLT — 1 if a < b (signed), else 0.
#[evm_opcode(SLT)]
pub fn slt(a: U256, b: U256) -> U256 {
    unsafe { core::hint::unreachable_unchecked() }
}

/// SGT — 1 if a > b (signed), else 0.
#[evm_opcode(SGT)]
pub fn sgt(a: U256, b: U256) -> U256 {
    unsafe { core::hint::unreachable_unchecked() }
}

/// EQ — 1 if a == b, else 0.
#[evm_opcode(EQ)]
pub fn eq(a: U256, b: U256) -> U256 {
    unsafe { core::hint::unreachable_unchecked() }
}

/// ISZERO — 1 if a == 0, else 0.
#[evm_opcode(ISZERO)]
pub fn iszero(a: U256) -> U256 {
    unsafe { core::hint::unreachable_unchecked() }
}

/// AND — bitwise AND.
#[evm_opcode(AND)]
pub fn and(a: U256, b: U256) -> U256 {
    unsafe { core::hint::unreachable_unchecked() }
}

/// OR — bitwise OR.
#[evm_opcode(OR)]
pub fn or(a: U256, b: U256) -> U256 {
    unsafe { core::hint::unreachable_unchecked() }
}

/// XOR — bitwise XOR.
#[evm_opcode(XOR)]
pub fn xor(a: U256, b: U256) -> U256 {
    unsafe { core::hint::unreachable_unchecked() }
}

/// NOT — bitwise NOT.
#[evm_opcode(NOT)]
pub fn not(a: U256) -> U256 {
    unsafe { core::hint::unreachable_unchecked() }
}

/// BYTE — i-th byte of x (0 = most significant); 0 if i >= 32.
#[evm_opcode(BYTE)]
pub fn byte(i: U256, x: U256) -> U256 {
    unsafe { core::hint::unreachable_unchecked() }
}

/// SHL — logical shift left: value << shift; 0 if shift >= 256.
#[evm_opcode(SHL)]
pub fn shl(shift: U256, value: U256) -> U256 {
    unsafe { core::hint::unreachable_unchecked() }
}

/// SHR — logical shift right: value >> shift; 0 if shift >= 256.
#[evm_opcode(SHR)]
pub fn shr(shift: U256, value: U256) -> U256 {
    unsafe { core::hint::unreachable_unchecked() }
}

/// SAR — arithmetic (signed) shift right.
#[evm_opcode(SAR)]
pub fn sar(shift: U256, value: U256) -> U256 {
    unsafe { core::hint::unreachable_unchecked() }
}
