//! Wrapping runtime code in initcode, so it can be deployed rather than
//! planted.
//!
//! This is a stopgap. `evm-link` (build order step 3) owns deployment
//! properly — constructor arguments, immutables, multiple contracts per crate.
//! What lives here is the minimum needed to exercise the `CREATE` path from a
//! test: a prologue that copies the runtime code out of its own initcode and
//! returns it.

use evm_codegen::{Asm, AsmError};
use evm_isa::Fork;

/// Initcode that deploys `runtime` verbatim.
///
/// The prologue is the conventional one:
///
/// ```text
/// PUSH len        ; len
/// DUP1            ; len len
/// PUSH offset     ; offset len len
/// PUSH 0          ; 0 offset len len
/// CODECOPY        ; memory[0..len] = initcode[offset..offset+len]
/// PUSH 0          ; 0 len
/// RETURN          ; deploy memory[0..len]
/// ```
pub fn deployer(runtime: &[u8], fork: Fork) -> Result<Vec<u8>, AsmError> {
    // `offset` is the prologue's own length — and the prologue's length
    // depends on the PUSH width the assembler picks for that very offset.
    // Iterate to a fixpoint. Widening an immediate only ever grows the
    // prologue, so the sequence is non-decreasing and bounded: it converges,
    // in practice after one or two rounds.
    let mut offset = 0usize;
    loop {
        let prologue = assemble_prologue(runtime.len(), offset, fork)?;
        if prologue.len() == offset {
            let mut initcode = prologue;
            initcode.extend_from_slice(runtime);
            return Ok(initcode);
        }
        offset = prologue.len();
    }
}

fn assemble_prologue(len: usize, offset: usize, fork: Fork) -> Result<Vec<u8>, AsmError> {
    let mut a = Asm::new(fork);
    a.push(len as u64)
        .dup(1)
        .push(offset as u64)
        .push(0u8)
        .op("CODECOPY")
        .push(0u8)
        .op("RETURN");
    a.finish()
}
