// transient.rs — TSTORE/TLOAD through the same shape as persistent
// storage, but a distinct type, so the type system prevents mixing
// persistent and transient semantics. See
// `design/std-evm-storage-spec.md` §7.

use core::marker::PhantomData;
use evm_sys::U256;
use evm_sys::storage::{tload, tstore};

use super::{StorageValue, WriteCap};
use crate::Address;

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
/// [`StorageValue::write_to`]. A separate trait rather than extra methods on
/// `StorageValue`, so that "can be stored" and "can be stored transiently"
/// stay answerable independently — but it takes `StorageValue` as a
/// supertrait, which keeps every primitive's two encodings side by side in
/// one impl block and makes the shared shape obvious.
///
/// The bound does mean a `#[transient]` field type must also be a
/// `StorageValue`. That costs nothing for the primitives; revisit if a
/// transient-only value type ever appears.
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

impl TransientValue for Address {
    fn read_from_transient(slot: U256) -> Self {
        let word = U256::read_from_transient(slot).to_be_bytes();
        let mut addr = [0u8; 20];
        addr.copy_from_slice(&word[12..]);
        Address::from_be_bytes(addr)
    }

    fn write_to_transient(&self, slot: U256) {
        let mut word = [0u8; 32];
        word[12..].copy_from_slice(&self.to_be_bytes());
        U256::from_be_bytes(word).write_to_transient(slot);
    }
}

/// Sets a transient flag on construction and clears it on `Drop` — the
/// answer to the one hazard transient storage has and persistent storage
/// doesn't.
///
/// Transient slots auto-clear at *transaction* end, which is far too coarse
/// to mean "clean at function entry": a flag set and not cleared lingers
/// through every later call in the same transaction. `Drop` runs on early
/// returns and on `?`, so the clear cannot be forgotten.
///
/// `#[transient]` generates a `{field}_guard()` accessor over this for every
/// `bool` field. The guard borrows nothing, so the handle stays usable
/// inside the guarded scope.
///
/// **Footgun:** bind it to a name — `let _guard = ...`. Binding to bare `_`
/// (`let _ = ...`) drops it immediately, clearing the flag before the body
/// runs.
///
/// **Footgun:** two live guards on one slot is a bug — the first `Drop`
/// clears the flag while the second is still in scope.
pub struct TransientGuard {
    slot: U256,
}

impl TransientGuard {
    /// `TSTORE` the flag set; the returned guard clears it on drop.
    ///
    /// `pub` because generated accessors call it from the consuming crate,
    /// the same reason [`TransientSlot::new`] is public.
    pub fn acquire(slot: U256) -> Self {
        true.write_to_transient(slot);
        TransientGuard { slot }
    }
}

impl Drop for TransientGuard {
    fn drop(&mut self) {
        false.write_to_transient(self.slot);
    }
}

/// `keccak256("std_evm.reentrancy_lock")` — the lock's slot.
///
/// A namespaced hash rather than a slot reserved out of `#[transient]`'s
/// sequential numbering (the ERC-7201/EIP-1967 convention): nothing is taken
/// out of the user's address space, contracts that never lock pay nothing,
/// and a 32-byte digest cannot collide with the small sequential slots
/// `#[transient]` hands out. The unit test below pins the constant to its
/// preimage.
///
/// `0x30d335fe9363e130344aba988f030a187d71dff2f1cbda9b4d15586facd932da`
const LOCK_SLOT: U256 = U256::from_be_bytes([
    0x30, 0xd3, 0x35, 0xfe, 0x93, 0x63, 0xe1, 0x30, 0x34, 0x4a, 0xba, 0x98, 0x8f, 0x03, 0x0a, 0x18, 0x7d, 0x71, 0xdf,
    0xf2, 0xf1, 0xcb, 0xda, 0x9b, 0x4d, 0x15, 0x58, 0x6f, 0xac, 0xd9, 0x32, 0xda,
]);

/// A reentrancy lock in transient storage — ~100 gas, versus ~5000 for the
/// `SSTORE`-based equivalent, and it needs no persistent slot.
///
/// Held as an RAII guard: [`acquire`](Self::acquire) aborts the call if the
/// lock is already held, and releases it when the guard drops — including on
/// early return.
///
/// ```ignore
/// pub fn withdraw(s: &mut Storage<VaultStorage>, amount: U256) {
///     let _guard = ReentrancyLock::acquire(s);
///     // ... state changes, then an external call that might reenter ...
/// }   // _guard drops here → lock released
/// ```
///
/// **Footgun:** bind to a name (`let _guard = ...`), never bare `_`, which
/// drops the guard immediately and releases the lock before the body runs.
///
/// Why RAII rather than trusting the transaction-end auto-clear: the
/// auto-clear is transaction-scoped, so a lock left set by an earlier
/// *legitimate, completed* call would falsely block a later one in the same
/// transaction.
pub struct ReentrancyLock;

impl ReentrancyLock {
    /// Take the lock, aborting the call if it is already held.
    ///
    /// The `cap` argument is never read — it is compile-time proof that the
    /// caller may write state. See [`WriteCap`]: `TSTORE` is illegal under
    /// `STATICCALL`, so a view method must not be able to reach this.
    pub fn acquire(cap: &mut impl WriteCap) -> Self {
        let _ = cap;
        if bool::read_from_transient(LOCK_SLOT) {
            // TODO: a real `REVERT` with `Error(string)` data once opcode
            // codegen makes that reachable; `panic!` is the placeholder the
            // rest of the crate uses (see `guard_not_payable`).
            panic!("reentrant call");
        }
        true.write_to_transient(LOCK_SLOT);
        ReentrancyLock
    }
}

impl Drop for ReentrancyLock {
    fn drop(&mut self) {
        false.write_to_transient(LOCK_SLOT);
    }
}

#[cfg(test)]
mod tests {
    use super::LOCK_SLOT;

    #[test]
    fn lock_slot_matches_its_preimage() {
        assert_eq!(
            LOCK_SLOT.to_be_bytes(),
            std_evm_abi::keccak256(b"std_evm.reentrancy_lock"),
            "LOCK_SLOT must stay the hash of its documented preimage"
        );
    }
}
