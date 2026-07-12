use crate::U256;
use evm_sys_macros::evm_opcode;

// control.rs
/// RETURN — halt and return `len` bytes of memory from `offset`.
#[evm_opcode(RETURN)]
pub unsafe fn ret(offset: u32, len: u32) -> ! {
    unsafe { core::hint::unreachable_unchecked() }
}

/// REVERT — halt, revert state, return `len` bytes as revert data.
#[evm_opcode(REVERT)]
pub unsafe fn revert(offset: u32, len: u32) -> ! {
    unsafe { core::hint::unreachable_unchecked() }
}

/// STOP — halt successfully with no return data.
#[evm_opcode(STOP)]
pub unsafe fn stop() -> ! {
    unsafe { core::hint::unreachable_unchecked() }
}

/// INVALID — consume all gas and halt with an invalid instruction.
#[evm_opcode(INVALID)]
pub unsafe fn invalid() -> ! {
    unsafe { core::hint::unreachable_unchecked() }
}

/// PC — program counter of the current instruction.
#[evm_opcode(PC)]
pub unsafe fn pc() -> U256 {
    unsafe { core::hint::unreachable_unchecked() }
}

/// JUMP — unconditional jump to `counter`.
#[evm_opcode(JUMP)]
pub unsafe fn jump(counter: U256) {
    unsafe { core::hint::unreachable_unchecked() }
}

/// JUMPI — conditional jump to `counter` if `condition` is non-zero.
#[evm_opcode(JUMPI)]
pub unsafe fn jumpi(counter: U256, condition: U256) {
    unsafe { core::hint::unreachable_unchecked() }
}

/// JUMPDEST — valid jump destination marker; no-op at runtime.
#[evm_opcode(JUMPDEST)]
pub fn jumpdest() {
    unsafe { core::hint::unreachable_unchecked() }
}

/// POP — discard the top stack value.
#[evm_opcode(POP)]
pub fn pop(value: U256) {
    unsafe { core::hint::unreachable_unchecked() }
}
