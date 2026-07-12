//! EVM bytecode emitter.
//!
//! [`Emitter`] accumulates a sequence of [`Opcode`]s and their operands into a
//! raw bytecode buffer.  It also tracks pending jump targets so that forward
//! references can be resolved in a second pass.
//!
//! # Stack conventions
//!
//! The EVM is a last-in-first-out stack machine.  Unless otherwise noted,
//! operands are pushed in the order the EVM expects to pop them — i.e. the
//! *top* of the stack is the last thing pushed.  For binary operations this
//! means pushing `rhs` before `lhs`.
//!
//! # Jump encoding
//!
//! EVM jump destinations are absolute byte offsets.  Because the size of the
//! emitted code is not known until after emission, forward jumps are recorded
//! as [`Patch`] entries and fixed up at the end of [`Emitter::finish`].

use std::collections::HashMap;

use rustc_codegen_ssa::CodegenResults;
use rustc_errors::ErrorGuaranteed;
use rustc_middle::mir::BasicBlock;
use rustc_session::Session;
use rustc_session::output::OutputFilenames;

// ── Opcode ────────────────────────────────────────────────────────────────────

/// EVM opcode byte values (Prague / EIP-7702 instruction set).
///
/// Only opcodes referenced from the codegen lowering are listed here; add more
/// as needed.  The byte value is the discriminant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
#[allow(non_camel_case_types, dead_code)]
pub enum Opcode {
    // ── stop / arithmetic ───────────────────────────────────────────────────
    STOP       = 0x00,
    ADD        = 0x01,
    MUL        = 0x02,
    SUB        = 0x03,
    DIV        = 0x04,
    SDIV       = 0x05,
    MOD        = 0x06,
    SMOD       = 0x07,
    ADDMOD     = 0x08,
    MULMOD     = 0x09,
    EXP        = 0x0a,
    SIGNEXTEND = 0x0b,

    // ── comparison ──────────────────────────────────────────────────────────
    LT     = 0x10,
    GT     = 0x11,
    SLT    = 0x12,
    SGT    = 0x13,
    EQ     = 0x14,
    ISZERO = 0x15,

    // ── bitwise ─────────────────────────────────────────────────────────────
    AND  = 0x16,
    OR   = 0x17,
    XOR  = 0x18,
    NOT  = 0x19,
    BYTE = 0x1a,
    SHL  = 0x1b,
    SHR  = 0x1c,
    SAR  = 0x1d,

    // ── hash ────────────────────────────────────────────────────────────────
    KECCAK256 = 0x20,

    // ── context ─────────────────────────────────────────────────────────────
    ADDRESS      = 0x30,
    BALANCE      = 0x31,
    ORIGIN       = 0x32,
    CALLER       = 0x33,
    CALLVALUE    = 0x34,
    CALLDATALOAD = 0x35,
    CALLDATASIZE = 0x36,
    CALLDATACOPY = 0x37,
    CODESIZE     = 0x38,
    CODECOPY     = 0x39,
    GASPRICE     = 0x3a,
    EXTCODESIZE  = 0x3b,
    EXTCODECOPY  = 0x3c,
    RETURNDATASIZE = 0x3d,
    RETURNDATACOPY = 0x3e,
    EXTCODEHASH  = 0x3f,

    // ── block ───────────────────────────────────────────────────────────────
    BLOCKHASH   = 0x40,
    COINBASE    = 0x41,
    TIMESTAMP   = 0x42,
    NUMBER      = 0x43,
    PREVRANDAO  = 0x44,
    GASLIMIT    = 0x45,
    CHAINID     = 0x46,
    SELFBALANCE = 0x47,
    BASEFEE     = 0x48,
    BLOBHASH    = 0x49,
    BLOBBASEFEE = 0x4a,

    // ── memory / storage ────────────────────────────────────────────────────
    POP      = 0x50,
    MLOAD    = 0x51,
    MSTORE   = 0x52,
    MSTORE8  = 0x53,
    SLOAD    = 0x54,
    SSTORE   = 0x55,
    JUMP     = 0x56,
    JUMPI    = 0x57,
    PC       = 0x58,
    MSIZE    = 0x59,
    GAS      = 0x5a,
    JUMPDEST = 0x5b,
    TLOAD    = 0x5c,
    TSTORE   = 0x5d,
    MCOPY    = 0x5e,

