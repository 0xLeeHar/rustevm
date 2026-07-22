//! Inert helper attributes (`#[constructor]`, `#[payable]`, `#[fallback]`,
//! `#[receive]`, `#[selector("...")]`) and the detection/validation logic
//! `#[contract]`'s expansion uses to read and strip them.

use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote;
use syn::{Error, ImplItem, ImplItemFn, ItemImpl, LitStr};

/// Every inert attribute name `#[contract]` recognizes and strips.
pub(crate) const INERT_ATTR_NAMES: &[&str] = &["constructor", "payable", "fallback", "receive", "selector"];

/// Shared body for every inert helper attribute macro.
///
/// These only do something when parsed and stripped by `#[contract]`'s own
/// expansion. Rust expands the outermost attribute macro on an item first,
/// so a correctly-placed `#[payable]` (inside a `#[contract]` impl block)
/// arrives at `#[contract]` as raw, unexpanded tokens and never reaches this
/// function at all — `#[contract]`'s expansion strips it before re-emitting
/// the method. This function only fires when an attribute is used somewhere
/// `#[contract]` never sees it, i.e. misuse, so it exists purely to turn
/// "cannot find attribute" into a clear pointer at the right usage.
pub(crate) fn inert(name: &str, _attr: TokenStream, item: TokenStream) -> TokenStream {
    let msg = format!("`#[{name}]` must be used on a method inside a `#[contract]` impl block");
    let err = Error::new(Span::call_site(), msg).to_compile_error();
    let item: proc_macro2::TokenStream = item.into();
    quote! {
        #err
        #item
    }
    .into()
}

pub(crate) fn has_attr(method: &ImplItemFn, name: &str) -> bool {
    method.attrs.iter().any(|a| a.path().is_ident(name))
}

/// Extract `#[selector("...")]`'s string literal, if present, with a basic
/// shape check. Full grammar validation is deferred to the `trybuild` suite
/// in the follow-up phase.
pub(crate) fn selector_override(method: &ImplItemFn) -> syn::Result<Option<String>> {
    for attr in &method.attrs {
        if attr.path().is_ident("selector") {
            let lit: LitStr = attr.parse_args()?;
            let s = lit.value();
            if s.is_empty() || !s.contains('(') || !s.ends_with(')') {
                return Err(Error::new_spanned(
                    &lit,
                    "#[selector(\"...\")] must be a full signature string, e.g. \"balanceOf(address)\"",
                ));
            }
            return Ok(Some(s));
        }
    }
    Ok(None)
}

/// Remove the inert helper attributes from a method before re-emitting it —
/// left in place, they'd be expanded a second time as top-level attribute
/// macros on `#[contract]`'s own output.
pub(crate) fn strip_inert_attrs(method: &mut ImplItemFn) {
    method
        .attrs
        .retain(|a| !INERT_ATTR_NAMES.iter().any(|n| a.path().is_ident(n)));
}

/// Validate that at most one method in the impl block carries `name`,
/// erroring with both sites named if it's duplicated — a duplicate
/// `#[constructor]`/`#[fallback]`/`#[receive]` is otherwise a silent
/// last-one-wins bug.
pub(crate) fn check_at_most_one(impl_block: &ItemImpl, name: &str) -> syn::Result<()> {
    let mut sites = Vec::new();
    for item in &impl_block.items {
        if let ImplItem::Fn(f) = item
            && has_attr(f, name)
        {
            sites.push(f.sig.ident.clone());
        }
    }
    if sites.len() > 1 {
        return Err(Error::new_spanned(
            &sites[1],
            format!("duplicate #[{name}]: also present on `{}`", sites[0]),
        ));
    }
    Ok(())
}
