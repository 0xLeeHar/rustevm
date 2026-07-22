//! `#[contract]`'s expansion: per-method `Method` impls, the `Dispatch`
//! selector match, and constructor wiring.

use proc_macro2::{Ident, Span, TokenStream};
use quote::quote;
use std_evm_abi::selector;
use syn::{FnArg, ImplItem, ImplItemFn, ItemImpl, ReturnType, Type, spanned::Spanned};

use crate::attrs::{check_at_most_one, has_attr, strip_inert_attrs};
use crate::signature::{eth_signature, is_storage_param};

pub(crate) fn expand_contract(mut impl_block: ItemImpl) -> syn::Result<TokenStream> {
    check_at_most_one(&impl_block, "constructor")?;
    check_at_most_one(&impl_block, "fallback")?;
    check_at_most_one(&impl_block, "receive")?;

    let self_ty = impl_block.self_ty.clone();

    let mut method_impls = Vec::new();
    let mut dispatch_arms = Vec::new();
    let mut fallback_call = None;
    let mut receive_call = None;
    let mut constructor_impl = None;
    let mut seen_selectors: Vec<([u8; 4], Ident)> = Vec::new();

    for item in &mut impl_block.items {
        let ImplItem::Fn(method) = item else { continue };

        if !matches!(method.vis, syn::Visibility::Public(_)) {
            strip_inert_attrs(method);
            continue;
        }

        if has_attr(method, "constructor") {
            constructor_impl = Some(expand_constructor(method, &self_ty)?);
            strip_inert_attrs(method);
            continue;
        }

        let is_fallback = has_attr(method, "fallback");
        let is_receive = has_attr(method, "receive");
        let is_payable = has_attr(method, "payable");

        let sig_str = eth_signature(method)?;
        let sel = selector(&sig_str);

        if !is_fallback && !is_receive {
            if let Some((_, other)) = seen_selectors.iter().find(|(s, _)| *s == sel) {
                return Err(syn::Error::new_spanned(
                    &method.sig.ident,
                    format!(
                        "selector collision: `{other}` and `{}` both hash to 0x{}",
                        method.sig.ident,
                        hex4(sel)
                    ),
                ));
            }
            seen_selectors.push((sel, method.sig.ident.clone()));
        }

        let marker = method_marker_ident(&method.sig.ident);
        let (method_impl, call_expr) = expand_method(method, &self_ty, &marker, sel, is_payable)?;
        method_impls.push(method_impl);

        if is_fallback {
            fallback_call = Some(call_expr);
        } else if is_receive {
            receive_call = Some(call_expr);
        } else {
            let sel_bytes = sel;
            dispatch_arms.push(quote! { [#(#sel_bytes),*] => { #call_expr } });
        }

        strip_inert_attrs(method);
    }

    let fallback_branch = fallback_call.unwrap_or_else(|| quote! { ::std_evm::no_matching_method() });

    let receive_branch = receive_call.map(|call| {
        quote! {
            if calldata.is_empty() && value != ::std_evm::U256::ZERO {
                return #call;
            }
        }
    });

    let dispatch_impl = quote! {
        impl ::std_evm::Dispatch for #self_ty {
            fn dispatch(selector: [u8; 4], calldata: &[u8], value: ::std_evm::U256) -> ::std_evm::Vec<u8> {
                #receive_branch
                match selector {
                    #(#dispatch_arms)*
                    _ => { #fallback_branch }
                }
            }
        }
    };

    Ok(quote! {
        #impl_block
        #(#method_impls)*
        #dispatch_impl
        #constructor_impl
    })
}

/// A `[u8; 4]` selector as a lowercase hex string, for diagnostics.
fn hex4(sel: [u8; 4]) -> String {
    sel.iter().map(|b| format!("{b:02x}")).collect()
}

fn method_marker_ident(name: &Ident) -> Ident {
    Ident::new(&format!("__Method_{name}"), name.span())
}

/// Split a method's params into an optional leading `&Storage`/`&mut Storage`
/// and the remaining ABI-visible parameters.
fn split_storage_param(method: &ImplItemFn) -> syn::Result<(Option<bool>, Vec<&Type>)> {
    let mut storage_kind = None;
    let mut abi_types = Vec::new();

    for arg in &method.sig.inputs {
        match arg {
            FnArg::Receiver(r) => {
                return Err(syn::Error::new_spanned(
                    r,
                    "#[contract] methods must be free functions on the type (no `self`) — \
                     use `&Storage`/`&mut Storage` instead",
                ));
            }
            FnArg::Typed(pt) => {
                if is_storage_param(&pt.ty) {
                    if storage_kind.is_some() || !abi_types.is_empty() {
                        return Err(syn::Error::new_spanned(
                            &pt.ty,
                            "`&Storage`/`&mut Storage` must be the first parameter",
                        ));
                    }
                    let is_mut = matches!(&*pt.ty, Type::Reference(r) if r.mutability.is_some());
                    storage_kind = Some(is_mut);
                } else {
                    abi_types.push(pt.ty.as_ref());
                }
            }
        }
    }

    Ok((storage_kind, abi_types))
}

fn arg_pat_idents(n: usize) -> Vec<Ident> {
    (0..n)
        .map(|i| Ident::new(&format!("arg{i}"), Span::call_site()))
        .collect()
}

fn storage_binding(storage_kind: Option<bool>) -> (TokenStream, TokenStream) {
    match storage_kind {
        Some(true) => (
            quote! { let mut storage = ::std_evm::Storage::new(); },
            quote! { &mut storage, },
        ),
        Some(false) => (
            quote! { let storage = ::std_evm::Storage::new(); },
            quote! { &storage, },
        ),
        None => (quote! {}, quote! {}),
    }
}

/// Generate the marker struct + `impl Method`, and the call expression that
/// slots into a dispatch arm / fallback / receive branch.
fn expand_method(
    method: &ImplItemFn,
    self_ty: &Type,
    marker: &Ident,
    sel: [u8; 4],
    is_payable: bool,
) -> syn::Result<(TokenStream, TokenStream)> {
    let name = &method.sig.ident;
    let (storage_kind, abi_types) = split_storage_param(method)?;

    let arg_pats = arg_pat_idents(abi_types.len());
    let args_ty = quote! { ( #(#abi_types,)* ) };
    let output_ty = match &method.sig.output {
        ReturnType::Default => quote! { () },
        ReturnType::Type(_, ty) => quote! { #ty },
    };

    let (storage_let, storage_arg) = storage_binding(storage_kind);
    let guard = if is_payable {
        quote! {}
    } else {
        quote! { ::std_evm::guard_not_payable(); }
    };
    let sel_bytes = sel;

    let method_impl = quote! {
        #[allow(non_camel_case_types)]
        struct #marker;

        impl ::std_evm::Method for #marker {
            const SELECTOR: [u8; 4] = [#(#sel_bytes),*];
            type Args = #args_ty;
            type Output = #output_ty;

            fn call(args: Self::Args) -> Self::Output {
                #guard
                let ( #(#arg_pats,)* ) = args;
                #storage_let
                <#self_ty>::#name(#storage_arg #(#arg_pats),*)
            }
        }
    };

    let call_expr = quote! {
        <#marker as ::std_evm::Method>::dispatch(calldata)
            .unwrap_or_else(|_| ::std_evm::encode_error("invalid calldata"))
    };

    Ok((method_impl, call_expr))
}

/// Wire the `#[constructor]`-tagged method into `impl Contract for
/// <ContractType>`.
fn expand_constructor(method: &ImplItemFn, self_ty: &Type) -> syn::Result<TokenStream> {
    if !matches!(method.sig.output, ReturnType::Default) {
        return Err(syn::Error::new(
            method.sig.output.span(),
            "#[constructor] function must have no return type",
        ));
    }

    let name = &method.sig.ident;
    let (storage_kind, abi_types) = split_storage_param(method)?;

    let arg_pats = arg_pat_idents(abi_types.len());
    let args_ty = quote! { ( #(#abi_types,)* ) };
    let (storage_let, storage_arg) = storage_binding(storage_kind);

    Ok(quote! {
        impl ::std_evm::Contract for #self_ty {
            fn deploy(calldata: &[u8]) {
                let ( #(#arg_pats,)* ) =
                    <#args_ty as ::std_evm::AbiDecodeArgs>::decode_args(calldata)
                        .unwrap_or_else(|_| panic!("invalid constructor calldata"));
                #storage_let
                <#self_ty>::#name(#storage_arg #(#arg_pats),*);
            }
        }
    })
}
