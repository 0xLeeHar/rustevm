//! Unit tests on `expand_contract` directly: parse a sample `impl` block,
//! expand it, and assert on the generated tokens. Proc-macro crates can
//! only really be exercised through expansion, but `expand_contract` itself
//! is a plain function over `proc_macro2::TokenStream`/`syn` types, so it's
//! testable without the `proc_macro`/`trybuild` machinery.

use syn::{ItemImpl, ItemStruct};

use crate::contract::expand_contract;
use crate::layout::expand_storage;

fn expand(src: &str) -> syn::Result<String> {
    let impl_block: ItemImpl = syn::parse_str(src)?;
    expand_contract(impl_block).map(|ts| ts.to_string())
}

fn expand_layout(src: &str) -> syn::Result<String> {
    let item_struct: ItemStruct = syn::parse_str(src)?;
    expand_storage(item_struct).map(|ts| ts.to_string())
}

#[test]
fn plain_method_generates_dispatch_impl() {
    let out = expand(
        r#"
        impl Counter {
            pub fn value() -> u64 { 0 }
        }
        "#,
    )
    .unwrap();

    assert!(out.contains("impl :: std_evm :: Dispatch for Counter"));
    assert!(out.contains("impl :: std_evm :: Method for"));
    assert!(
        out.contains("guard_not_payable"),
        "non-#[payable] methods must guard callvalue"
    );
}

#[test]
fn self_receiver_is_rejected() {
    let err = expand(
        r#"
        impl Counter {
            pub fn value(&self) -> u64 { 0 }
        }
        "#,
    )
    .unwrap_err();
    assert!(err.to_string().contains("no `self`"));
}

#[test]
fn payable_suppresses_guard() {
    let out = expand(
        r#"
        impl Wallet {
            #[payable]
            pub fn deposit() {}
        }
        "#,
    )
    .unwrap();
    assert!(!out.contains("guard_not_payable"));
}

#[test]
fn constructor_generates_contract_impl() {
    let out = expand(
        r#"
        impl Token {
            #[constructor]
            pub fn init(supply: U256) {}
        }
        "#,
    )
    .unwrap();
    assert!(out.contains("impl :: std_evm :: Contract for Token"));
    // The constructor itself must not also become a normal dispatch arm —
    // a `Dispatch` impl is still generated (always-fallback body is fine
    // for a contract with no other callable methods).
    assert!(!out.contains("__Method_init"));
}

#[test]
fn no_constructor_means_no_contract_impl() {
    let out = expand(
        r#"
        impl Token {
            pub fn total_supply() -> U256 { U256::ZERO }
        }
        "#,
    )
    .unwrap();
    assert!(!out.contains(":: std_evm :: Contract for"));
}

#[test]
fn constructor_with_return_type_errors() {
    let err = expand(
        r#"
        impl Token {
            #[constructor]
            pub fn init() -> bool { true }
        }
        "#,
    )
    .unwrap_err();
    assert!(err.to_string().contains("no return type"));
}

#[test]
fn selector_override_changes_bytes() {
    let default_sel = expand(
        r#"
        impl Token {
            pub fn balance_of(who: Address) -> U256 { U256::ZERO }
        }
        "#,
    )
    .unwrap();

    let overridden = expand(
        r#"
        impl Token {
            #[selector("differentName(address)")]
            pub fn balance_of(who: Address) -> U256 { U256::ZERO }
        }
        "#,
    )
    .unwrap();

    assert_ne!(default_sel, overridden);
}

#[test]
fn malformed_selector_override_errors() {
    let err = expand(
        r#"
        impl Token {
            #[selector("not a signature")]
            pub fn balance_of(who: Address) -> U256 { U256::ZERO }
        }
        "#,
    )
    .unwrap_err();
    assert!(err.to_string().contains("signature string"));
}

#[test]
fn duplicate_fallback_errors() {
    let err = expand(
        r#"
        impl Token {
            #[fallback]
            pub fn a() {}
            #[fallback]
            pub fn b() {}
        }
        "#,
    )
    .unwrap_err();
    assert!(err.to_string().contains("duplicate #[fallback]"));
}

#[test]
fn duplicate_constructor_errors() {
    let err = expand(
        r#"
        impl Token {
            #[constructor]
            pub fn a() {}
            #[constructor]
            pub fn b() {}
        }
        "#,
    )
    .unwrap_err();
    assert!(err.to_string().contains("duplicate #[constructor]"));
}

#[test]
fn selector_collision_errors() {
    let err = expand(
        r#"
        impl Token {
            #[selector("same(uint256)")]
            pub fn a(x: U256) {}
            #[selector("same(uint256)")]
            pub fn b(x: U256) {}
        }
        "#,
    )
    .unwrap_err();
    assert!(err.to_string().contains("selector collision"));
}

