// value.rs — per-type storage read/write and key encoding. See
// `design/std-evm-storage-spec.md` §5.

use crate::Address;
use evm_sys::U256;
use evm_sys::storage::{sload, sstore};

/// A type that can occupy one or more storage slots.
///
/// Hand-written impls for the primitives only — a `derive(Storage)` for
/// multi-field structs is deferred (layout is where subtle bugs live; earn
/// the complexity once the basic rules are proven).
pub trait StorageValue {
    fn read_from(slot: U256) -> Self;
    fn write_to(&self, slot: U256);
}

/// A type usable as a [`Mapping`](crate::Mapping) key — reduced to the
/// 32-byte word `keccak256(key ++ slot)` hashes over.
pub trait StorageKey {
    fn to_bytes(&self) -> [u8; 32];
}

impl StorageValue for U256 {
    fn read_from(slot: U256) -> Self {
        unsafe { sload(slot) }
    }

    fn write_to(&self, slot: U256) {
        unsafe { sstore(slot, *self) }
    }
}

impl StorageKey for U256 {
    fn to_bytes(&self) -> [u8; 32] {
        self.to_be_bytes()
    }
}

impl StorageValue for Address {
    fn read_from(slot: U256) -> Self {
        let word = U256::read_from(slot).to_be_bytes();
        let mut addr = [0u8; 20];
        addr.copy_from_slice(&word[12..]);
        Address::from_be_bytes(addr)
    }

    fn write_to(&self, slot: U256) {
        let mut word = [0u8; 32];
        word[12..].copy_from_slice(&self.to_be_bytes());
        U256::from_be_bytes(word).write_to(slot);
    }
}

impl StorageKey for Address {
    fn to_bytes(&self) -> [u8; 32] {
        let mut word = [0u8; 32];
        word[12..].copy_from_slice(&self.to_be_bytes());
        word
    }
}

impl StorageValue for bool {
    fn read_from(slot: U256) -> Self {
        U256::read_from(slot) != U256::ZERO
    }

    fn write_to(&self, slot: U256) {
        let v = if *self { U256::from_u64(1) } else { U256::ZERO };
        v.write_to(slot);
    }
}
