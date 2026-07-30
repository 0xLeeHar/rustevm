// mapping.rs — hashed-slot derivation, matching Solidity's
// `keccak256(key ++ slot)`. See `design/std-evm-storage-spec.md` §4.

use core::marker::PhantomData;
use evm_sys::U256;
use evm_sys::hash::keccak256;
use evm_sys::memory::mstore;

use super::{StorageKey, StorageValue, TransientValue};

/// A mapping field's declared type — purely notational. `#[storage]` reads
/// `Mapping<K, V>` off a field's syntax at compile time to decide what
/// accessors to generate; it never constructs a `Mapping` value, only the
/// `MappingRef`/`MappingMut` views below.
pub struct Mapping<K, V> {
    _marker: PhantomData<(K, V)>,
}

/// A borrowed, read-only view of a [`Mapping`], tied to a `&Storage<T>`
/// borrow by the `'a` lifetime.
pub struct MappingRef<'a, K, V> {
    slot: U256,
    _marker: PhantomData<(&'a (), K, V)>,
}

impl<'a, K, V> MappingRef<'a, K, V> {
    pub const fn new(slot: U256) -> Self {
        MappingRef {
            slot,
            _marker: PhantomData,
        }
    }
}

impl<'a, K: StorageKey, V: StorageValue> MappingRef<'a, K, V> {
    pub fn get(&self, key: K) -> V {
        V::read_from(derived_slot(self.slot, key))
    }
}

/// A borrowed, mutating view of a [`Mapping`], tied to a `&mut Storage<T>`
/// borrow by the `'a` lifetime — `MappingMut` cannot be obtained from a
/// shared handle.
pub struct MappingMut<'a, K, V> {
    slot: U256,
    _marker: PhantomData<(&'a mut (), K, V)>,
}

impl<'a, K, V> MappingMut<'a, K, V> {
    pub const fn new(slot: U256) -> Self {
        MappingMut {
            slot,
            _marker: PhantomData,
        }
    }
}

impl<'a, K: StorageKey, V: StorageValue> MappingMut<'a, K, V> {
    pub fn get(&self, key: K) -> V {
        V::read_from(derived_slot(self.slot, key))
    }

    pub fn set(&mut self, key: K, v: V) {
        v.write_to(derived_slot(self.slot, key))
    }
}

/// A borrowed, read-only view of a transient mapping — [`MappingRef`]'s twin
/// on `TLOAD`. Slot derivation is identical; only the opcode differs.
pub struct TransientMappingRef<'a, K, V> {
    slot: U256,
    _marker: PhantomData<(&'a (), K, V)>,
}

impl<'a, K, V> TransientMappingRef<'a, K, V> {
    pub const fn new(slot: U256) -> Self {
        TransientMappingRef {
            slot,
            _marker: PhantomData,
        }
    }
}

impl<'a, K: StorageKey, V: TransientValue> TransientMappingRef<'a, K, V> {
    pub fn get(&self, key: K) -> V {
        V::read_from_transient(derived_slot(self.slot, key))
    }
}

/// A borrowed, mutating view of a transient mapping — [`MappingMut`]'s twin
/// on `TSTORE`.
pub struct TransientMappingMut<'a, K, V> {
    slot: U256,
    _marker: PhantomData<(&'a mut (), K, V)>,
}

impl<'a, K, V> TransientMappingMut<'a, K, V> {
    pub const fn new(slot: U256) -> Self {
        TransientMappingMut {
            slot,
            _marker: PhantomData,
        }
    }
}

impl<'a, K: StorageKey, V: TransientValue> TransientMappingMut<'a, K, V> {
    pub fn get(&self, key: K) -> V {
        V::read_from_transient(derived_slot(self.slot, key))
    }

    pub fn set(&mut self, key: K, v: V) {
        v.write_to_transient(derived_slot(self.slot, key))
    }
}

/// `keccak256(key ++ slot)` — the value for `key` in a mapping based at
/// `base` lives here. Writes the key and base slot into memory offsets
/// `0`/`32` (the same two-word scratch region Solidity's own compiler
/// reserves for this exact hash) and hashes those 64 bytes.
///
/// Shared by both layouts — transient mappings derive their slots the same
/// way, in their own address space.
///
/// `pub` (not crate-internal) because the generated accessors for *nested*
/// mappings (`Mapping<K, Mapping<K2, V2>>`) call this directly
/// from code compiled in the consuming crate, to derive the intermediate
/// mapping's base slot without going through a `MappingRef`/`MappingMut`.
pub fn derived_slot(base: U256, key: impl StorageKey) -> U256 {
    unsafe {
        mstore(0, U256::from_be_bytes(key.to_bytes()));
        mstore(32, base);
        keccak256(0, 64)
    }
}
