// word.rs
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct U256(
    // Opaque to Rust; the backend treats a value of this type as one
    // 256-bit EVM stack slot. The inner array is only how it's *stored*
    // in memory when spilled — big-endian, matching the target spec.
    [u8; 32],
);

impl U256 {
    pub const ZERO: U256 = U256([0u8; 32]);

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
