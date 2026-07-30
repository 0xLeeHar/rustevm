//! `#[storage]`/`#[transient]`'s expansion: assign each field a slot in
//! declaration order (Solidity's algorithm — no packing, see
//! `design/std-evm-storage-spec.md` §2), then generate a `{Name}Access` trait
//! (and its impl for the matching handle) with one accessor per field. The
//! struct itself is rewritten to a bare marker — its fields are schema,
//! consumed entirely into slot assignment and accessor generation, never real
//! instance data.
//!
//! The two macros differ only in which types the accessors reach for, so the
//! expansion is written once and parameterized by [`Flavor`]. Persistent and
//! transient slots live in separate address spaces, so both number from 0.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Fields, GenericArgument, ItemStruct, PathArguments, Type, spanned::Spanned};

/// The type paths one expansion targets — everything that differs between
/// `#[storage]` and `#[transient]`. Same shape, different opcodes underneath.
struct Flavor {
    layout_trait: TokenStream,
    handle: TokenStream,
    slot_ty: TokenStream,
    map_ref: TokenStream,
    map_mut: TokenStream,
    /// Whether `bool` fields also get a `{field}_guard()` RAII accessor. Only
    /// transient storage has a clearing hazard to guard against — persistent
    /// storage never auto-clears, so there is nothing to forget.
    guards: bool,
}

impl Flavor {
    fn persistent() -> Self {
        Flavor {
            layout_trait: quote!(::std_evm::StorageLayout),
            handle: quote!(::std_evm::Storage),
            slot_ty: quote!(::std_evm::Slot),
            map_ref: quote!(::std_evm::MappingRef),
            map_mut: quote!(::std_evm::MappingMut),
            guards: false,
        }
    }

    fn transient() -> Self {
        Flavor {
            layout_trait: quote!(::std_evm::TransientLayout),
            handle: quote!(::std_evm::TransientStorage),
            slot_ty: quote!(::std_evm::TransientSlot),
            map_ref: quote!(::std_evm::TransientMappingRef),
            map_mut: quote!(::std_evm::TransientMappingMut),
            guards: true,
        }
    }
}

pub(crate) fn expand_storage(item: ItemStruct) -> syn::Result<TokenStream> {
    expand_layout(item, &Flavor::persistent(), "#[storage]")
}

pub(crate) fn expand_transient(item: ItemStruct) -> syn::Result<TokenStream> {
    expand_layout(item, &Flavor::transient(), "#[transient]")
}

