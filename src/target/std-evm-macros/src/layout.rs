//! `#[storage]`'s expansion: assign each field a slot in declaration order
//! (Solidity's algorithm — no packing, see `design/std-evm-storage-spec.md`
//! §2), then generate a `{Name}Access` trait (and its impl for
//! `Storage<Name>`) with one accessor per field. The struct itself is
//! rewritten to a bare marker — its fields are schema, consumed entirely
//! into slot assignment and accessor generation, never real instance data.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Fields, GenericArgument, ItemStruct, PathArguments, Type, spanned::Spanned};

pub(crate) fn expand_storage(item: ItemStruct) -> syn::Result<TokenStream> {
    let name = &item.ident;
    let vis = &item.vis;
    let attrs = &item.attrs;

    let fields = match &item.fields {
        Fields::Named(f) => &f.named,
        _ => {
            return Err(syn::Error::new_spanned(
                &item,
                "#[storage] requires a struct with named fields",
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
                // rather than trying to make `MappingRef`/`MappingMut`
                // generic over "value is itself a Mapping" (that would need
                // a `StorageValue` impl for `Mapping`, which isn't a real
                // storable value — it's a handle).
                Some((k2, v2)) => {
                    let mut_name = format_ident!("{field_name}_mut");
                    trait_methods.push(quote! {
                        fn #field_name(&self, key: #k) -> ::std_evm::MappingRef<'_, #k2, #v2>;
                        fn #mut_name(&mut self, key: #k) -> ::std_evm::MappingMut<'_, #k2, #v2>;
                    });
                    impl_methods.push(quote! {
                        fn #field_name(&self, key: #k) -> ::std_evm::MappingRef<'_, #k2, #v2> {
                            ::std_evm::MappingRef::new(::std_evm::derived_slot(::std_evm::U256::from_u64(#slot), key))
                        }
                        fn #mut_name(&mut self, key: #k) -> ::std_evm::MappingMut<'_, #k2, #v2> {
                            ::std_evm::MappingMut::new(::std_evm::derived_slot(::std_evm::U256::from_u64(#slot), key))
                        }
                    });
                }
                None => {
                    let mut_name = format_ident!("{field_name}_mut");
                    trait_methods.push(quote! {
                        fn #field_name(&self) -> ::std_evm::MappingRef<'_, #k, #v>;
                        fn #mut_name(&mut self) -> ::std_evm::MappingMut<'_, #k, #v>;
                    });
                    impl_methods.push(quote! {
                        fn #field_name(&self) -> ::std_evm::MappingRef<'_, #k, #v> {
                            ::std_evm::MappingRef::new(::std_evm::U256::from_u64(#slot))
                        }
                        fn #mut_name(&mut self) -> ::std_evm::MappingMut<'_, #k, #v> {
                            ::std_evm::MappingMut::new(::std_evm::U256::from_u64(#slot))
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
                        ::std_evm::Slot::<#ty>::new(#slot).read()
                    }
                    fn #setter_name(&mut self, v: #ty) {
                        ::std_evm::Slot::<#ty>::new(#slot).write(v)
                    }
                });
            }
        }
    }

    Ok(quote! {
        #(#attrs)*
        #vis struct #name;

        impl ::std_evm::StorageLayout for #name {}

        pub trait #trait_name {
            #(#trait_methods)*
        }

        impl #trait_name for ::std_evm::Storage<#name> {
            #(#impl_methods)*
        }
    })
}

/// If `ty` is `Mapping<K, V>`, its two type arguments; `None` if `ty` isn't
/// a `Mapping` at all (an ordinary `StorageValue` leaf field).
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
