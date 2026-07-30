// selector.rs — Solidity function selectors.

use tiny_keccak::{Hasher, Keccak};

/// The full 32-byte Keccak-256 digest of `bytes`.
///
/// The same hash the EVM's `KECCAK256` opcode computes, in software — for the
/// places that need a digest at *compile* time (selectors) or as a constant
/// (namespaced storage slots), where an opcode isn't available.
pub fn keccak256(bytes: &[u8]) -> [u8; 32] {
    let mut output = [0u8; 32];
    let mut hasher = Keccak::v256();
    hasher.update(bytes);
    hasher.finalize(&mut output);
    output
}

/// Compute the 4-byte Solidity selector `keccak256(sig)[..4]` for a canonical
/// function signature string such as `"transfer(address,uint256)"`.
///
/// This is the single source of truth shared by `std-evm-macros` (which emits
/// `__evm_fn_XXXXXXXX` symbols at compile time) and the runtime dispatcher.
pub fn selector(sig: &str) -> [u8; 4] {
    let d = keccak256(sig.as_bytes());
    [d[0], d[1], d[2], d[3]]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transfer_selector() {
        // ERC-20 transfer(address,uint256) = 0xa9059cbb
        assert_eq!(selector("transfer(address,uint256)"), [0xa9, 0x05, 0x9c, 0xbb]);
    }

    #[test]
    fn balance_of_selector() {
        // ERC-20 balanceOf(address) = 0x70a08231
        assert_eq!(selector("balanceOf(address)"), [0x70, 0xa0, 0x82, 0x31]);
    }
}
