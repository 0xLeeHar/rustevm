use crate::U256;
use evm_sys_macros::evm_opcode;

#[evm_opcode(LOG0)]
pub unsafe fn log0(offset: u32, size: u32) {
    unsafe { core::hint::unreachable_unchecked() }
}

#[evm_opcode(LOG1)]
pub unsafe fn log1(offset: u32, size: u32, topic0: U256) {
    unsafe { core::hint::unreachable_unchecked() }
}

#[evm_opcode(LOG2)]
pub unsafe fn log2(offset: u32, size: u32, topic0: U256, topic1: U256) {
    unsafe { core::hint::unreachable_unchecked() }
}

#[evm_opcode(LOG3)]
pub unsafe fn log3(offset: u32, size: u32, topic0: U256, topic1: U256, topic2: U256) {
    unsafe { core::hint::unreachable_unchecked() }
}

#[evm_opcode(LOG4)]
pub unsafe fn log4(offset: u32, size: u32, topic0: U256, topic1: U256, topic2: U256, topic3: U256) {
    unsafe { core::hint::unreachable_unchecked() }
}
