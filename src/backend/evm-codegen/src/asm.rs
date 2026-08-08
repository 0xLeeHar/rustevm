//! The assembler: a sequence of opcodes in, EVM bytecode out.
//!
//! Everything it knows about the instruction set comes from `evm-isa` —
//! byte values, stack arity, and the fork each opcode landed in. A mnemonic
//! this crate has never heard of but the table has will assemble correctly;
//! a typo will not assemble at all.
//!
//! ```
//! # use evm_codegen::Asm;
//! # use evm_isa::Fork;
//! let mut a = Asm::new(Fork::Cancun);
//! a.push(2u8).push(3u8).op("ADD");
//! let bytecode = a.finish().unwrap();
//! assert_eq!(bytecode, vec![0x60, 0x02, 0x60, 0x03, 0x01]);
//! ```

use std::collections::HashMap;
use std::fmt;

use evm_isa::{Fork, OpForm, by_byte, by_mnemonic, op_form};

pub mod disasm;

/// EIP-170's deployed-bytecode limit.
///
/// Informational — [`Asm::finish`] does not enforce it, because plenty of
/// legitimate output is not deployed runtime code (initcode has its own,
/// larger EIP-3860 limit, and a test fragment is not a contract at all).
/// Callers producing runtime code should check [`Asm::is_deployable`].
pub const MAX_CONTRACT_SIZE: usize = 24_576;

/// Every label reference is a `PUSH2`, so this is the highest byte offset a
/// jump can name.
const MAX_LABEL_OFFSET: usize = u16::MAX as usize;

/// A jump target. Created by [`Asm::label`], placed by [`Asm::bind`], and
/// referenced by [`Asm::push_label`] — any number of times, before or after
/// it is bound.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Label(u32);

/// A label reference awaiting patch: `at` is the offset of the two immediate
/// bytes, not of the `PUSH2` itself.
struct Fixup {
    label: Label,
    at: usize,
}

/// A 32-byte big-endian immediate. Construct via `From` rather than directly;
/// [`Asm::push`] narrows it to the shortest `PUSH` that fits.
///
/// Also serves as the IR's 256-bit literal — one word type per crate is the
/// right number, and `ir` hands these straight back to [`Asm::push`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Immediate([u8; 32]);

impl Immediate {
    /// Significant bytes — the width the value actually needs. Zero for zero.
    pub fn width(&self) -> usize {
        match self.0.iter().position(|&b| b != 0) {
            Some(first) => 32 - first,
            None => 0,
        }
    }

    /// Significant *bits* — the narrowest type this value fits in unsigned.
    ///
    /// `ir::verify` uses it to reject a constant too wide for the type it was
    /// given, which is the one way a `Const` can violate the representation
    /// invariant.
    pub fn bit_width(&self) -> u16 {
        match self.0.iter().position(|&b| b != 0) {
            Some(first) => (32 - first) as u16 * 8 - self.0[first].leading_zeros() as u16,
            None => 0,
        }
    }

    /// The big-endian bytes.
    pub const fn to_be_bytes(&self) -> [u8; 32] {
        self.0
    }

    pub fn is_zero(&self) -> bool {
        self.width() == 0
    }
}

impl fmt::LowerHex for Immediate {
    /// Prints the significant bytes only — `0x2a`, not thirty-one leading
    /// zeros. Zero prints as `0x0`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let width = self.width();
        if width == 0 {
            return write!(f, "0");
        }
        for byte in &self.0[32 - width..] {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl From<bool> for Immediate {
    fn from(b: bool) -> Self {
        Immediate::from(u8::from(b))
    }
}

impl From<[u8; 32]> for Immediate {
    fn from(w: [u8; 32]) -> Self {
        Immediate(w)
    }
}

macro_rules! immediate_from_int {
    ($($t:ty),*) => {$(
        impl From<$t> for Immediate {
            fn from(v: $t) -> Self {
                let mut w = [0u8; 32];
                let b = v.to_be_bytes();
                w[32 - b.len()..].copy_from_slice(&b);
                Immediate(w)
            }
        }
    )*};
}

immediate_from_int!(u8, u16, u32, u64, u128);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AsmError {
    /// No such mnemonic in `evm-isa`'s table.
    UnknownMnemonic(String),
    /// The opcode exists but postdates the target fork.
    OpcodeTooNew {
        mnemonic: &'static str,
        min_fork: Fork,
        target: Fork,
    },
    /// `PUSH`/`DUP`/`SWAP` reached through [`Asm::op`] rather than their
    /// helpers. Emitting a `PUSH1` byte without also emitting its immediate
    /// silently corrupts everything after it.
    UseHelperInstead {
        mnemonic: &'static str,
        helper: &'static str,
    },
    /// `DUP`/`SWAP` depth outside `1..=16`.
    BadStackDepth { helper: &'static str, n: u8 },
    /// A label was referenced but never [`bound`](Asm::bind).
    UnboundLabel(Label),
    /// A label was bound twice.
    LabelAlreadyBound(Label),
    /// Code grew past what a `PUSH2` label reference can address.
    CodeTooLargeToAddress { len: usize },
}

impl fmt::Display for AsmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AsmError::UnknownMnemonic(m) => write!(f, "unknown opcode `{m}`"),
            AsmError::OpcodeTooNew {
                mnemonic,
                min_fork,
                target,
            } => write!(
                f,
                "`{mnemonic}` requires {min_fork:?} but the target fork is {target:?}"
            ),
            AsmError::UseHelperInstead { mnemonic, helper } => {
                write!(f, "use `{helper}` to emit `{mnemonic}` — it carries an operand")
            }
            AsmError::BadStackDepth { helper, n } => {
                write!(f, "`{helper}({n})` is out of range; depth must be 1..=16")
            }
            AsmError::UnboundLabel(l) => write!(f, "label {l:?} was referenced but never bound"),
            AsmError::LabelAlreadyBound(l) => write!(f, "label {l:?} was bound twice"),
            AsmError::CodeTooLargeToAddress { len } => write!(
                f,
                "code is {len} bytes; a PUSH2 label reference cannot address past {MAX_LABEL_OFFSET}"
            ),
        }
    }
}

