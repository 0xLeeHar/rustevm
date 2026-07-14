use crate::U256;
use evm_sys_macros::evm_opcode;

/// BLOCKHASH — get the hash of one of the 256 most recent blocks.
/// Returns zero if `block_number` is not within the last 256 blocks.
#[evm_opcode(BLOCKHASH)]
pub unsafe fn blockhash(block_number: U256) -> U256 {
    unsafe { core::hint::unreachable_unchecked() }
}

/// COINBASE — beneficiary address of the current block.
#[evm_opcode(COINBASE)]
pub unsafe fn coinbase() -> U256 {
    unsafe { core::hint::unreachable_unchecked() }
}

/// TIMESTAMP — Unix timestamp of the current block.
#[evm_opcode(TIMESTAMP)]
pub unsafe fn timestamp() -> U256 {
    unsafe { core::hint::unreachable_unchecked() }
}

/// NUMBER — current block number.
#[evm_opcode(NUMBER)]
pub unsafe fn number() -> U256 {
    unsafe { core::hint::unreachable_unchecked() }
}

/// PREVRANDAO — post-merge randomness beacon (formerly DIFFICULTY).
#[evm_opcode(PREVRANDAO)]
pub unsafe fn prevrandao() -> U256 {
    unsafe { core::hint::unreachable_unchecked() }
}

/// GASLIMIT — gas limit of the current block.
#[evm_opcode(GASLIMIT)]
pub unsafe fn gaslimit() -> U256 {
    unsafe { core::hint::unreachable_unchecked() }
}

/// CHAINID — EIP-155 chain ID.
#[evm_opcode(CHAINID)]
pub unsafe fn chainid() -> U256 {
    unsafe { core::hint::unreachable_unchecked() }
}

/// BASEFEE — EIP-1559 base fee of the current block, in wei.
#[evm_opcode(BASEFEE)]
pub unsafe fn basefee() -> U256 {
    unsafe { core::hint::unreachable_unchecked() }
}

/// BLOBHASH — EIP-4844 versioned hash of the blob at `index` in the transaction.
#[evm_opcode(BLOBHASH)]
pub unsafe fn blobhash(index: U256) -> U256 {
    unsafe { core::hint::unreachable_unchecked() }
}

/// BLOBBASEFEE — EIP-4844 current blob base fee, in wei.
#[evm_opcode(BLOBBASEFEE)]
pub unsafe fn blobbasefee() -> U256 {
    unsafe { core::hint::unreachable_unchecked() }
}
