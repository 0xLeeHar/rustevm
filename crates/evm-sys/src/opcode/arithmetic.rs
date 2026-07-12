use crate::U256;
use evm_sys_macros::evm_opcode;

/// ADD — a + b mod 2^256.
#[evm_opcode(ADD)]
pub fn add(a: U256, b: U256) -> U256 {
    unsafe { core::hint::unreachable_unchecked() }
}

/// MUL — a * b mod 2^256.
#[evm_opcode(MUL)]
pub fn mul(a: U256, b: U256) -> U256 {
    unsafe { core::hint::unreachable_unchecked() }
}

/// SUB — a - b mod 2^256.
#[evm_opcode(SUB)]
pub fn sub(a: U256, b: U256) -> U256 {
    unsafe { core::hint::unreachable_unchecked() }
}

/// DIV — integer division a / b; 0 if b == 0.
#[evm_opcode(DIV)]
pub fn div(a: U256, b: U256) -> U256 {
    unsafe { core::hint::unreachable_unchecked() }
}

/// SDIV — signed integer division; 0 if b == 0.
#[evm_opcode(SDIV)]
pub fn sdiv(a: U256, b: U256) -> U256 {
    unsafe { core::hint::unreachable_unchecked() }
}

/// MOD — a % b; 0 if b == 0.
#[evm_opcode(MOD)]
pub fn mod_(a: U256, b: U256) -> U256 {
    unsafe { core::hint::unreachable_unchecked() }
}

/// SMOD — signed modulo; 0 if b == 0.
#[evm_opcode(SMOD)]
pub fn smod(a: U256, b: U256) -> U256 {
    unsafe { core::hint::unreachable_unchecked() }
}

/// ADDMOD — (a + b) % n; 0 if n == 0.
#[evm_opcode(ADDMOD)]
pub fn addmod(a: U256, b: U256, n: U256) -> U256 {
    unsafe { core::hint::unreachable_unchecked() }
}

/// MULMOD — (a * b) % n; 0 if n == 0.
#[evm_opcode(MULMOD)]
pub fn mulmod(a: U256, b: U256, n: U256) -> U256 {
    unsafe { core::hint::unreachable_unchecked() }
}

/// EXP — a ** exponent mod 2^256.
#[evm_opcode(EXP)]
pub fn exp(a: U256, exponent: U256) -> U256 {
    unsafe { core::hint::unreachable_unchecked() }
}

/// SIGNEXTEND — sign-extend x from bit b*8+7.
#[evm_opcode(SIGNEXTEND)]
pub fn signextend(b: U256, x: U256) -> U256 {
    unsafe { core::hint::unreachable_unchecked() }
}
