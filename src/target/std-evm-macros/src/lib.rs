//! The user-facing macro layer for writing EVM contracts in Rust.
//! See `design/std-evm-macros-spec.md`.
//!
//! Users never name this crate directly — `std-evm` re-exports everything
//! here. Kept thin per the crate's own testing strategy: all the genuinely
//! tricky logic (type mapping, selector computation, camelCase conversion)
//! lives in `std-evm-abi` and is unit-tested there.

extern crate proc_macro;

use proc_macro::TokenStream;
use syn::{ItemImpl, ItemStruct, parse_macro_input};

mod attrs;
mod contract;
mod layout;
mod signature;
#[cfg(test)]
mod tests;

use contract::expand_contract;
use layout::{expand_storage, expand_transient};

/// Mark an `impl` block as an EVM contract. See the crate-level docs.
#[proc_macro_attribute]
pub fn contract(attr: TokenStream, item: TokenStream) -> TokenStream {
    if !attr.is_empty() {
        return syn::Error::new(proc_macro2::Span::call_site(), "#[contract] takes no arguments")
            .to_compile_error()
            .into();
    }

    let impl_block = parse_macro_input!(item as ItemImpl);
    match expand_contract(impl_block) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

/// Route this method to the initcode constructor section. Inert outside
/// `#[contract]`.
#[proc_macro_attribute]
pub fn constructor(attr: TokenStream, item: TokenStream) -> TokenStream {
    attrs::inert("constructor", attr, item)
}

/// Suppress the auto-injected callvalue guard. Inert outside `#[contract]`.
#[proc_macro_attribute]
pub fn payable(attr: TokenStream, item: TokenStream) -> TokenStream {
    attrs::inert("payable", attr, item)
}

/// Route unmatched selectors to this method. Inert outside `#[contract]`.
#[proc_macro_attribute]
pub fn fallback(attr: TokenStream, item: TokenStream) -> TokenStream {
    attrs::inert("fallback", attr, item)
}

/// Route empty-calldata-with-value calls to this method. Inert outside
/// `#[contract]`.
#[proc_macro_attribute]
pub fn receive(attr: TokenStream, item: TokenStream) -> TokenStream {
    attrs::inert("receive", attr, item)
}

/// Override the auto-derived (camelCase) ABI signature for this method.
/// Inert outside `#[contract]`.
#[proc_macro_attribute]
pub fn selector(attr: TokenStream, item: TokenStream) -> TokenStream {
    attrs::inert("selector", attr, item)
}

/// Turn a plain struct into a storage layout: assigns each field a slot in
/// declaration order and generates typed accessors. See
/// `design/std-evm-storage-spec.md`.
#[proc_macro_attribute]
pub fn storage(attr: TokenStream, item: TokenStream) -> TokenStream {
    if !attr.is_empty() {
        return syn::Error::new(proc_macro2::Span::call_site(), "#[storage] takes no arguments")
            .to_compile_error()
            .into();
    }

    let item_struct = parse_macro_input!(item as ItemStruct);
    match expand_storage(item_struct) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

/// Turn a plain struct into a *transient* storage layout — `#[storage]`'s
/// counterpart on `TLOAD`/`TSTORE`, numbering from 0 in transient's own slot
/// address space. `bool` fields additionally get a `{field}_guard()` RAII
/// accessor, because a transient flag left set lingers to the end of the
/// transaction. See `design/std-evm-storage-spec.md` §7.
#[proc_macro_attribute]
pub fn transient(attr: TokenStream, item: TokenStream) -> TokenStream {
    if !attr.is_empty() {
        return syn::Error::new(proc_macro2::Span::call_site(), "#[transient] takes no arguments")
            .to_compile_error()
            .into();
    }

    let item_struct = parse_macro_input!(item as ItemStruct);
    match expand_transient(item_struct) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}
