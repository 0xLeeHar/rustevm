// slot.rs — a single typed storage slot. See
// `design/std-evm-storage-spec.md` §3.

use core::marker::PhantomData;
use evm_sys::U256;

use super::StorageValue;

/// One storage slot, typed. Users never see this directly under the
/// recommended design — the `#[storage]` macro produces named accessors
/// over it; `Slot` is the mechanism the generated code targets.
pub struct Slot<T> {
    slot: U256,
    _marker: PhantomData<T>,
}

impl<T: StorageValue> Slot<T> {
    pub const fn new(slot: u64) -> Self {
        Slot {
            slot: U256::from_u64(slot),
            _marker: PhantomData,
        }
    }

    pub fn read(&self) -> T {
        T::read_from(self.slot)
    }

    pub fn write(&self, v: T) {
        v.write_to(self.slot)
    }
}
