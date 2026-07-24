// address.rs — the 20-byte Ethereum account address type.
//
// Lives here rather than in `std-evm-abi`: `Address` is a concrete runtime
// domain type, not ABI machinery. Valid under the orphan rules either way
// (the type is local to whichever crate defines it), but keeps
// `std-evm-abi` scoped to pure encode/decode rules.

use std_evm_abi::{AbiDecode, AbiEncode, Component, DecodeError, read_word};

/// A 20-byte Ethereum address, ABI-encoded as a 32-byte word left-padded
/// with zeros (the low 20 bytes hold the address) — the same convention
/// `solidity_type_name` already maps `"Address"` to (`"address"`).
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Address([u8; 20]);

impl Address {
    pub const ZERO: Address = Address([0u8; 20]);

    pub const fn from_be_bytes(b: [u8; 20]) -> Address {
        Address(b)
    }

    pub const fn to_be_bytes(self) -> [u8; 20] {
        self.0
    }
}

impl AbiEncode for Address {
    fn to_component(&self) -> Component {
        let mut word = [0u8; 32];
        word[12..].copy_from_slice(&self.0);
        Component::Static(word.to_vec())
    }
}

impl AbiDecode for Address {
    fn decode(input: &[u8], offset: usize) -> Result<Self, DecodeError> {
        let word = read_word(input, offset)?;
        let mut addr = [0u8; 20];
        addr.copy_from_slice(&word[12..]);
        Ok(Address(addr))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std_evm_abi::encode_tuple;

    #[test]
    fn round_trips_through_abi_encoding() {
        let addr = Address::from_be_bytes([0x11; 20]);
        let enc = encode_tuple(&[addr.to_component()]);
        assert_eq!(enc.len(), 32);
        assert_eq!(Address::decode(&enc, 0).unwrap(), addr);
    }

    #[test]
    fn encodes_left_padded_with_zeros() {
        let addr = Address::from_be_bytes([0xAB; 20]);
        let enc = encode_tuple(&[addr.to_component()]);
        assert!(enc[..12].iter().all(|&b| b == 0));
        assert_eq!(&enc[12..32], &[0xABu8; 20]);
    }
}
