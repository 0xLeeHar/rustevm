// handle.rs — the unforgeable capability token. See
// `design/std-evm-storage-spec.md` §1.

use core::marker::PhantomData;

/// Marks a type as a storage layout — implemented once per contract by the
/// `#[storage]` macro on a zero-field marker struct. Never implemented by
/// hand: the whole point is that slot assignment only ever happens inside
/// the macro (see `design/std-evm-storage-spec.md` §2).
pub trait StorageLayout {}

/// A storage handle, type-tagged by layout. `&Storage<T>` on a method
/// signature is *provably* view — enforced by the borrow checker, not an
/// attribute a developer can get wrong; `&mut Storage<T>` is mutating.
///
/// Zero-sized: this carries no data of its own, only a capability to reach
/// `T`'s slots via the accessors `#[storage]` generates.
pub struct Storage<T: StorageLayout>(PhantomData<T>);

impl<T: StorageLayout> Storage<T> {
    /// Deliberately not `pub` — see `with_storage`/`with_storage_mut` below.
    /// If this were public, a user could mint a handle inside a view
    /// function and the `&`/`&mut` discipline would be decorative.
    pub(crate) fn new() -> Self {
        Storage(PhantomData)
    }
}

/// Run `f` with a borrowed read-only handle.
///
/// `#[contract]`'s generated code calls this instead of constructing
/// `Storage::new()` itself, because that code is compiled as part of the
/// *consuming* crate, not `std-evm` — a `pub(crate)` constructor there would
/// be invisible to it. Handing out only a borrow, scoped to `f`, is
/// actually a stronger guarantee than a hidden-but-public constructor would
/// be: nothing outside this crate can hold a `Storage<T>` value at all, only
/// borrow one for the duration of one call.
pub fn with_storage<T: StorageLayout, R>(f: impl FnOnce(&Storage<T>) -> R) -> R {
    f(&Storage::new())
}

/// Run `f` with a borrowed mutating handle. See [`with_storage`].
pub fn with_storage_mut<T: StorageLayout, R>(f: impl FnOnce(&mut Storage<T>) -> R) -> R {
    f(&mut Storage::new())
}
