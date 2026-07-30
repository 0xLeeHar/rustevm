// handle.rs — the unforgeable capability token. See
// `design/std-evm-storage-spec.md` §1.

use core::marker::PhantomData;

/// Marks a type as a storage layout — implemented once per contract by the
/// `#[storage]` macro on a zero-field marker struct. Never implemented by
/// hand: the whole point is that slot assignment only ever happens inside
/// the macro (see `design/std-evm-storage-spec.md` §2).
pub trait StorageLayout {}

/// Marks a type as a *transient* storage layout — the `#[transient]`
/// counterpart of [`StorageLayout`], and deliberately a separate trait.
///
/// Transient slots live in their own address space: transient slot 0 and
/// persistent slot 0 are unrelated storage. Keeping the marker traits
/// distinct is what stops a `#[storage]` layout being used as a
/// `TransientStorage<T>` parameter, where every accessor name would silently
/// mean a different value than it does on `Storage<T>`.
pub trait TransientLayout {}

/// A storage handle, type-tagged by layout. `&Storage<T>` on a method
/// signature is *provably* view — enforced by the borrow checker, not an
/// attribute a developer can get wrong; `&mut Storage<T>` is mutating.
///
/// Zero-sized: this carries no data of its own, only a capability to reach
/// `T`'s slots via the accessors `#[storage]` generates.
pub struct Storage<T: StorageLayout>(PhantomData<T>);

/// The transient-storage handle — [`Storage`]'s twin in every respect except
/// the opcodes underneath it: `TLOAD`/`TSTORE` instead of `SLOAD`/`SSTORE`.
///
/// Same capability discipline, same `&`/`&mut` = read/write rule. `TLOAD` is
/// permitted under `STATICCALL` and `TSTORE` is not, so `&TransientStorage<T>`
/// is view-safe exactly as `&Storage<T>` is.
///
/// **Gotcha:** transient storage is cleared at *transaction* end, not when the
/// call returns. A value written here outlives the frame that wrote it. See
/// [`TransientGuard`](crate::TransientGuard).
pub struct TransientStorage<T: TransientLayout>(PhantomData<T>);

impl<T: StorageLayout> Storage<T> {
    /// Deliberately not `pub` — see `with_storage`/`with_storage_mut` below.
    /// If this were public, a user could mint a handle inside a view
    /// function and the `&`/`&mut` discipline would be decorative.
    pub(crate) fn new() -> Self {
        Storage(PhantomData)
    }
}

impl<T: TransientLayout> TransientStorage<T> {
    /// Crate-private for the same reason as [`Storage::new`].
    pub(crate) fn new() -> Self {
        TransientStorage(PhantomData)
    }
}

/// Proof that the holder may write state — carried by `&mut` of either
/// handle, and unforgeable outside this crate.
///
/// Exists for the hand-called APIs that emit `TSTORE` without going through a
/// generated accessor, [`ReentrancyLock`](crate::ReentrancyLock) above all.
/// `TSTORE` is forbidden under `STATICCALL`, so an argument-free `acquire()`
/// would let a method taking only `&Storage<T>` — which the ABI marks `view`
/// and callers reach via `STATICCALL` — emit one and revert at runtime.
/// Demanding a `&mut` handle pushes that to a compile error.
///
/// Scope note: this is not a general seal on writes. `Slot::new` and
/// `TransientSlot::new` are `pub` (they have to be — generated accessors are
/// compiled in the consuming crate), so code that reaches past the generated
/// accessors can still write from a view function. Closing that properly means
/// threading a capability through slot construction itself, which is a
/// separate change across both layouts.
pub trait WriteCap: sealed::Sealed {}

impl<T: StorageLayout> WriteCap for Storage<T> {}
impl<T: TransientLayout> WriteCap for TransientStorage<T> {}

mod sealed {
    use super::{Storage, StorageLayout, TransientLayout, TransientStorage};

    pub trait Sealed {}

    impl<T: StorageLayout> Sealed for Storage<T> {}
    impl<T: TransientLayout> Sealed for TransientStorage<T> {}
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

/// Run `f` with a borrowed read-only transient handle. See [`with_storage`] —
/// same reasoning, same guarantee, `TLOAD` instead of `SLOAD`.
pub fn with_transient_storage<T: TransientLayout, R>(f: impl FnOnce(&TransientStorage<T>) -> R) -> R {
    f(&TransientStorage::new())
}

/// Run `f` with a borrowed mutating transient handle. See
/// [`with_transient_storage`].
pub fn with_transient_storage_mut<T: TransientLayout, R>(f: impl FnOnce(&mut TransientStorage<T>) -> R) -> R {
    f(&mut TransientStorage::new())
}
