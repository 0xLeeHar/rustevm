// revert.rs — Solidity-compatible revert payloads.
//
// Used by the `std-evm` panic handler so on-chain reverts decode as the standard
// `Error(string)` / `Panic(uint256)` that tooling (ethers, etherscan, …) expects.

use alloc::vec::Vec;

use crate::codec::{AbiEncode, encode_tuple};
use crate::word::U256;

/// Selector for `Error(string)` — `keccak256("Error(string)")[..4]`.
pub const ERROR_SELECTOR: [u8; 4] = [0x08, 0xc3, 0x79, 0xa0];

/// Selector for `Panic(uint256)` — `keccak256("Panic(uint256)")[..4]`.
pub const PANIC_SELECTOR: [u8; 4] = [0x4e, 0x48, 0x7b, 0x71];

/// Encode `Error(string)` revert data: selector ++ ABI-encoded reason.
pub fn encode_error(reason: &str) -> Vec<u8> {
    let mut out = ERROR_SELECTOR.to_vec();
    out.extend_from_slice(&encode_tuple(&[reason.to_component()]));
    out
}

/// Encode `Panic(uint256)` revert data: selector ++ ABI-encoded code.
pub fn encode_panic(code: U256) -> Vec<u8> {
    let mut out = PANIC_SELECTOR.to_vec();
    out.extend_from_slice(&encode_tuple(&[code.to_component()]));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::selector::selector;

    #[test]
    fn selectors_match_signatures() {
        assert_eq!(ERROR_SELECTOR, selector("Error(string)"));
        assert_eq!(PANIC_SELECTOR, selector("Panic(uint256)"));
    }

    #[test]
    fn panic_payload_shape() {
        // selector (4) + one 32-byte word
        assert_eq!(encode_panic(U256::from_u64(0x11)).len(), 4 + 32);
    }
}
