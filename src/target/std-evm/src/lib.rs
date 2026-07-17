#![no_std]

pub use evm_sys::U256;
pub use std_evm_macros::{constructor, contract};

pub fn add(a: u32, b: u32) -> u32 {
    a + b
}
