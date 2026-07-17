#![no_std]

use std_evm::{U256, contract};

pub struct MyFirstContract;

#[contract]
impl MyFirstContract {
    fn test() {
        let t: U256 = U256::ZERO;
    }
}
