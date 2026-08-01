//! The assembler run backwards: bytecode in, instructions out.
//!
//! Design doc §16 lists this among the tools that have to be built rather than
//! borrowed, because LLVM's are absent. It is what turns a revm trace, an
//! `Asm` buffer, or a chunk of on-chain bytecode back into something readable.
//!
//! Decoding has to start at offset zero and walk forwards, because a `PUSH`'s
//! immediate is indistinguishable from code by inspection — the same byte
//! means `JUMPDEST` in one position and `0x5b` of data in another. That is
//! also why [`jumpdests`] exists: scanning for `0x5b` finds targets the EVM
//! will reject.
//!
//! ```
//! # use evm_codegen::disasm::listing;
//! // PUSH1 0x02, PUSH1 0x03, ADD
//! let text = listing(&[0x60, 0x02, 0x60, 0x03, 0x01]).to_string();
//! assert_eq!(text, "0000  PUSH1 0x02\n0002  PUSH1 0x03\n0004  ADD\n");
//! ```
//!
//! Output is not guaranteed to reassemble to the same bytes. [`Asm::push`]
//! narrows an immediate to the width that holds it, so a `PUSH2 0x0001` — what
//! [`Asm::push_label`] emits for every jump — comes back as `PUSH1 0x01`. The
//! listing is for reading, not for round-tripping.
//!
//! [`Asm::push`]: crate::Asm::push
//! [`Asm::push_label`]: crate::Asm::push_label

use std::fmt;

use evm_isa::{OpForm, OpSpec, by_byte, op_form};

/// What a byte with no entry in `evm-isa`'s table is called.
///
/// Not a real mnemonic — the EVM has no `INVALID` instruction, it simply
/// aborts the frame on an opcode it does not recognise. (`0xfe` is the
/// designated invalid opcode and *is* in the table.)
const UNKNOWN: &str = "INVALID";

/// One decoded instruction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Instruction<'a> {
    /// Offset of the opcode byte.
    pub pc: usize,
    pub opcode: u8,
    /// `None` for a byte the table has no entry for.
    pub spec: Option<&'static OpSpec>,
    /// A `PUSH`'s immediate, borrowed from the code. Empty for everything
    /// else, including `PUSH0`.
    pub immediate: &'a [u8],
    /// Set when the code ended mid-immediate: `immediate` holds the bytes that
    /// were actually there, which is fewer than the opcode called for.
    ///
    /// Real bytecode hits this at the end of a contract, where solc's metadata
    /// blob gets decoded as instructions and runs out mid-`PUSH`.
    pub truncated: bool,
}

impl Instruction<'_> {
    /// The mnemonic, or [`UNKNOWN`] for a byte not in the table.
    pub fn mnemonic(&self) -> &'static str {
        self.spec.map_or(UNKNOWN, |spec| spec.mnemonic)
    }

    /// Bytes this instruction occupies: the opcode plus whatever immediate was
    /// present. Add [`missing`](Self::missing) for the width it wanted.
    pub fn size(&self) -> usize {
        1 + self.immediate.len()
    }

    /// Immediate bytes the opcode called for but the code did not have.
    /// Zero unless [`truncated`](Self::truncated).
    pub fn missing(&self) -> usize {
        self.immediate_width() - self.immediate.len()
    }

    /// Immediate width this opcode carries: `n` for `PUSH{n}`, zero otherwise.
    pub fn immediate_width(&self) -> usize {
        match self.spec.map(op_form) {
            Some(OpForm::Push(n)) => n as usize,
            _ => 0,
        }
    }
}

impl fmt::Display for Instruction<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04x}  {}", self.pc, self.mnemonic())?;

        // An unrecognised byte has no mnemonic worth printing on its own —
        // the byte itself is the only information there is.
        if self.spec.is_none() {
            return write!(f, " {:#04x}", self.opcode);
        }

        if !self.immediate.is_empty() {
            write!(f, " 0x")?;
            for byte in self.immediate {
                write!(f, "{byte:02x}")?;
            }
        }
        if self.truncated {
            write!(f, " (truncated, {} byte(s) missing)", self.missing())?;
        }
        Ok(())
    }
}

/// Decode `code` from offset zero.
///
/// Never fails and never panics: a byte outside the table decodes to an
/// instruction with no [`spec`](Instruction::spec), and a `PUSH` running past
/// the end decodes [`truncated`](Instruction::truncated). Bytecode from a
/// chain is arbitrary data, and a disassembler that gives up on the first odd
/// byte is no use on it.
pub fn disassemble(code: &[u8]) -> Instructions<'_> {
    Instructions { code, pc: 0 }
}

/// Iterator over decoded instructions. See [`disassemble`].
pub struct Instructions<'a> {
    code: &'a [u8],
    pc: usize,
}

impl<'a> Iterator for Instructions<'a> {
    type Item = Instruction<'a>;

    fn next(&mut self) -> Option<Instruction<'a>> {
        let opcode = *self.code.get(self.pc)?;
        let pc = self.pc;
        let spec = by_byte(opcode);

        let wanted = match spec.map(op_form) {
            Some(OpForm::Push(n)) => n as usize,
            _ => 0,
        };
        let from = pc + 1;
        let taken = wanted.min(self.code.len() - from);

        self.pc = from + taken;
        Some(Instruction {
            pc,
            opcode,
            spec,
            immediate: &self.code[from..from + taken],
            truncated: taken < wanted,
        })
    }
}

/// Offsets in `code` that are legal `JUMP`/`JUMPI` targets.
///
/// A `0x5b` byte inside a `PUSH` immediate is data, and jumping to it aborts
/// the frame with nothing more diagnostic than "invalid jump destination".
/// Telling the two apart needs a decode from the start, which is the whole
/// reason this cannot be a byte scan.
pub fn jumpdests(code: &[u8]) -> impl Iterator<Item = usize> + '_ {
    disassemble(code)
        .filter(|ins| ins.mnemonic() == "JUMPDEST")
        .map(|ins| ins.pc)
}

/// `code` rendered one instruction per line. See [`listing`].
pub struct Listing<'a> {
    code: &'a [u8],
}

impl fmt::Display for Listing<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for instruction in disassemble(self.code) {
            writeln!(f, "{instruction}")?;
        }
        Ok(())
    }
}

/// Render `code` as a listing, one instruction per line.
pub fn listing(code: &[u8]) -> Listing<'_> {
    Listing { code }
}

#[cfg(test)]
mod tests;
