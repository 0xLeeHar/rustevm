#![no_std]

use std_evm::{Address, U256, contract};

pub struct MyFirstContract;

#[contract]
impl MyFirstContract {
    #[constructor]
    pub fn init(initial_supply: U256) {
        let _ = initial_supply;
    }

    #[payable]
    pub fn deposit() {}

    #[selector("balanceOf(address)")]
    pub fn balance_of(who: Address) -> U256 {
        let _ = who;
        U256::ZERO
    }
}
