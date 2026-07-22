#![cfg_attr(not(test), no_std)]

//! Canonical, machine-readable definition of the EVM instruction set.
//!
//! Pure data, no behaviour — see `design/evm-isa-spec.md` for the full
//! rationale. This crate has zero dependencies and must build on stable
//! Rust: it's read by a nightly-only compiler backend *and* a stable-only
//! proc-macro crate, and the redundancy between what the macro stamps onto a
//! binding and what this table says is the safety mechanism that catches a
//! mismatched arity at compile time.

mod category;
mod fork;
mod gas;
mod lookup;
mod op_form;
mod spec;
mod table;

pub use category::OpCategory;
pub use fork::Fork;
pub use gas::GasClass;
pub use lookup::{by_byte, by_mnemonic};
pub use op_form::{OpForm, op_form};
pub use spec::OpSpec;
pub use table::OPCODES;

#[cfg(test)]
mod tests;
