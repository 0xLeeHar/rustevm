// codec.rs — Ethereum ABI head/tail encoding & decoding.
//
// The ABI lays a tuple of arguments out in two regions:
//   * head — one entry per argument, in order. A *static* argument's value sits
//     inline here; a *dynamic* argument contributes a 32-byte **offset** pointing
//     into the tail.
//   * tail — the contents of each dynamic argument, appended in order.
//
// Offsets are relative to the start of the tuple (not absolute calldata; the
// 4-byte selector is not part of the tuple). All words are 32 bytes, big-endian,
// matching the target spec.

use alloc::string::String;
use alloc::vec::Vec;

use crate::word::{U256, Word};

/// A single tuple element, classified by how it participates in head/tail layout.
pub enum Component {
    /// Static value — its bytes (a whole number of 32-byte words) go inline in
    /// the head.
    Static(Vec<u8>),
    /// Dynamic value — a 32-byte offset goes in the head; these bytes go in the tail.
    Dynamic(Vec<u8>),
}

/// Error returned while decoding ABI-encoded input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeError {
    /// Input ended before the expected bytes were available.
    Truncated,
    /// An offset or length word did not fit in a `usize` (or overflowed).
    Overflow,
    /// A `string` payload was not valid UTF-8.
    InvalidUtf8,
}

/// Encode a tuple of components into a single ABI buffer (head followed by tail).
///
/// This is the entry point the `#[derive(AbiEncode)]` output and the runtime
/// dispatcher target: build one `Component` per field via
/// [`AbiEncode::to_component`], then call this.
pub fn encode_tuple(components: &[Component]) -> Vec<u8> {
    // Head size = static components contribute their full length; dynamic ones
    // contribute a single 32-byte offset slot.
    let head_size: usize = components
        .iter()
        .map(|c| match c {
            Component::Static(b) => b.len(),
            Component::Dynamic(_) => 32,
        })
        .sum();

    let mut head = Vec::with_capacity(head_size);
    let mut tail = Vec::new();

    for c in components {
        match c {
            Component::Static(b) => head.extend_from_slice(b),
            Component::Dynamic(b) => {
                let offset = head_size + tail.len();
                head.extend_from_slice(&word_from_usize(offset));
                tail.extend_from_slice(b);
            }
        }
    }

    head.extend_from_slice(&tail);
    head
}

// ── traits ──────────────────────────────────────────────────────────────────

/// Encode a Rust value into its ABI representation.
pub trait AbiEncode {
    /// Produce this value's tuple component (static value or dynamic tail bytes).
    fn to_component(&self) -> Component;
}

/// Decode a Rust value from ABI-encoded input.
///
/// `offset` is the byte position, within `input`, of this value's **head slot**.
/// For dynamic types that slot holds a relative offset which is interpreted
/// against the start of `input` (i.e. `input` must be the tuple blob with the
/// 4-byte selector already stripped).
pub trait AbiDecode: Sized {
    fn decode(input: &[u8], offset: usize) -> Result<Self, DecodeError>;
}

// ── word helpers ────────────────────────────────────────────────────────────

/// Big-endian ABI word from an unsigned value (left-padded with zeros).
pub fn word_from_u128(v: u128) -> Word {
    let mut w = [0u8; 32];
    w[16..].copy_from_slice(&v.to_be_bytes());
    w
}

/// Big-endian ABI word from a signed value (sign-extended into the high bytes).
pub fn word_from_i128(v: i128) -> Word {
    let mut w = if v < 0 { [0xffu8; 32] } else { [0u8; 32] };
    w[16..].copy_from_slice(&v.to_be_bytes());
    w
}

fn word_from_usize(v: usize) -> Word {
    word_from_u128(v as u128)
}

/// Read the ABI word at `offset`, or `Truncated` if it runs past the end.
pub fn read_word(input: &[u8], offset: usize) -> Result<Word, DecodeError> {
    let end = offset.checked_add(32).ok_or(DecodeError::Overflow)?;
    if end > input.len() {
        return Err(DecodeError::Truncated);
    }
    let mut w = [0u8; 32];
    w.copy_from_slice(&input[offset..end]);
    Ok(w)
}

