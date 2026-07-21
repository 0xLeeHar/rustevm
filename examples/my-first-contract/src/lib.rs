#![no_std]

use std_evm::{U256, constructor, contract};

pub struct MyFirstContract;

#[contract]
impl MyFirstContract {
    pub fn init() {}
}