    // ── push ────────────────────────────────────────────────────────────────
    // PUSH0 through PUSH32; variants with immediate bytes follow.
    PUSH0  = 0x5f,
    PUSH1  = 0x60,
    PUSH2  = 0x61,
    PUSH3  = 0x62,
    PUSH4  = 0x63,
    PUSH5  = 0x64,
    PUSH6  = 0x65,
    PUSH7  = 0x66,
    PUSH8  = 0x67,
    PUSH9  = 0x68,
    PUSH10 = 0x69,
    PUSH11 = 0x6a,
    PUSH12 = 0x6b,
    PUSH13 = 0x6c,
    PUSH14 = 0x6d,
    PUSH15 = 0x6e,
    PUSH16 = 0x6f,
    PUSH17 = 0x70,
    PUSH18 = 0x71,
    PUSH19 = 0x72,
    PUSH20 = 0x73,
    PUSH21 = 0x74,
    PUSH22 = 0x75,
    PUSH23 = 0x76,
    PUSH24 = 0x77,
    PUSH25 = 0x78,
    PUSH26 = 0x79,
    PUSH27 = 0x7a,
    PUSH28 = 0x7b,
    PUSH29 = 0x7c,
    PUSH30 = 0x7d,
    PUSH31 = 0x7e,
    PUSH32 = 0x7f,

    // ── dup / swap ───────────────────────────────────────────────────────────
    DUP1  = 0x80,
    DUP2  = 0x81,
    DUP3  = 0x82,
    DUP4  = 0x83,
    DUP5  = 0x84,
    DUP6  = 0x85,
    DUP7  = 0x86,
    DUP8  = 0x87,
    DUP9  = 0x88,
    DUP10 = 0x89,
    DUP11 = 0x8a,
    DUP12 = 0x8b,
    DUP13 = 0x8c,
    DUP14 = 0x8d,
    DUP15 = 0x8e,
    DUP16 = 0x8f,

    SWAP1  = 0x90,
    SWAP2  = 0x91,
    SWAP3  = 0x92,
    SWAP4  = 0x93,
    SWAP5  = 0x94,
    SWAP6  = 0x95,
    SWAP7  = 0x96,
    SWAP8  = 0x97,
    SWAP9  = 0x98,
    SWAP10 = 0x99,
    SWAP11 = 0x9a,
    SWAP12 = 0x9b,
    SWAP13 = 0x9c,
    SWAP14 = 0x9d,
    SWAP15 = 0x9e,
    SWAP16 = 0x9f,

    // ── log ─────────────────────────────────────────────────────────────────
    LOG0 = 0xa0,
    LOG1 = 0xa1,
    LOG2 = 0xa2,
    LOG3 = 0xa3,
    LOG4 = 0xa4,

    // ── account ─────────────────────────────────────────────────────────────
    CREATE       = 0xf0,
    CALL         = 0xf1,
    CALLCODE     = 0xf2,
    RETURN       = 0xf3,
    DELEGATECALL = 0xf4,
    CREATE2      = 0xf5,
    STATICCALL   = 0xfa,
    REVERT       = 0xfd,
    INVALID      = 0xfe,
    SELFDESTRUCT = 0xff,
}

// ── U256 ──────────────────────────────────────────────────────────────────────

/// A 256-bit unsigned integer represented as big-endian bytes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct U256([u8; 32]);

impl From<u8> for U256 {
    fn from(v: u8) -> Self {
        let mut b = [0u8; 32];
        b[31] = v;
        U256(b)
    }
}

impl From<u64> for U256 {
    fn from(v: u64) -> Self {
        let mut b = [0u8; 32];
        b[24..].copy_from_slice(&v.to_be_bytes());
        U256(b)
    }
}

impl From<u128> for U256 {
    fn from(v: u128) -> Self {
        let mut b = [0u8; 32];
        b[16..].copy_from_slice(&v.to_be_bytes());
        U256(b)
    }
}

impl U256 {
    /// Number of significant bytes (minimum PUSH size).
    fn significant_bytes(self) -> usize {
        let leading = self.0.iter().take_while(|&&b| b == 0).count();
        32usize.saturating_sub(leading).max(1)
    }
}

// ── Patch ─────────────────────────────────────────────────────────────────────

/// A pending forward-jump fixup.
///
/// The `offset` is the byte position in `Emitter::buf` where the 3-byte
/// (PUSH2 + 2 immediate bytes) jump target must be written once the destination
/// basic block's byte address is known.
struct Patch {
    /// Byte offset of the PUSH2 opcode in the buffer.
    offset: usize,
    /// The MIR basic block that is the jump target.
    target: BasicBlock,
}

