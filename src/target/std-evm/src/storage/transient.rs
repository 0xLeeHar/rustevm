// transient.rs — TSTORE/TLOAD through the same shape as persistent
// storage, but a distinct type, so the type system prevents mixing
// persistent and transient semantics. See
// `design/std-evm-storage-spec.md` §7.
//
// `ReentrancyLock` (the main reason to want this) is deliberately not
// implemented yet — it needs a storage slot reserved out of band from
// whatever `#[storage]` assigns sequentially, and that reservation
// strategy hasn't been decided. Land `TransientSlot` now; add the lock
// once that's settled.

use core::marker::PhantomData;
use evm_sys::U256;
use evm_sys::storage::{tload, tstore};

use super::StorageValue;

/// A separate capability marker for transient-storage access, so a helper
/// written against `Storage<T>` can't be handed transient state and vice
/// versa.
pub struct Transient;

/// One transient storage slot, typed — mirrors [`Slot`](crate::Slot) but
/// backed by `TLOAD`/`TSTORE` instead of `SLOAD`/`SSTORE`.
///
/// **Gotcha:** transient storage clears at *transaction* end, not between
/// calls within a transaction. Do not assume a fresh slot at function
/// entry.
pub struct TransientSlot<T> {
    slot: U256,
    _marker: PhantomData<T>,
}

impl<T: TransientValue> TransientSlot<T> {
    pub const fn new(slot: u64) -> Self {
        TransientSlot {
            slot: U256::from_u64(slot),
            _marker: PhantomData,
        }
    }

    pub fn read(&self) -> T {
        T::read_from_transient(self.slot)
    }

    pub fn write(&self, v: T) {
        v.write_to_transient(self.slot)
    }
}

/// Transient-storage counterpart of [`StorageValue::read_from`]/
/// [`StorageValue::write_to`]. A separate trait method pair rather than an
/// overload, since a type's persistent and transient encodings are the same
/// shape but come from different opcodes — kept on `StorageValue` itself
/// (rather than a second trait) so primitive impls stay in one place.
pub trait TransientValue: StorageValue {
    fn read_from_transient(slot: U256) -> Self;
    fn write_to_transient(&self, slot: U256);
}

impl TransientValue for U256 {
    fn read_from_transient(slot: U256) -> Self {
        unsafe { tload(slot) }
    }

    fn write_to_transient(&self, slot: U256) {
        unsafe { tstore(slot, *self) }
    }
}

impl TransientValue for bool {
    fn read_from_transient(slot: U256) -> Self {
        U256::read_from_transient(slot) != U256::ZERO
    }

    fn write_to_transient(&self, slot: U256) {
        let v = if *self { U256::from_u64(1) } else { U256::ZERO };
        v.write_to_transient(slot);
    }
}