/// Read a 32-byte word at `offset` and interpret it as a `usize` (used for
/// ABI offsets and lengths). Errors if the value exceeds `usize`.
fn read_usize(input: &[u8], offset: usize) -> Result<usize, DecodeError> {
    let w = read_word(input, offset)?;
    // Only the low `size_of::<usize>()` bytes may be set.
    let n = core::mem::size_of::<usize>();
    if w[..32 - n].iter().any(|&b| b != 0) {
        return Err(DecodeError::Overflow);
    }
    let mut b = [0u8; core::mem::size_of::<usize>()];
    b.copy_from_slice(&w[32 - n..]);
    Ok(usize::from_be_bytes(b))
}

/// Right-pad `data` with zeros to the next 32-byte boundary and prefix its
/// length — the tail form of a dynamic `bytes`/`string`.
fn encode_bytes(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(32 + data.len().div_ceil(32) * 32);
    out.extend_from_slice(&word_from_usize(data.len()));
    out.extend_from_slice(data);
    let rem = data.len() % 32;
    if rem != 0 {
        out.extend(core::iter::repeat(0u8).take(32 - rem));
    }
    out
}

/// Decode a dynamic `bytes` payload whose head slot is at `offset`.
fn decode_bytes(input: &[u8], offset: usize) -> Result<Vec<u8>, DecodeError> {
    let ptr = read_usize(input, offset)?;
    let len = read_usize(input, ptr)?;
    let start = ptr.checked_add(32).ok_or(DecodeError::Overflow)?;
    let end = start.checked_add(len).ok_or(DecodeError::Overflow)?;
    if end > input.len() {
        return Err(DecodeError::Truncated);
    }
    Ok(input[start..end].to_vec())
}

// ── primitive impls ─────────────────────────────────────────────────────────

macro_rules! impl_uint {
    ($($t:ty),*) => {$(
        impl AbiEncode for $t {
            fn to_component(&self) -> Component {
                Component::Static(word_from_u128(*self as u128).to_vec())
            }
        }
        impl AbiDecode for $t {
            fn decode(input: &[u8], offset: usize) -> Result<Self, DecodeError> {
                const N: usize = core::mem::size_of::<$t>();
                let w = read_word(input, offset)?;
                let mut b = [0u8; N];
                b.copy_from_slice(&w[32 - N..]);
                Ok(<$t>::from_be_bytes(b))
            }
        }
    )*};
}
impl_uint!(u8, u16, u32, u64, u128);

macro_rules! impl_int {
    ($($t:ty),*) => {$(
        impl AbiEncode for $t {
            fn to_component(&self) -> Component {
                Component::Static(word_from_i128(*self as i128).to_vec())
            }
        }
        impl AbiDecode for $t {
            fn decode(input: &[u8], offset: usize) -> Result<Self, DecodeError> {
                const N: usize = core::mem::size_of::<$t>();
                let w = read_word(input, offset)?;
                let mut b = [0u8; N];
                b.copy_from_slice(&w[32 - N..]);
                Ok(<$t>::from_be_bytes(b))
            }
        }
    )*};
}
impl_int!(i8, i16, i32, i64, i128);

impl AbiEncode for bool {
    fn to_component(&self) -> Component {
        Component::Static(word_from_u128(*self as u128).to_vec())
    }
}
impl AbiDecode for bool {
    fn decode(input: &[u8], offset: usize) -> Result<Self, DecodeError> {
        Ok(read_word(input, offset)?.iter().any(|&b| b != 0))
    }
}

impl AbiEncode for [u8; 32] {
    fn to_component(&self) -> Component {
        Component::Static(self.to_vec())
    }
}
impl AbiDecode for [u8; 32] {
    fn decode(input: &[u8], offset: usize) -> Result<Self, DecodeError> {
        read_word(input, offset)
    }
}