#[test]
fn non_pub_method_gets_no_dispatch_entry() {
    let out = expand(
        r#"
        impl Token {
            fn internal_helper() {}
            pub fn total_supply() -> U256 { U256::ZERO }
        }
        "#,
    )
    .unwrap();
    // The original method is passed through unchanged (so its name still
    // appears as plain source), but it must not get a `Method` marker/impl.
    assert!(!out.contains("__Method_internal_helper"));
    assert!(out.contains("__Method_total_supply"));
}

#[test]
fn storage_param_excluded_from_args() {
    let out = expand(
        r#"
        impl Token {
            pub fn balance_of(s: &Storage<TokenStorage>, who: Address) -> U256 { U256::ZERO }
        }
        "#,
    )
    .unwrap();
    // Args tuple should only contain the Address type, not Storage.
    assert!(out.contains("type Args = (Address ,) ;"));
}

#[test]
fn bare_storage_without_layout_type_errors() {
    let err = expand(
        r#"
        impl Token {
            pub fn balance_of(s: &Storage, who: Address) -> U256 { U256::ZERO }
        }
        "#,
    )
    .unwrap_err();
    assert!(err.to_string().contains("layout type argument"));
}

#[test]
fn view_method_wraps_call_in_with_storage() {
    let out = expand(
        r#"
        impl Token {
            pub fn balance_of(s: &Storage<TokenStorage>, who: Address) -> U256 { U256::ZERO }
        }
        "#,
    )
    .unwrap();
    assert!(out.contains(":: std_evm :: with_storage :: < TokenStorage , _ >"));
    assert!(!out.contains("with_storage_mut"));
}

#[test]
fn mutating_method_wraps_call_in_with_storage_mut() {
    let out = expand(
        r#"
        impl Token {
            pub fn transfer(s: &mut Storage<TokenStorage>, to: Address, amt: U256) {}
        }
        "#,
    )
    .unwrap();
    assert!(out.contains(":: std_evm :: with_storage_mut :: < TokenStorage , _ >"));
}

#[test]
fn constructor_wraps_call_in_with_storage_mut() {
    let out = expand(
        r#"
        impl Token {
            #[constructor]
            pub fn init(s: &mut Storage<TokenStorage>, supply: U256) {}
        }
        "#,
    )
    .unwrap();
    assert!(out.contains(":: std_evm :: with_storage_mut :: < TokenStorage , _ >"));
}

#[test]
fn no_storage_param_means_no_wrapping() {
    let out = expand(
        r#"
        impl Counter {
            pub fn value() -> u64 { 0 }
        }
        "#,
    )
    .unwrap();
    assert!(!out.contains("with_storage"));
}

#[test]
fn storage_layout_generates_marker_and_access_impl() {
    let out = expand_layout(
        r#"
        struct TokenStorage {
            total_supply: U256,
            balances: Mapping<Address, U256>,
        }
        "#,
    )
    .unwrap();

    assert!(
        out.contains("struct TokenStorage ;"),
        "fields must be consumed, not kept: {out}"
    );
    assert!(out.contains("impl :: std_evm :: StorageLayout for TokenStorage"));
    assert!(out.contains("trait TokenStorageAccess"));
    assert!(out.contains("impl TokenStorageAccess for :: std_evm :: Storage < TokenStorage >"));

    // Plain field: getter + setter over a Slot.
    assert!(out.contains("fn total_supply (& self) -> U256"));
    assert!(out.contains("fn set_total_supply (& mut self , v : U256)"));
    assert!(out.contains(":: std_evm :: Slot :: < U256 > :: new (0u64)"));

    // Mapping field: MappingRef/MappingMut accessors, no hashing at this level.
    assert!(out.contains("fn balances (& self) -> :: std_evm :: MappingRef < '_ , Address , U256 >"));
    assert!(out.contains("fn balances_mut (& mut self) -> :: std_evm :: MappingMut < '_ , Address , U256 >"));
}

#[test]
fn nested_mapping_generates_flattened_accessor() {
    let out = expand_layout(
        r#"
        struct TokenStorage {
            allowances: Mapping<Address, Mapping<Address, U256>>,
        }
        "#,
    )
    .unwrap();

    // Flattened: takes the outer key directly, returns a handle over the inner mapping.
    assert!(out.contains("fn allowances (& self , key : Address) -> :: std_evm :: MappingRef < '_ , Address , U256 >"));
    assert!(out.contains("derived_slot"));
}

#[test]
fn storage_layout_rejects_tuple_struct() {
    let err = expand_layout("struct TokenStorage(U256);").unwrap_err();
    assert!(err.to_string().contains("named fields"));
}
