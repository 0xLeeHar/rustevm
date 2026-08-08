//! `evm-codegen` — the backend pipeline: IR, optimization, stackification,
//! assembly, and linking, as one crate with internal modules (see
//! `design/rust-evm-compiler-design.md` §18).
//!
//! Only [`asm`] exists so far — the assembler and its inverse, [`disasm`]. It
//! is the bottom of the pipeline and the piece everything above it is checked
//! against: per the design doc's build order, hand-written opcodes that
//! assemble and run on revm come *before* an IR, so that the IR has something
//! known-good to be debugged against.
//!
//! All opcode data comes from `evm-isa`. Nothing here restates a byte value,
//! an arity, or a fork gate — the whole point of that crate is that the
//! compiler and `evm-sys-macros` read one table.

pub mod asm;
pub mod ir;

pub use asm::disasm;
pub use asm::{Asm, AsmError, Label, MAX_CONTRACT_SIZE};
pub use disasm::{Instruction, disassemble, jumpdests, listing};
pub use ir::{Block, Effects, Func, Inst, IntTy, Repair, Signature, Type, Value};
