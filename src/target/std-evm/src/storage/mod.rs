//! The storage layer. See `design/std-evm-storage-spec.md` for the full
//! design — this module wires together its pieces:
//!
//! ```text
//! contract code       s.balances().get(addr)          <- typed, slots invisible
//!    |
//! Storage<T> handle    capability token; &/&mut = read/write
//!    |
//! Slot<T> / Mapping    keccak slot derivation, dispatch to StorageValue
//!    |
//! StorageValue trait   per-type read/write (1 slot, multi-slot, packed)
//!    |
//! evm-sys              raw sload / sstore / tload / tstore
//! ```

mod handle;
mod mapping;
mod slot;
mod transient;
mod value;

pub use handle::{Storage, StorageLayout, with_storage, with_storage_mut};
pub use mapping::{Mapping, MappingMut, MappingRef, derived_slot};
pub use slot::Slot;
pub use transient::{Transient, TransientSlot, TransientValue};
pub use value::{StorageKey, StorageValue};
