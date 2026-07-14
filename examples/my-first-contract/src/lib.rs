#![no_std]

use std_evm::{contract, U256};

pub struct MyFirstContract;

#[contract]
impl MyFirstContract {
    fn test() {
        let t: U256 = U256::ZERO;
    }
}