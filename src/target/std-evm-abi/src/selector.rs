// selector.rs — Solidity function selectors.

use tiny_keccak::{Hasher, Keccak};

/// Compute the 4-byte Solidity selector `keccak256(sig)[..4]` for a canonical
/// function signature string such as `"transfer(address,uint256)"`.
///
/// This is the single source of truth shared by `std-evm-macros` (which emits
/// `__evm_fn_XXXXXXXX` symbols at compile time) and the runtime dispatcher.
pub fn selector(sig: &str) -> [u8; 4] {
    let mut output = [0u8; 32];
    let mut hasher = Keccak::v256();
    hasher.update(sig.as_bytes());
    hasher.finalize(&mut output);
    [output[0], output[1], output[2], output[3]]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transfer_selector() {
        // ERC-20 transfer(address,uint256) = 0xa9059cbb
        assert_eq!(
            selector("transfer(address,uint256)"),
            [0xa9, 0x05, 0x9c, 0xbb]
        );
    }

    #[test]
    fn balance_of_selector() {
        // ERC-20 balanceOf(address) = 0x70a08231
        assert_eq!(selector("balanceOf(address)"), [0x70, 0xa0, 0x82, 0x31]);
    }
}
