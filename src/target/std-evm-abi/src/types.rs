// types.rs — canonical Rust-type-name → Solidity-ABI-type-name mapping.

use alloc::string::String;

/// Map a Rust type identifier (the final path segment, e.g. `"u64"`, `"U256"`,
/// `"Address"`) to its canonical Solidity ABI type name.
///
/// This is the single source of truth for the mapping. `std-evm-macros` keeps
/// the `syn`-specific glue (extracting the identifier from a `syn::Type`) and
/// calls this function, so the macro and any runtime reflection never drift.
///
/// The fallback is `bytes32`; annotate with a `#[solidity_type = "…"]` override
/// when a precise mapping matters.
pub fn solidity_type_name(rust_ident: &str) -> &'static str {
    match rust_ident {
        "u8" => "uint8",
        "u16" => "uint16",
        "u32" => "uint32",
        "u64" => "uint64",
        "u128" => "uint128",
        "i8" => "int8",
        "i16" => "int16",
        "i32" => "int32",
        "i64" => "int64",
        "i128" => "int128",
        "bool" => "bool",
        "U256" => "uint256",
        "Address" => "address",
        "Bytes32" => "bytes32",
        _ => "bytes32",
    }
}

/// Convert a Rust `snake_case` identifier to Solidity's `camelCase` naming
/// convention.
///
/// A method named `balance_of` hashes to a selector no Solidity caller will
/// ever compute unless it's converted to `balanceOf` first — see
/// `design/std-evm-macros-spec.md`'s "naming problem" section. Consecutive or
/// leading/trailing underscores are collapsed away rather than producing
/// empty segments; an already-camelCase input (no underscores) passes
/// through unchanged.
pub fn camel_case(ident: &str) -> String {
    let mut out = String::with_capacity(ident.len());
    for (i, segment) in ident.split('_').filter(|s| !s.is_empty()).enumerate() {
        if i == 0 {
            out.push_str(segment);
        } else {
            let mut chars = segment.chars();
            if let Some(first) = chars.next() {
                out.extend(first.to_uppercase());
                out.push_str(chars.as_str());
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_mappings() {
        assert_eq!(solidity_type_name("u64"), "uint64");
        assert_eq!(solidity_type_name("U256"), "uint256");
        assert_eq!(solidity_type_name("Address"), "address");
        assert_eq!(solidity_type_name("i128"), "int128");
    }

    #[test]
    fn fallback_is_bytes32() {
        assert_eq!(solidity_type_name("SomethingElse"), "bytes32");
    }

    #[test]
    fn camel_case_converts_snake_case() {
        assert_eq!(camel_case("balance_of"), "balanceOf");
        assert_eq!(camel_case("transfer_from"), "transferFrom");
    }

    #[test]
    fn camel_case_passes_through_no_underscores() {
        assert_eq!(camel_case("transfer"), "transfer");
    }

    #[test]
    fn camel_case_collapses_stray_underscores() {
        assert_eq!(camel_case("_leading"), "leading");
        assert_eq!(camel_case("trailing_"), "trailing");
        assert_eq!(camel_case("double__underscore"), "doubleUnderscore");
    }
}
