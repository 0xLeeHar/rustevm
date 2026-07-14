extern crate proc_macro;

use proc_macro::TokenStream;
// use quote::quote;
// use syn::{parse_macro_input, ItemFn, ReturnType};

#[proc_macro_attribute]
pub fn evm_opcode(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
    // let mnemonic = parse_macro_input!(attr as syn::Ident).to_string();
    // let func = parse_macro_input!(item as ItemFn);
    //
    // // 1. Look the mnemonic up in the macro's authoritative table.
    // let spec = match OPCODES.iter().find(|o| o.mnemonic == mnemonic) {
    //     Some(s) => s,
    //     None => {
    //         return syn::Error::new_spanned(
    //             &func.sig.ident,
    //             format!("unknown EVM opcode `{mnemonic}`"),
    //         ).to_compile_error().into();
    //     }
    // };
    //
    // // 2. Validate the Rust signature matches the opcode's stack arity.
    // if let Err(e) = validate_signature(&func, spec) {
    //     return e.to_compile_error().into();
    // }
    //
    // // 3. Emit the tagged function + metadata record.
    // expand(func, spec)
}
