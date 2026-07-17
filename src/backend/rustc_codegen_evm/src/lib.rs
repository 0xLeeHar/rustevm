//! `rustc_codegen_evm` — A rustc codegen backend that emits EVM bytecode.
//!
//! # Architecture
//!
//! ```text
//! Rust source
//!     │  rustc frontend (parsing → HIR → MIR)
//!     ▼
//! MIR (Mid-level Intermediate Representation)
//!     │  this crate
//!     ▼
//! EVM bytecode  (.evm / hex output)
//! ```
//!
//! # How rustc loads a codegen backend
//!
//! rustc opens the compiled dylib and calls [`__rustc_codegen_backend`] to
//! obtain a [`CodegenBackend`] trait object.  Every method on that trait is
//! then called at the appropriate phase of compilation.
//!
//! # Usage
//!
//! ```sh
//! # Build the backend first:
//! cargo +nightly build -p rustc_codegen_evm
//!
//! # Then compile a contract:
//! rustc +nightly \
//!     -Z codegen-backend=target/debug/librustc_codegen_evm.dylib \
//!     mycontract.rs
//! ```
//!
//! # Relationship to `evm-sys`
//!
//! Functions in the `evm-sys` crate are declared with `unreachable_unchecked()`
//! bodies.  This backend recognises calls to those functions by their link-name
//! (e.g. `evm_sys::arithmetic::add`) and emits the corresponding EVM opcode
//! directly instead of generating a function call.

#![feature(rustc_private)]
#![deny(unsafe_op_in_unsafe_fn)]
#![allow(internal_features)]
#![allow(rustc::untranslatable_diagnostic)]

// These are provided by the installed rustc via `rustc-dev`. They are NOT normal Cargo dependencies.
extern crate rustc_abi;
extern crate rustc_ast;
extern crate rustc_codegen_ssa;
extern crate rustc_data_structures;
extern crate rustc_driver;
extern crate rustc_errors;
extern crate rustc_hir;
extern crate rustc_metadata;
extern crate rustc_middle;
extern crate rustc_session;
extern crate rustc_span;
extern crate rustc_target;

pub use backend::EvmCodegenBackend;

mod backend;

#[unsafe(no_mangle)]
pub fn __rustc_codegen_backend() -> Box<dyn rustc_codegen_ssa::traits::CodegenBackend> {
    println!("Starting EVM codegen backend");
    Box::new(EvmCodegenBackend::new())
}
