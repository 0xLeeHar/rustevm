// dispatch.rs — the selector-`match` entry point `#[contract]` generates.

use alloc::vec::Vec;

use crate::U256;

/// Routes a call's 4-byte selector and calldata to the right [`Method`](crate::Method).
///
/// `#[contract]` generates the implementation for a contract's type; the
/// `match` over `selector` is the one artifact traits genuinely cannot
/// express — Rust has no way to enumerate "all types implementing `Method`"
/// generically.
pub trait Dispatch {
    fn dispatch(selector: [u8; 4], calldata: &[u8], value: U256) -> Vec<u8>;
}

/// The default `Dispatch::dispatch` else-branch body when no method is
/// tagged `#[fallback]` — an empty revert payload for an unmatched selector.
pub fn no_matching_method() -> Vec<u8> {
    Vec::new()
}
