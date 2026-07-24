//! Building the Ethereum ABI signature string for a method, e.g.
//! `"transfer(uint256,bool)"` — the input to `std_evm_abi::selector`.

use std_evm_abi::{camel_case, solidity_type_name};
use syn::{FnArg, ImplItemFn, Type};

use crate::attrs::selector_override;

/// Build the signature string, honoring `#[selector("...")]` as an
/// exact-match override and applying `camel_case` to the method name
/// otherwise.
///
/// **This is the highest-priority correctness concern in the crate** (per
/// `design/std-evm-macros-spec.md`): a method named `balance_of` must hash
/// to the selector for `balanceOf(address)`, or the contract is silently
/// uncallable from the entire Solidity ecosystem.
pub(crate) fn eth_signature(method: &ImplItemFn) -> syn::Result<String> {
    if let Some(sig) = selector_override(method)? {
        return Ok(sig);
    }

    let name = camel_case(&method.sig.ident.to_string());
    let params: Vec<String> = method
        .sig
        .inputs
        .iter()
        .filter_map(|arg| match arg {
            FnArg::Typed(pt) if !is_storage_param(&pt.ty) => Some(rust_ty_to_solidity(&pt.ty)),
            _ => None,
        })
        .collect();

    Ok(format!("{name}({})", params.join(",")))
}

/// `&Storage`/`&mut Storage` isn't an ABI-visible parameter — the
/// dispatcher injects it, it's never decoded from calldata.
pub(crate) fn is_storage_param(ty: &Type) -> bool {
    match ty {
        Type::Reference(r) => is_storage_param(&r.elem),
        Type::Path(tp) => tp.path.segments.last().is_some_and(|s| s.ident == "Storage"),
        _ => false,
    }
}

/// Map a Rust type path to its Solidity ABI type name.
///
/// The `syn` glue lives here; the canonical name table is
/// [`std_evm_abi::solidity_type_name`], so the macro and runtime never drift.
pub(crate) fn rust_ty_to_solidity(ty: &Type) -> String {
    match ty {
        Type::Path(tp) => {
            let last = tp.path.segments.last().map(|s| s.ident.to_string());
            solidity_type_name(last.as_deref().unwrap_or("")).into()
        }
        Type::Reference(r) => rust_ty_to_solidity(&r.elem),
        _ => "bytes32".into(),
    }
}
