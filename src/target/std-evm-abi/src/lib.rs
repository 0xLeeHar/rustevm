//! `std-evm-abi` — shared Ethereum ABI logic.
//!
//! The ABI-side counterpart to `evm-isa`: a single crate on the macro↔runtime
//! boundary, depended on by both `std-evm-macros` (at compile time, to compute
//! selectors and type names) and `std-evm` (at runtime, to encode/decode calldata
//! and reverts). This keeps one implementation of the ABI rules and breaks the
//! `std-evm` ↔ `std-evm-macros` dependency cycle.
//!
//! Opcode-free by construction — it depends only on `core`, `alloc`, and a keccak
//! implementation, never on `evm-sys`.

#![no_std]

extern crate alloc;

pub mod codec;
pub mod revert;
pub mod selector;
pub mod types;
mod word;

pub use codec::{
    AbiDecode, AbiDecodeArgs, AbiEncode, AbiEncodeOutput, Component, DecodeError, encode_tuple, read_word,
};
pub use revert::{ERROR_SELECTOR, PANIC_SELECTOR, encode_error, encode_panic};
pub use selector::{keccak256, selector};
pub use types::{camel_case, solidity_type_name};
pub use word::{U256, Word};
