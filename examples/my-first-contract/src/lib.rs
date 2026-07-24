#![no_std]

use std_evm::{Address, Storage, U256, contract, storage};

pub struct MyFirstContract;

#[storage]
pub struct MyFirstContractStorage {
    total_supply: U256,
    balances: Mapping<Address, U256>,
}

#[contract]
impl MyFirstContract {
    #[constructor]
    pub fn init(s: &mut Storage<MyFirstContractStorage>, initial_supply: U256) {
        s.set_total_supply(initial_supply);
    }

    #[payable]
    pub fn deposit(s: &mut Storage<MyFirstContractStorage>, to: Address, amount: U256) {
        s.balances_mut().set(to, amount);
    }

    pub fn total_supply(s: &Storage<MyFirstContractStorage>) -> U256 {
        s.total_supply()
    }

    pub fn balance_of(s: &Storage<MyFirstContractStorage>, who: Address) -> U256 {
        s.balances().get(who)
    }
}
