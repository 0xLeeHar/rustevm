use crate::U256;
use evm_sys_macros::evm_opcode;

/// BALANCE — ETH balance of `addr`.
#[evm_opcode(BALANCE)]
pub fn balance(addr: U256) -> U256 {
    unsafe { core::hint::unreachable_unchecked() }
}

/// EXTCODESIZE — byte length of the code at `addr`.
#[evm_opcode(EXTCODESIZE)]
pub fn extcodesize(addr: U256) -> U256 {
    unsafe { core::hint::unreachable_unchecked() }
}

/// EXTCODECOPY — copy `len` bytes of `addr`'s code at `src` into memory at `dst`.
#[evm_opcode(EXTCODECOPY)]
pub unsafe fn extcodecopy(addr: U256, dst: u32, src: u32, len: u32) {
    unsafe { core::hint::unreachable_unchecked() }
}

/// EXTCODEHASH — keccak256 hash of `addr`'s code (EIP-1052).
#[evm_opcode(EXTCODEHASH)]
pub fn extcodehash(addr: U256) -> U256 {
    unsafe { core::hint::unreachable_unchecked() }
}

/// CODESIZE — byte length of the currently executing contract's code.
#[evm_opcode(CODESIZE)]
pub fn codesize() -> U256 {
    unsafe { core::hint::unreachable_unchecked() }
}

/// CODECOPY — copy `len` bytes of the current contract's code at `src` into memory at `dst`.
#[evm_opcode(CODECOPY)]
pub unsafe fn codecopy(dst: u32, src: u32, len: u32) {
    unsafe { core::hint::unreachable_unchecked() }
}

/// SELFDESTRUCT — send contract balance to `addr` and mark contract for deletion.
#[evm_opcode(SELFDESTRUCT)]
pub unsafe fn selfdestruct(addr: U256) -> ! {
    unsafe { core::hint::unreachable_unchecked() }
}
