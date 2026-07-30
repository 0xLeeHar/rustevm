//! The storage layer. See `design/std-evm-storage-spec.md` for the full
//! design — this module wires together its pieces:
//!
//! ```text
//! contract code        s.balances().get(addr)         <- typed, slots invisible
//!    |
//! Storage<T> handle    capability token; &/&mut = read/write
//!    |
//! Slot<T> / Mapping    keccak slot derivation, dispatch to StorageValue
//!    |
//! StorageValue trait   per-type read/write (1 slot, multi-slot, packed)
//!    |
//! evm-sys              raw sload / sstore
//! ```
//!
//! `TransientStorage<T>` is the same stack one layer down, on `TLOAD`/
//! `TSTORE` and its own slot address space: `TransientSlot<T>` /
//! `TransientMapping*` over `TransientValue`, fed by `#[transient]` instead
//! of `#[storage]`. The two are separate types all the way up so a helper
//! written for one can never be handed the other.

mod handle;
mod mapping;
mod slot;
mod transient;
mod value;

pub use handle::{
    Storage, StorageLayout, TransientLayout, TransientStorage, WriteCap, with_storage, with_storage_mut,
    with_transient_storage, with_transient_storage_mut,
};
pub use mapping::{Mapping, MappingMut, MappingRef, TransientMappingMut, TransientMappingRef, derived_slot};
pub use slot::Slot;
pub use transient::{ReentrancyLock, TransientGuard, TransientSlot, TransientValue};
pub use value::{StorageKey, StorageValue};
