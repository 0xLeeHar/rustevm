use crate::U256;
use evm_sys_macros::evm_opcode;

/// ADDRESS — address of the currently executing contract.
#[evm_opcode(ADDRESS)]
pub fn address() -> U256 {
    unsafe { core::hint::unreachable_unchecked() }
}

/// CALLER — msg.sender.
#[evm_opcode(CALLER)]
pub fn caller() -> U256 {
    unsafe { core::hint::unreachable_unchecked() }
}

/// CALLVALUE — msg.value in wei.
#[evm_opcode(CALLVALUE)]
pub fn callvalue() -> U256 {
    unsafe { core::hint::unreachable_unchecked() }
}

/// ORIGIN — tx.origin.
#[evm_opcode(ORIGIN)]
pub fn origin() -> U256 {
    unsafe { core::hint::unreachable_unchecked() }
}

/// GAS — remaining gas.
#[evm_opcode(GAS)]
pub fn gas() -> U256 {
    unsafe { core::hint::unreachable_unchecked() }
}

/// GASPRICE — effective gas price of the current transaction.
#[evm_opcode(GASPRICE)]
pub fn gasprice() -> U256 {
    unsafe { core::hint::unreachable_unchecked() }
}

/// SELFBALANCE — ETH balance of the currently executing contract.
#[evm_opcode(SELFBALANCE)]
pub fn selfbalance() -> U256 {
    unsafe { core::hint::unreachable_unchecked() }
}