fn expand_layout(item: ItemStruct, flavor: &Flavor, macro_name: &str) -> syn::Result<TokenStream> {
    let name = &item.ident;
    let vis = &item.vis;
    let attrs = &item.attrs;

    let Flavor {
        layout_trait,
        handle,
        slot_ty,
        map_ref,
        map_mut,
        guards,
    } = flavor;

    let fields = match &item.fields {
        Fields::Named(f) => &f.named,
        _ => {
            return Err(syn::Error::new_spanned(
                &item,
                format!("{macro_name} requires a struct with named fields"),
            ));
        }
    };

    let trait_name = format_ident!("{name}Access");
    let mut trait_methods = Vec::new();
    let mut impl_methods = Vec::new();

    for (i, field) in fields.iter().enumerate() {
        let field_name = field.ident.as_ref().expect("Fields::Named");
        let ty = &field.ty;
        let slot = i as u64;

        match mapping_generics(ty)? {
            Some((k, v)) => match mapping_generics(&v)? {
                // Nested mapping: flatten into one accessor that takes the
                // outer key and returns a handle over the inner mapping,
                // rather than trying to make the mapping views generic over
                // "value is itself a Mapping" (that would need a
                // `StorageValue` impl for `Mapping`, which isn't a real
                // storable value — it's a handle).
                Some((k2, v2)) => {
                    let mut_name = format_ident!("{field_name}_mut");
                    trait_methods.push(quote! {
                        fn #field_name(&self, key: #k) -> #map_ref<'_, #k2, #v2>;
                        fn #mut_name(&mut self, key: #k) -> #map_mut<'_, #k2, #v2>;
                    });
                    impl_methods.push(quote! {
                        fn #field_name(&self, key: #k) -> #map_ref<'_, #k2, #v2> {
                            #map_ref::new(::std_evm::derived_slot(::std_evm::U256::from_u64(#slot), key))
                        }
                        fn #mut_name(&mut self, key: #k) -> #map_mut<'_, #k2, #v2> {
                            #map_mut::new(::std_evm::derived_slot(::std_evm::U256::from_u64(#slot), key))
                        }
                    });
                }
                None => {
                    let mut_name = format_ident!("{field_name}_mut");
                    trait_methods.push(quote! {
                        fn #field_name(&self) -> #map_ref<'_, #k, #v>;
                        fn #mut_name(&mut self) -> #map_mut<'_, #k, #v>;
                    });
                    impl_methods.push(quote! {
                        fn #field_name(&self) -> #map_ref<'_, #k, #v> {
                            #map_ref::new(::std_evm::U256::from_u64(#slot))
                        }
                        fn #mut_name(&mut self) -> #map_mut<'_, #k, #v> {
                            #map_mut::new(::std_evm::U256::from_u64(#slot))
                        }
                    });
                }
            },
            None => {
                let setter_name = format_ident!("set_{field_name}");
                trait_methods.push(quote! {
                    fn #field_name(&self) -> #ty;
                    fn #setter_name(&mut self, v: #ty);
                });
                impl_methods.push(quote! {
                    fn #field_name(&self) -> #ty {
                        #slot_ty::<#ty>::new(#slot).read()
                    }
                    fn #setter_name(&mut self, v: #ty) {
                        #slot_ty::<#ty>::new(#slot).write(v)
                    }
                });

                // A transient flag left set lingers for the rest of the
                // transaction, so hand out an RAII acquire alongside the raw
                // setter — see `TransientGuard`.
                if *guards && is_bool(ty) {
                    let guard_name = format_ident!("{field_name}_guard");
                    trait_methods.push(quote! {
                        fn #guard_name(&mut self) -> ::std_evm::TransientGuard;
                    });
                    impl_methods.push(quote! {
                        fn #guard_name(&mut self) -> ::std_evm::TransientGuard {
                            ::std_evm::TransientGuard::acquire(::std_evm::U256::from_u64(#slot))
                        }
                    });
                }
            }
        }
    }

    Ok(quote! {
        #(#attrs)*
        #vis struct #name;

        impl #layout_trait for #name {}

        pub trait #trait_name {
            #(#trait_methods)*
        }

        impl #trait_name for #handle<#name> {
            #(#impl_methods)*
        }
    })
}

/// Whether `ty` is `bool` — the field types that get an RAII guard under
/// `#[transient]`.
fn is_bool(ty: &Type) -> bool {
    matches!(ty, Type::Path(tp) if tp.qself.is_none() && tp.path.is_ident("bool"))
}

/// If `ty` is `Mapping<K, V>`, its two type arguments; `None` if `ty` isn't
/// a `Mapping` at all (an ordinary leaf field).
fn mapping_generics(ty: &Type) -> syn::Result<Option<(Type, Type)>> {
    let Type::Path(tp) = ty else { return Ok(None) };
    let Some(seg) = tp.path.segments.last() else {
        return Ok(None);
    };
    if seg.ident != "Mapping" {
        return Ok(None);
    }
    let PathArguments::AngleBracketed(args) = &seg.arguments else {
        return Err(syn::Error::new(
            ty.span(),
            "Mapping must have two type arguments: Mapping<K, V>",
        ));
    };
    let types: Vec<Type> = args
        .args
        .iter()
        .filter_map(|a| match a {
            GenericArgument::Type(t) => Some(t.clone()),
            _ => None,
        })
        .collect();
    let [k, v]: [Type; 2] = types
        .try_into()
        .map_err(|_| syn::Error::new(ty.span(), "Mapping must have exactly two type arguments: Mapping<K, V>"))?;
    Ok(Some((k, v)))
}