impl std::error::Error for AsmError {}

/// Assembles bytecode for one target fork.
///
/// Errors are **sticky**: the first failure is recorded, later emits become
/// no-ops, and [`finish`](Self::finish) returns it. That keeps the builder
/// chainable without a `?` on every call, and nothing can act on bad output
/// because `finish` is the only way to get the bytes out.
pub struct Asm {
    fork: Fork,
    code: Vec<u8>,
    labels: HashMap<Label, usize>,
    fixups: Vec<Fixup>,
    next_label: u32,
    error: Option<AsmError>,
}

impl Asm {
    pub fn new(fork: Fork) -> Self {
        Asm {
            fork,
            code: Vec::new(),
            labels: HashMap::new(),
            fixups: Vec::new(),
            next_label: 0,
            error: None,
        }
    }

    /// Current byte offset — the address the next emitted opcode will sit at.
    pub fn offset(&self) -> usize {
        self.code.len()
    }

    /// Whether the output is within EIP-170's deployed-code limit. Only
    /// meaningful for runtime bytecode; see [`MAX_CONTRACT_SIZE`].
    pub fn is_deployable(&self) -> bool {
        self.code.len() <= MAX_CONTRACT_SIZE
    }

    /// Emit an operand-free opcode by mnemonic.
    ///
    /// `PUSH`/`DUP`/`SWAP` are rejected here: they carry an operand encoded
    /// into the opcode byte (or following it), so they go through
    /// [`push`](Self::push), [`dup`](Self::dup) and [`swap`](Self::swap),
    /// which compute the byte from the operand rather than trusting the
    /// caller to keep the two in sync.
    pub fn op(&mut self, mnemonic: &str) -> &mut Self {
        if self.error.is_some() {
            return self;
        }

        let Some(spec) = by_mnemonic(mnemonic) else {
            return self.fail(AsmError::UnknownMnemonic(mnemonic.to_string()));
        };

        let helper = match op_form(spec) {
            OpForm::Simple => None,
            OpForm::Push(_) => Some("push"),
            OpForm::Dup(_) => Some("dup"),
            OpForm::Swap(_) => Some("swap"),
        };
        if let Some(helper) = helper {
            return self.fail(AsmError::UseHelperInstead {
                mnemonic: spec.mnemonic,
                helper,
            });
        }

        self.emit_byte(spec.byte, spec.mnemonic, spec.min_fork)
    }

    /// Push a constant, using the narrowest `PUSH` that holds it.
    ///
    /// Zero is `PUSH0` from Shanghai on, and `PUSH1 0x00` before it — one
    /// byte more, but `PUSH0` simply does not exist on those forks.
    pub fn push(&mut self, value: impl Into<Immediate>) -> &mut Self {
        if self.error.is_some() {
            return self;
        }

        let imm = value.into();
        let width = imm.width();

        if width == 0 && self.fork < Fork::Shanghai {
            return self.push_bytes(&[0u8]);
        }
        if width == 0 {
            // PUSH0 — its own opcode, no immediate follows.
            return self.emit_byte(0x5f, "PUSH0", Fork::Shanghai);
        }
        self.push_bytes(&imm.0[32 - width..])
    }