// ── Emitter ───────────────────────────────────────────────────────────────────

/// Accumulates EVM bytecode for a single function.
pub struct Emitter {
    buf: Vec<u8>,
    /// Map from MIR BasicBlock index to byte offset of its JUMPDEST.
    bb_offsets: HashMap<u32, usize>,
    /// Pending jump-target patches.
    patches: Vec<Patch>,
}

impl Emitter {
    pub fn new() -> Self {
        Emitter {
            buf: Vec::new(),
            bb_offsets: HashMap::new(),
            patches: Vec::new(),
        }
    }

    // ── primitive emission ────────────────────────────────────────────────────

    /// Emit a single opcode byte.
    pub fn op(&mut self, opcode: Opcode) {
        self.buf.push(opcode as u8);
    }

    /// Emit a raw byte (used for PUSH immediates).
    fn byte(&mut self, b: u8) {
        self.buf.push(b);
    }

    // ── PUSH helpers ──────────────────────────────────────────────────────────

    /// Emit `PUSH<n>` + the minimal big-endian encoding of `value`.
    pub fn push_u256(&mut self, value: U256) {
        if value == U256::default() {
            self.op(Opcode::PUSH0);
            return;
        }
        let sig = value.significant_bytes();
        // PUSH1 = 0x60, PUSH2 = 0x61, ..., PUSH32 = 0x7f
        let push_opcode = 0x5f + sig as u8; // PUSH0 = 0x5f, PUSH1 = 0x60
        self.byte(push_opcode);
        let start = 32 - sig;
        for b in &value.0[start..] {
            self.byte(*b);
        }
    }

    /// Emit `PUSH2 <placeholder>` and record a patch for the given basic block.
    ///
    /// The placeholder bytes are filled in by [`Emitter::finish`].
    pub fn jump(&mut self, target: BasicBlock) {
        let offset = self.buf.len();
        // PUSH2 + 2 placeholder bytes
        self.byte(Opcode::PUSH2 as u8);
        self.byte(0x00);
        self.byte(0x00);
        self.op(Opcode::JUMP);
        self.patches.push(Patch { offset, target });
    }

    /// Emit `PUSH2 <placeholder> JUMPI`.
    pub fn jumpi(&mut self, target: BasicBlock) {
        let offset = self.buf.len();
        self.byte(Opcode::PUSH2 as u8);
        self.byte(0x00);
        self.byte(0x00);
        self.op(Opcode::JUMPI);
        self.patches.push(Patch { offset, target });
    }

    /// Mark the current byte offset as the start of a basic block.
    ///
    /// Emits `JUMPDEST` and records the offset so jumps to this block can be
    /// patched.
    pub fn mark_bb(&mut self, bb: BasicBlock) {
        let offset = self.buf.len();
        self.bb_offsets.insert(bb.as_u32(), offset);
        self.op(Opcode::JUMPDEST);
    }

    // ── finalise ─────────────────────────────────────────────────────────────

    /// Resolve all pending jump-target patches and return the finished bytecode.
    pub fn finish(mut self) -> Vec<u8> {
        for patch in &self.patches {
            let dest_offset = self.bb_offsets[&patch.target.as_u32()];
            assert!(
                dest_offset <= 0xffff,
                "bytecode too large for PUSH2 jump encoding (offset {dest_offset:#x})"
            );
            // Overwrite the two placeholder bytes (offset+1 and offset+2).
            self.buf[patch.offset + 1] = (dest_offset >> 8) as u8;
            self.buf[patch.offset + 2] = (dest_offset & 0xff) as u8;
        }
        self.buf
    }
}

// ── link ──────────────────────────────────────────────────────────────────────

/// Write the compiled EVM bytecode to the output path.
///
/// This is called from [`EvmCodegenBackend::link`] after all modules have been
/// compiled and joined.
pub fn link(
    _sess: &Session,
    _codegen_results: CodegenResults,
    _outputs: &OutputFilenames,
) -> Result<(), ErrorGuaranteed> {
    // TODO:
    //  1. Extract the bytecode bytes from each compiled module.
    //  2. Assemble the EVM contract:
    //     a. Deploy code (constructor): copies runtime bytecode to memory, returns it.
    //     b. Runtime code: dispatcher + compiled function bodies.
    //  3. Write the hex-encoded bytecode to `outputs.path(OutputType::Exe)`.
    //
    // For now this is a no-op placeholder that lets the pipeline run end-to-end.
    Ok(())
}
