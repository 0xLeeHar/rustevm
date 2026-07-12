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
//! directly instead of generating a function call.  See [`intrinsics`].

#![feature(rustc_private)]
#![deny(unsafe_op_in_unsafe_fn)]
#![allow(internal_features)]
// The compiler-internal crates do not follow our lint configuration.
#![allow(rustc::untranslatable_diagnostic)]

// ── compiler-internal crates ────────────────────────────────────────────────
// These are provided by the installed rustc via `rustc-dev`; they are NOT
// normal Cargo dependencies.
extern crate rustc_abi;
extern crate rustc_ast;
extern crate rustc_codegen_ssa;
extern crate rustc_data_structures;
extern crate rustc_errors;
extern crate rustc_hir;
extern crate rustc_metadata;
extern crate rustc_middle;
extern crate rustc_session;
extern crate rustc_span;
extern crate rustc_target;

// ── modules ──────────────────────────────────────────────────────────────────
mod abi;
mod backend;
mod context;
mod emit;
mod intrinsics;

pub use backend::EvmCodegenBackend;

// ── entry point ──────────────────────────────────────────────────────────────

/// The symbol rustc looks for when it dlopen-s a codegen backend.
///
/// Point rustc at the compiled dylib:
/// ```sh
/// rustc -Z codegen-backend=/path/to/librustc_codegen_evm.dylib ...
/// ```
#[no_mangle]
pub fn __rustc_codegen_backend() -> Box<dyn rustc_codegen_ssa::traits::CodegenBackend> {
    Box::new(EvmCodegenBackend::new())
}
