// method.rs — per-method ABI dispatch: decode → call → encode.

use alloc::vec::Vec;
use std_evm_abi::{AbiDecodeArgs, AbiEncodeOutput, DecodeError};

/// One ABI-callable method: its selector, argument tuple, and return type.
///
/// `#[contract]` generates one zero-sized marker type + `impl Method` per
/// `pub fn`, supplying only `SELECTOR`/`Args`/`Output`/`call` — decode→call→
/// encode is this trait's default `dispatch` body, inherited rather than
/// regenerated per method (see `design/std-evm-macros-spec.md`'s design
/// principle: "the macro is a gap-filler, not a code generator").
pub trait Method {
    const SELECTOR: [u8; 4];
    type Args: AbiDecodeArgs;
    type Output: AbiEncodeOutput;

    /// Calls the real method. Generated per method by `#[contract]` —
    /// forwards to `<ContractType>::method_name(..)`.
    fn call(args: Self::Args) -> Self::Output;

    /// Decode calldata into `Args`, call, and ABI-encode the result. Not
    /// overridden by generated code — this is the shared behaviour the
    /// macro only has to name, not produce.
    fn dispatch(calldata: &[u8]) -> Result<Vec<u8>, DecodeError> {
        let args = Self::Args::decode_args(calldata)?;
        Ok(Self::call(args).encode_output())
    }
}
