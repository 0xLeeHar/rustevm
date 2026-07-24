//! `#[contract]`'s expansion: per-method `Method` impls, the `Dispatch`
//! selector match, and constructor wiring.

use proc_macro2::{Ident, Span, TokenStream};
use quote::quote;
use std_evm_abi::selector;
use syn::{FnArg, GenericArgument, ImplItem, ImplItemFn, ItemImpl, PathArguments, ReturnType, Type, spanned::Spanned};

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

/// `(is_mut, layout_ty)` for a method's leading `&Storage<T>`/`&mut
/// Storage<T>` parameter, if it has one.
type StorageInfo = Option<(bool, Type)>;

/// Split a method's params into an optional leading `&Storage<T>`/`&mut
/// Storage<T>` and the remaining ABI-visible parameters.
fn split_storage_param(method: &ImplItemFn) -> syn::Result<(StorageInfo, Vec<&Type>)> {
    let mut storage_info = None;
    let mut abi_types = Vec::new();

    for arg in &method.sig.inputs {
        match arg {
            FnArg::Receiver(r) => {
                return Err(syn::Error::new_spanned(
                    r,
                    "#[contract] methods must be free functions on the type (no `self`) — \
                     use `&Storage<T>`/`&mut Storage<T>` instead",
                ));
            }
            FnArg::Typed(pt) => {
                if is_storage_param(&pt.ty) {
                    if storage_info.is_some() || !abi_types.is_empty() {
                        return Err(syn::Error::new_spanned(
                            &pt.ty,
                            "`&Storage<T>`/`&mut Storage<T>` must be the first parameter",
                        ));
                    }
                    let is_mut = matches!(&*pt.ty, Type::Reference(r) if r.mutability.is_some());
                    let layout_ty = storage_layout_ty(&pt.ty)?;
                    storage_info = Some((is_mut, layout_ty));
                } else {
                    abi_types.push(pt.ty.as_ref());
                }
            }
        }
    }

    Ok((storage_info, abi_types))
}

/// Extract `T` out of a `&Storage<T>`/`&mut Storage<T>` parameter type —
/// the macro needs it to name the concrete layout in the
/// `with_storage`/`with_storage_mut` turbofish it generates.
fn storage_layout_ty(ty: &Type) -> syn::Result<Type> {
    let inner = match ty {
        Type::Reference(r) => r.elem.as_ref(),
        other => other,
    };
    let Type::Path(tp) = inner else {
        return Err(syn::Error::new_spanned(ty, "expected `&Storage<T>`/`&mut Storage<T>`"));
    };
    let Some(seg) = tp.path.segments.last() else {
        return Err(syn::Error::new_spanned(ty, "expected `&Storage<T>`/`&mut Storage<T>`"));
    };
    let PathArguments::AngleBracketed(args) = &seg.arguments else {
        return Err(syn::Error::new_spanned(
            ty,
            "`Storage` needs a layout type argument, e.g. `&Storage<TokenStorage>` — bare `&Storage` \
             isn't valid",
        ));
    };
    let mut type_args = args.args.iter().filter_map(|a| match a {
        GenericArgument::Type(t) => Some(t.clone()),
        _ => None,
    });
    let Some(layout_ty) = type_args.next() else {
        return Err(syn::Error::new_spanned(
            ty,
            "`Storage` needs exactly one layout type argument",
        ));
    };
    if type_args.next().is_some() {
        return Err(syn::Error::new_spanned(
            ty,
            "`Storage` takes exactly one layout type argument",
        ));
    }
    Ok(layout_ty)
}

fn arg_pat_idents(n: usize) -> Vec<Ident> {
    (0..n)
        .map(|i| Ident::new(&format!("arg{i}"), Span::call_site()))
        .collect()
}

/// Wrap `inner` (a call expression/statement referring to a `storage`
/// binding) in `with_storage`/`with_storage_mut`, or leave it untouched if
/// the method takes no storage parameter. `std-evm`'s helpers construct the
/// handle — the generated code here never calls `Storage::new()` itself, so
/// its constructor can stay genuinely crate-private (see
/// `design/std-evm-storage-spec.md` §1).
fn wrap_with_storage(storage_info: &StorageInfo, inner: TokenStream) -> TokenStream {
    match storage_info {
        Some((true, layout_ty)) => quote! {
            ::std_evm::with_storage_mut::<#layout_ty, _>(|storage| { #inner })
        },
        Some((false, layout_ty)) => quote! {
            ::std_evm::with_storage::<#layout_ty, _>(|storage| { #inner })
        },
        None => inner,
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
    let (storage_info, abi_types) = split_storage_param(method)?;

    let arg_pats = arg_pat_idents(abi_types.len());
    let args_ty = quote! { ( #(#abi_types,)* ) };
    let output_ty = match &method.sig.output {
        ReturnType::Default => quote! { () },
        ReturnType::Type(_, ty) => quote! { #ty },
    };

    let storage_arg = if storage_info.is_some() {
        quote! { storage, }
    } else {
        quote! {}
    };
    let inner_call = quote! { <#self_ty>::#name(#storage_arg #(#arg_pats),*) };
    let call_body = wrap_with_storage(&storage_info, inner_call);

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
                #call_body
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
    let (storage_info, abi_types) = split_storage_param(method)?;

    let arg_pats = arg_pat_idents(abi_types.len());
    let args_ty = quote! { ( #(#abi_types,)* ) };

    let storage_arg = if storage_info.is_some() {
        quote! { storage, }
    } else {
        quote! {}
    };
    let inner_call = quote! { <#self_ty>::#name(#storage_arg #(#arg_pats),*); };
    let call_body = wrap_with_storage(&storage_info, inner_call);

    Ok(quote! {
        impl ::std_evm::Contract for #self_ty {
            fn deploy(calldata: &[u8]) {
                let ( #(#arg_pats,)* ) =
                    <#args_ty as ::std_evm::AbiDecodeArgs>::decode_args(calldata)
                        .unwrap_or_else(|_| panic!("invalid constructor calldata"));
                #call_body
            }
        }
    })
}
