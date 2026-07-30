//! `#[contract]`'s expansion: per-method `Method` impls, the `Dispatch`
//! selector match, and constructor wiring.

use proc_macro2::{Ident, Span, TokenStream};
use quote::quote;
use std_evm_abi::selector;
use syn::{FnArg, GenericArgument, ImplItem, ImplItemFn, ItemImpl, PathArguments, ReturnType, Type, spanned::Spanned};

use crate::attrs::{check_at_most_one, has_attr, strip_inert_attrs};
use crate::signature::{eth_signature, is_handle_param};

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

/// Which storage space a handle parameter reaches.
#[derive(Clone, Copy, PartialEq)]
enum HandleKind {
    Persistent,
    Transient,
}

impl HandleKind {
    /// The `Storage`/`TransientStorage` spelling, for diagnostics.
    fn name(self) -> &'static str {
        match self {
            HandleKind::Persistent => "Storage",
            HandleKind::Transient => "TransientStorage",
        }
    }

    /// The local the generated `with_*` closure binds the handle to. The
    /// method body never sees these names — they only have to be distinct
    /// from each other and from `arg{n}`.
    fn binding(self) -> Ident {
        let s = match self {
            HandleKind::Persistent => "storage",
            HandleKind::Transient => "transient",
        };
        Ident::new(s, Span::call_site())
    }
}

/// One handle parameter, in declared order.
struct HandleParam {
    kind: HandleKind,
    is_mut: bool,
    layout_ty: Type,
}

/// Split a method's params into its leading handle parameters and the
/// remaining ABI-visible ones.
///
/// A method may take one of each kind, in either order, but all of them must
/// precede the ABI parameters — the generated call passes bindings
/// positionally, and keeping the injected arguments up front is also what
/// makes a signature readable.
fn split_handle_params(method: &ImplItemFn) -> syn::Result<(Vec<HandleParam>, Vec<&Type>)> {
    let mut handles: Vec<HandleParam> = Vec::new();
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
                if !is_handle_param(&pt.ty) {
                    abi_types.push(pt.ty.as_ref());
                    continue;
                }

                let kind = handle_kind(&pt.ty)?;
                if handles.iter().any(|h| h.kind == kind) {
                    return Err(syn::Error::new_spanned(
                        &pt.ty,
                        format!("a method takes at most one `{}` handle", kind.name()),
                    ));
                }
                if !abi_types.is_empty() {
                    return Err(syn::Error::new_spanned(
                        &pt.ty,
                        format!(
                            "`&{0}<T>`/`&mut {0}<T>` must come before any ABI parameters",
                            kind.name()
                        ),
                    ));
                }
                handles.push(HandleParam {
                    kind,
                    is_mut: matches!(&*pt.ty, Type::Reference(r) if r.mutability.is_some()),
                    layout_ty: handle_layout_ty(&pt.ty, kind)?,
                });
            }
        }
    }

    Ok((handles, abi_types))
}

/// Which handle a parameter type names. Only called on types
/// [`is_handle_param`] already accepted.
fn handle_kind(ty: &Type) -> syn::Result<HandleKind> {
    match ty {
        Type::Reference(r) => handle_kind(&r.elem),
        Type::Path(tp) if tp.path.segments.last().is_some_and(|s| s.ident == "TransientStorage") => {
            Ok(HandleKind::Transient)
        }
        _ => Ok(HandleKind::Persistent),
    }
}

/// Extract `T` out of a `&Storage<T>`/`&mut TransientStorage<T>` parameter
/// type — the macro needs it to name the concrete layout in the `with_*`
/// turbofish it generates.
fn handle_layout_ty(ty: &Type, kind: HandleKind) -> syn::Result<Type> {
    let handle = kind.name();
    let inner = match ty {
        Type::Reference(r) => r.elem.as_ref(),
        other => other,
    };
    let expected = format!("expected `&{handle}<T>`/`&mut {handle}<T>`");
    let Type::Path(tp) = inner else {
        return Err(syn::Error::new_spanned(ty, expected));
    };
    let Some(seg) = tp.path.segments.last() else {
        return Err(syn::Error::new_spanned(ty, expected));
    };
    let PathArguments::AngleBracketed(args) = &seg.arguments else {
        return Err(syn::Error::new_spanned(
            ty,
            format!(
                "`{handle}` needs a layout type argument, e.g. `&{handle}<TokenStorage>` — bare \
                 `&{handle}` isn't valid"
            ),
        ));
    };
    let mut type_args = args.args.iter().filter_map(|a| match a {
        GenericArgument::Type(t) => Some(t.clone()),
        _ => None,
    });
    let Some(layout_ty) = type_args.next() else {
        return Err(syn::Error::new_spanned(
            ty,
            format!("`{handle}` needs exactly one layout type argument"),
        ));
    };
    if type_args.next().is_some() {
        return Err(syn::Error::new_spanned(
            ty,
            format!("`{handle}` takes exactly one layout type argument"),
        ));
    }
    Ok(layout_ty)
}

fn arg_pat_idents(n: usize) -> Vec<Ident> {
    (0..n)
        .map(|i| Ident::new(&format!("arg{i}"), Span::call_site()))
        .collect()
}

/// Wrap `inner` (a call expression/statement referring to the handle
/// bindings) in one `with_*` closure per handle parameter, nesting so that
/// every binding is live around `inner`. Returns `inner` untouched for a
/// method that takes no handles.
///
/// `std-evm`'s helpers construct the handles — the generated code here never
/// calls `Storage::new()` itself, so those constructors can stay genuinely
/// crate-private (see `design/std-evm-storage-spec.md` §1).
fn wrap_with_handles(handles: &[HandleParam], inner: TokenStream) -> TokenStream {
    handles.iter().rev().fold(inner, |acc, h| {
        let layout_ty = &h.layout_ty;
        let binding = h.kind.binding();
        let with = match (h.kind, h.is_mut) {
            (HandleKind::Persistent, false) => quote!(::std_evm::with_storage),
            (HandleKind::Persistent, true) => quote!(::std_evm::with_storage_mut),
            (HandleKind::Transient, false) => quote!(::std_evm::with_transient_storage),
            (HandleKind::Transient, true) => quote!(::std_evm::with_transient_storage_mut),
        };
        quote! {
            #with::<#layout_ty, _>(|#binding| { #acc })
        }
    })
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
    let (handles, abi_types) = split_handle_params(method)?;

    let arg_pats = arg_pat_idents(abi_types.len());
    let args_ty = quote! { ( #(#abi_types,)* ) };
    let output_ty = match &method.sig.output {
        ReturnType::Default => quote! { () },
        ReturnType::Type(_, ty) => quote! { #ty },
    };

    let handle_args: Vec<Ident> = handles.iter().map(|h| h.kind.binding()).collect();
    let inner_call = quote! { <#self_ty>::#name(#(#handle_args,)* #(#arg_pats),*) };
    let call_body = wrap_with_handles(&handles, inner_call);

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
    let (handles, abi_types) = split_handle_params(method)?;

    let arg_pats = arg_pat_idents(abi_types.len());
    let args_ty = quote! { ( #(#abi_types,)* ) };

    let handle_args: Vec<Ident> = handles.iter().map(|h| h.kind.binding()).collect();
    let inner_call = quote! { <#self_ty>::#name(#(#handle_args,)* #(#arg_pats),*); };
    let call_body = wrap_with_handles(&handles, inner_call);

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