    /// `DUP{n}` — copy the `n`-th stack item to the top. `n` is 1..=16.
    pub fn dup(&mut self, n: u8) -> &mut Self {
        if self.error.is_some() {
            return self;
        }
        if !(1..=16).contains(&n) {
            return self.fail(AsmError::BadStackDepth { helper: "dup", n });
        }
        self.emit_family_byte(0x7f + n)
    }

    /// `SWAP{n}` — exchange the top with the `(n+1)`-th stack item. `n` is
    /// 1..=16.
    pub fn swap(&mut self, n: u8) -> &mut Self {
        if self.error.is_some() {
            return self;
        }
        if !(1..=16).contains(&n) {
            return self.fail(AsmError::BadStackDepth { helper: "swap", n });
        }
        self.emit_family_byte(0x8f + n)
    }

    /// Reserve a jump target. Emits nothing; call [`bind`](Self::bind) at the
    /// point it should land, which may be before or after the references.
    pub fn label(&mut self) -> Label {
        let l = Label(self.next_label);
        self.next_label += 1;
        l
    }

    /// Place `label` here, emitting the `JUMPDEST` it names.
    ///
    /// Binding is the only way to give a label an address, and it always
    /// emits the `JUMPDEST` — so a bound label cannot fail to be a legal jump
    /// target. (Jumping anywhere else aborts the frame at runtime with no
    /// diagnostic beyond "invalid jump destination".)
    pub fn bind(&mut self, label: Label) -> &mut Self {
        if self.error.is_some() {
            return self;
        }
        if self.labels.contains_key(&label) {
            return self.fail(AsmError::LabelAlreadyBound(label));
        }
        self.labels.insert(label, self.code.len());
        self.emit_byte(0x5b, "JUMPDEST", Fork::Frontier)
    }

    /// Push a label's address, for a following `JUMP`/`JUMPI`.
    ///
    /// Always a `PUSH2`, even for an address that would fit in one byte.
    /// Narrowing would need a fixpoint: shrinking one reference moves every
    /// later label, which can shrink another, and so on. `PUSH2` addresses
    /// 64KB — comfortably past EIP-170's 24KB — so it is always wide enough.
    /// The cost is one byte per jump below offset 256; recovering it belongs
    /// in a peephole pass over the finished code, not here.
    pub fn push_label(&mut self, label: Label) -> &mut Self {
        if self.error.is_some() {
            return self;
        }
        self.code.push(0x61); // PUSH2
        self.fixups.push(Fixup {
            label,
            at: self.code.len(),
        });
        self.code.extend_from_slice(&[0, 0]);
        self
    }

    /// Resolve label references and return the bytecode.
    pub fn finish(mut self) -> Result<Vec<u8>, AsmError> {
        if let Some(e) = self.error {
            return Err(e);
        }

        if self.code.len() > MAX_LABEL_OFFSET && !self.fixups.is_empty() {
            return Err(AsmError::CodeTooLargeToAddress { len: self.code.len() });
        }

        for fixup in &self.fixups {
            let Some(&target) = self.labels.get(&fixup.label) else {
                return Err(AsmError::UnboundLabel(fixup.label));
            };
            let bytes = (target as u16).to_be_bytes();
            self.code[fixup.at..fixup.at + 2].copy_from_slice(&bytes);
        }

        Ok(self.code)
    }

    // --- internals ---

    /// Emit one opcode byte after checking it exists on the target fork.
    fn emit_byte(&mut self, byte: u8, mnemonic: &'static str, min_fork: Fork) -> &mut Self {
        if min_fork > self.fork {
            return self.fail(AsmError::OpcodeTooNew {
                mnemonic,
                min_fork,
                target: self.fork,
            });
        }
        self.code.push(byte);
        self
    }

    /// Emit a `DUP`/`SWAP` byte, taking its mnemonic and fork from the table
    /// so the diagnostics name the real opcode.
    fn emit_family_byte(&mut self, byte: u8) -> &mut Self {
        let spec = by_byte(byte).expect("DUP/SWAP bytes are all in the table");
        self.emit_byte(spec.byte, spec.mnemonic, spec.min_fork)
    }

    /// Emit `PUSH{n}` plus `bytes` as its immediate.
    fn push_bytes(&mut self, bytes: &[u8]) -> &mut Self {
        let byte = 0x5f + bytes.len() as u8;
        let spec = by_byte(byte).expect("PUSH1..=PUSH32 are all in the table");
        self.emit_byte(spec.byte, spec.mnemonic, spec.min_fork);
        if self.error.is_none() {
            self.code.extend_from_slice(bytes);
        }
        self
    }

    fn fail(&mut self, e: AsmError) -> &mut Self {
        self.error.get_or_insert(e);
        self
    }
}

#[cfg(test)]
mod tests;
