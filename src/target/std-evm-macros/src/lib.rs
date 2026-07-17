extern crate proc_macro;

use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote;
use std_evm_abi::{selector, solidity_type_name};
use syn::{
    Error, FnArg, Ident, ImplItem, ImplItemFn, ItemFn, ItemImpl, Pat, ReturnType, Type,
    parse_macro_input, spanned::Spanned,
};

// ── #[contract] ───────────────────────────────────────────────────────────────

/// Mark an `impl` block as an EVM contract.
///
/// Applied to an `impl` block, this macro:
///
/// 1. Passes through the impl block unchanged.
/// 2. For each `pub fn` that is **not** tagged `#[constructor]`, generates a
///    `#[no_mangle] pub extern "C" fn __evm_fn_XXXXXXXX()` shim, where
///    `XXXXXXXX` is the first 4 bytes of `keccak256("name(types…)")` encoded as
///    lowercase hex.  The codegen backend uses these shims to build the ABI
///    dispatcher.
///
/// # Example
///
/// ```rust,ignore
/// use std_evm_macros::{constructor, contract};
///
/// struct Counter;
///
/// #[contract]
/// impl Counter {
///     #[constructor]
///     pub fn new() {}
///
///     pub fn increment() { /* … */ }
///     pub fn value() -> u64 { /* … */ }
/// }
/// ```
#[proc_macro_attribute]
pub fn contract(attr: TokenStream, item: TokenStream) -> TokenStream {
    if !attr.is_empty() {
        return Error::new(Span::call_site(), "#[contract] takes no arguments")
            .to_compile_error()
            .into();
    }

    let impl_block = parse_macro_input!(item as ItemImpl);

    match expand_contract(impl_block) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

fn expand_contract(impl_block: ItemImpl) -> syn::Result<proc_macro2::TokenStream> {
    // Collect external (pub, non-constructor) functions.
    let mut shims = Vec::new();

    for item in &impl_block.items {
        let ImplItem::Fn(method) = item else { continue };

        // Only `pub` functions become external entry points.
        if !matches!(method.vis, syn::Visibility::Public(_)) {
            continue;
        }

        // Skip functions tagged #[constructor] — handled by that macro.
        if has_attr(method, "constructor") {
            continue;
        }

        shims.push(external_shim(method, &impl_block.self_ty)?);
    }

    Ok(quote! {
        #impl_block
        #(#shims)*
    })
}

/// Generate `#[no_mangle] pub extern "C" fn __evm_fn_XXXXXXXX() { … }`.
fn external_shim(method: &ImplItemFn, self_ty: &Type) -> syn::Result<proc_macro2::TokenStream> {
    let name = &method.sig.ident;

    // Build the Ethereum function signature string, e.g. "transfer(uint256,bool)".
    let sig_str = eth_signature(method)?;
    let sel = selector(&sig_str);
    let shim_name = Ident::new(
        &format!("__evm_fn_{:08x}", u32::from_be_bytes(sel)),
        name.span(),
    );

    // Collect arg names for the forwarding call.
    let arg_names: Vec<_> = method.sig.inputs.iter().filter_map(arg_ident).collect();

    // Collect arg types for the shim signature (drop `self`).
    let typed_args: Vec<_> = method
        .sig
        .inputs
        .iter()
        .filter_map(|a| match a {
            FnArg::Typed(pt) => Some(pt),
            FnArg::Receiver(_) => None,
        })
        .collect();

    let ret = &method.sig.output;
    let selector_hex = format!("{:08x}", u32::from_be_bytes(sel));
    let doc = format!(
        "ABI shim for `{self_ty}::{name}`.  \
         Selector `0x{selector_hex}` = `keccak256(\"{sig_str}\")[..4]`.",
        self_ty = quote!(#self_ty),
    );

    Ok(quote! {
        #[doc = #doc]
        #[no_mangle]
        pub extern "C" fn #shim_name(#(#typed_args),*) #ret {
            <#self_ty>::#name(#(#arg_names),*)
        }
    })
}

// ── #[constructor] ────────────────────────────────────────────────────────────

/// Mark a function as the EVM contract constructor.
///
/// Applied to a free function **or** an `impl` method, this macro:
///
/// 1. Passes through the original function unchanged.
/// 2. Emits a `#[no_mangle] pub extern "C" fn __evm_constructor()` shim that
///    the codegen backend uses as the constructor entry point.
///
/// Only one `#[constructor]` may appear per contract.  The codegen backend
/// enforces this at link time (duplicate symbol `__evm_constructor`).
///
/// # Example
///
/// ```rust,ignore
/// use std_evm_macros::constructor;
///
/// #[constructor]
/// pub fn deploy() {
///     // initialise storage …
/// }
/// ```
#[proc_macro_attribute]
pub fn constructor(attr: TokenStream, item: TokenStream) -> TokenStream {
    if !attr.is_empty() {
        return Error::new(Span::call_site(), "#[constructor] takes no arguments")
            .to_compile_error()
            .into();
    }

    // Support both free functions and impl methods (when used alongside
    // #[contract]).  Try free fn first; if that fails, try impl method.
    if let Ok(func) = syn::parse::<ItemFn>(item.clone()) {
        match expand_constructor_fn(func) {
            Ok(ts) => ts.into(),
            Err(e) => e.to_compile_error().into(),
        }
    } else {
        // Inside an #[contract] impl block the outer macro handles the shim;
        // just pass the method through so it stays in the impl.
        item
    }
}

fn expand_constructor_fn(func: ItemFn) -> syn::Result<proc_macro2::TokenStream> {
    let name = &func.sig.ident;

    // Collect arg names / types for the shim.
    let arg_names: Vec<_> = func.sig.inputs.iter().filter_map(arg_ident).collect();
    let typed_args: Vec<_> = func
        .sig
        .inputs
        .iter()
        .filter_map(|a| match a {
            FnArg::Typed(pt) => Some(pt),
            FnArg::Receiver(_) => None,
        })
        .collect();

    // Constructors must not return a value — the contract runtime handles
    // copying the runtime bytecode into the output.
    if !matches!(func.sig.output, ReturnType::Default) {
        return Err(Error::new(
            func.sig.output.span(),
            "#[constructor] function must have no return type",
        ));
    }

    Ok(quote! {
        #func

        /// EVM constructor entry point.
        ///
        /// Generated by `#[constructor]`.  The codegen backend replaces the body
        /// with proper ABI argument decoding from calldata before calling the
        /// original constructor function.
        #[no_mangle]
        pub extern "C" fn __evm_constructor(#(#typed_args),*) {
            #name(#(#arg_names),*)
        }
    })
}

// ── helpers ───────────────────────────────────────────────────────────────────

/// Return `true` if `method` has an attribute named `ident`.
fn has_attr(method: &ImplItemFn, ident: &str) -> bool {
    method.attrs.iter().any(|a| a.path().is_ident(ident))
}

/// Extract the identifier from a typed function argument.
fn arg_ident(arg: &FnArg) -> Option<proc_macro2::TokenStream> {
    match arg {
        FnArg::Typed(pt) => match pt.pat.as_ref() {
            Pat::Ident(pi) => {
                let id = &pi.ident;
                Some(quote!(#id))
            }
            _ => None,
        },
        FnArg::Receiver(_) => None,
    }
}

/// Build the Ethereum ABI function signature string for a method, e.g.
/// `"transfer(uint256,bool)"`.
///
/// The Rust-to-Solidity type mapping used here is conservative:
///
/// | Rust      | Solidity   |
/// |-----------|------------|
/// | `u8`      | `uint8`    |
/// | `u16`     | `uint16`   |
/// | `u32`     | `uint32`   |
/// | `u64`     | `uint64`   |
/// | `u128`    | `uint128`  |
/// | `i8`      | `int8`     |
/// | `i16`     | `int16`    |
/// | `i32`     | `int32`    |
/// | `i64`     | `int64`    |
/// | `i128`    | `int128`   |
/// | `bool`    | `bool`     |
/// | `U256`    | `uint256`  |
/// | `Address` | `address`  |
/// | other     | `bytes32`  | (fallback — annotate with `#[solidity_type = "…"]` when precise mapping matters)
fn eth_signature(method: &ImplItemFn) -> syn::Result<String> {
    let name = method.sig.ident.to_string();

    let params: Vec<String> = method
        .sig
        .inputs
        .iter()
        .filter_map(|arg| match arg {
            FnArg::Typed(pt) => Some(rust_ty_to_solidity(&pt.ty)),
            FnArg::Receiver(_) => None,
        })
        .collect();

    Ok(format!("{}({})", name, params.join(",")))
}

/// Map a Rust type path to its Solidity ABI type name.
///
/// The `syn` glue lives here; the canonical name table is
/// [`std_evm_abi::solidity_type_name`], so the macro and runtime never drift.
fn rust_ty_to_solidity(ty: &Type) -> String {
    match ty {
        Type::Path(tp) => {
            let last = tp.path.segments.last().map(|s| s.ident.to_string());
            solidity_type_name(last.as_deref().unwrap_or("")).into()
        }
        Type::Reference(r) => rust_ty_to_solidity(&r.elem),
        _ => "bytes32".into(),
    }
}
