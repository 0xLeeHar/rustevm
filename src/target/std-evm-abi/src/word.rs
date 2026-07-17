// word.rs — the 256-bit EVM word type.
//
// `U256` lives here (not in `evm-sys`) because it is fundamentally the ABI word
// type: the codec impls `AbiEncode`/`AbiDecode` for it, and keeping the type
// local avoids an orphan-rule violation. `evm-sys` re-exports it
// (`pub use std_evm_abi::U256;`) so the opcode layer still refers to `crate::U256`.
// It is pure data — a big-endian `[u8; 32]` newtype with `const` helpers, no opcodes.

/// One 32-byte ABI word — the fixed-width slot every ABI value serializes into.
///
/// A plain alias for `[u8; 32]` (not a newtype), so it splices straight into byte
/// buffers and sub-slices without conversions. Distinct in intent from [`U256`],
/// which is the opaque 256-bit EVM integer the backend recognizes as a stack slot.
pub type Word = [u8; 32];

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct U256(
    // Opaque to Rust; the backend treats a value of this type as one
    // 256-bit EVM stack slot. The inner array is only how it's *stored*
    // in memory when spilled — big-endian, matching the target spec.
    [u8; 32],
);

impl U256 {
    pub const ZERO: U256 = U256([0u8; 32]);
    pub const MAX: U256 = U256([0xffu8; 32]);

    pub const fn from_u64(x: u64) -> U256 {
        let mut b = [0u8; 32];
        let xb = x.to_be_bytes();
        b[24] = xb[0];
        b[25] = xb[1];
        b[26] = xb[2];
        b[27] = xb[3];
        b[28] = xb[4];
        b[29] = xb[5];
        b[30] = xb[6];
        b[31] = xb[7];
        U256(b)
    }

    pub const fn to_be_bytes(self) -> [u8; 32] {
        self.0
    }
    pub const fn from_be_bytes(b: [u8; 32]) -> U256 {
        U256(b)
    }
}