impl AbiEncode for U256 {
    fn to_component(&self) -> Component {
        Component::Static(self.to_be_bytes().to_vec())
    }
}
impl AbiDecode for U256 {
    fn decode(input: &[u8], offset: usize) -> Result<Self, DecodeError> {
        Ok(U256::from_be_bytes(read_word(input, offset)?))
    }
}

// Dynamic: bytes & string. (Generic `Vec<T>` arrays are a follow-up — a blanket
// impl would collide with `Vec<u8>` under coherence without specialization.)
impl AbiEncode for [u8] {
    fn to_component(&self) -> Component {
        Component::Dynamic(encode_bytes(self))
    }
}
impl AbiEncode for Vec<u8> {
    fn to_component(&self) -> Component {
        Component::Dynamic(encode_bytes(self))
    }
}
impl AbiDecode for Vec<u8> {
    fn decode(input: &[u8], offset: usize) -> Result<Self, DecodeError> {
        decode_bytes(input, offset)
    }
}

impl AbiEncode for str {
    fn to_component(&self) -> Component {
        Component::Dynamic(encode_bytes(self.as_bytes()))
    }
}
impl AbiEncode for String {
    fn to_component(&self) -> Component {
        Component::Dynamic(encode_bytes(self.as_bytes()))
    }
}
impl AbiDecode for String {
    fn decode(input: &[u8], offset: usize) -> Result<Self, DecodeError> {
        String::from_utf8(decode_bytes(input, offset)?).map_err(|_| DecodeError::InvalidUtf8)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // f(uint256 a, bytes b, bool c) with a = 0x11, b = 0xAABB, c = true.
    // The head/tail layout worked out in the design notes.
    fn sample() -> Vec<u8> {
        encode_tuple(&[
            U256::from_u64(0x11).to_component(),
            (&[0xAAu8, 0xBB][..]).to_component(),
            true.to_component(),
        ])
    }

    #[test]
    fn head_tail_layout() {
        let enc = sample();
        assert_eq!(enc.len(), 0xa0); // 3 head words + 2 tail words
        // head slot 0: a inline = 0x11
        assert_eq!(read_word(&enc, 0x00).unwrap()[31], 0x11);
        // head slot 1: b's offset = 0x60
        assert_eq!(read_word(&enc, 0x20).unwrap()[31], 0x60);
        // head slot 2: c inline = 1
        assert_eq!(read_word(&enc, 0x40).unwrap()[31], 0x01);
        // tail: b length = 2, then data right-padded
        assert_eq!(read_word(&enc, 0x60).unwrap()[31], 0x02);
        assert_eq!(&enc[0x80..0x82], &[0xAA, 0xBB]);
        assert!(enc[0x82..0xa0].iter().all(|&b| b == 0));
    }

    #[test]
    fn round_trip() {
        let enc = sample();
        assert_eq!(U256::decode(&enc, 0x00).unwrap(), U256::from_u64(0x11));
        assert_eq!(Vec::<u8>::decode(&enc, 0x20).unwrap(), alloc::vec![0xAA, 0xBB]);
        assert_eq!(bool::decode(&enc, 0x40).unwrap(), true);
    }

    #[test]
    fn signed_sign_extends() {
        let c = (-1i64).to_component();
        match c {
            Component::Static(w) => assert!(w.iter().all(|&b| b == 0xff)),
            _ => panic!("i64 should be static"),
        }
        // round-trip a negative value
        let enc = encode_tuple(&[(-5i32).to_component()]);
        assert_eq!(i32::decode(&enc, 0).unwrap(), -5);
    }

    #[test]
    fn truncated_is_error() {
        assert_eq!(u64::decode(&[0u8; 8], 0), Err(DecodeError::Truncated));
    }

    #[test]
    fn string_round_trip() {
        let enc = encode_tuple(&["hello".to_component()]);
        assert_eq!(String::decode(&enc, 0).unwrap(), "hello");
    }
}
