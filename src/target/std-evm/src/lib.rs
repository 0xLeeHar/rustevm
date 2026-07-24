#![no_std]

//! `std-evm` — the user-facing runtime layer for writing EVM contracts in
//! Rust. Traits do the work (`Method`/`Dispatch`/`Contract`); the macros
//! re-exported here (from `std-evm-macros`) only fill the two reflection
//! gaps Rust's trait system can't: enumerating "all methods" and
//! enumerating "all fields." See `design/std-evm-macros-spec.md`.

extern crate alloc;

pub use alloc::vec::Vec;

mod address;
mod contract;
mod dispatch;
mod guard;
mod method;
mod storage;

pub use address::Address;
pub use contract::Contract;
pub use dispatch::{Dispatch, no_matching_method};
pub use evm_sys::U256;
pub use guard::guard_not_payable;
pub use method::Method;
pub use std_evm_abi::{AbiDecodeArgs, encode_error};
pub use std_evm_macros::{constructor, contract, fallback, payable, receive, selector};
pub use storage::Storage;
