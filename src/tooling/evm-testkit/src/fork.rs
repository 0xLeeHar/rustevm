//! `evm-isa`'s [`Fork`] to revm's [`SpecId`].
//!
//! One function, deliberately exhaustive: adding a variant to `Fork` should
//! fail to compile here rather than silently execute against the wrong gas
//! schedule.

use evm_isa::Fork;
use revm::primitives::hardfork::SpecId;

/// The revm spec that gates the same opcodes and gas rules as `fork`.
pub(crate) fn spec_id(fork: Fork) -> SpecId {
    match fork {
        Fork::Frontier => SpecId::FRONTIER,
        Fork::Homestead => SpecId::HOMESTEAD,
        Fork::Byzantium => SpecId::BYZANTIUM,
        // revm has no CONSTANTINOPLE: it never activated on mainnet, having
        // been superseded by Petersburg — Constantinople minus EIP-1283.
        // Petersburg is the closest executable spec.
        Fork::Constantinople => SpecId::PETERSBURG,
        Fork::Istanbul => SpecId::ISTANBUL,
        Fork::Berlin => SpecId::BERLIN,
        Fork::London => SpecId::LONDON,
        Fork::Shanghai => SpecId::SHANGHAI,
        Fork::Cancun => SpecId::CANCUN,
        Fork::Prague => SpecId::PRAGUE,
    }
}
