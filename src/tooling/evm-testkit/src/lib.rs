//! `evm-testkit` — run assembled bytecode on a real EVM and assert on what it did.
//!
//! The assembler's own tests assert on *bytes*: they prove the encoder agrees
//! with `evm-isa`, not that those bytes do the right thing when executed. This
//! crate closes that loop. It is the third piece of the design doc's step 1
//! ("hand-write ops, assemble, run on revm") and the known-good oracle every
//! layer added above the assembler gets debugged against — per §16, revm
//! execution *is* the debugger, so a failed assertion prints the full
//! instruction trace rather than a bare `assertion failed`.
//!
//! ```
//! use evm_codegen::Asm;
//! use evm_isa::Fork;
//! use evm_testkit::Testkit;
//!
//! let mut a = Asm::new(Fork::Cancun);
//! a.push(2u8)
//!     .push(3u8)
//!     .op("ADD")
//!     .push(0u8)
//!     .op("MSTORE")
//!     .push(32u8)
//!     .push(0u8)
//!     .op("RETURN");
//!
//! let mut tk = Testkit::new(Fork::Cancun);
//! assert_eq!(tk.run_asm(a).returned_u64(), 5);
//! ```
//!
//! Mnemonics in a trace are resolved through `evm-isa`, the same table the
//! assembler emits from — nothing here keeps its own copy of the opcode set.

mod fork;
mod harness;
mod link;
mod outcome;
mod trace;

pub use harness::Testkit;
pub use link::deployer;
pub use outcome::{Outcome, Status};
pub use trace::{Step, Trace};

/// revm types that appear in this crate's signatures, re-exported so a test
/// does not need its own `revm` dependency to name an address or a word.
pub use revm::primitives::{Address, Bytes, Log, U256, address};

#[cfg(test)]
mod tests;
