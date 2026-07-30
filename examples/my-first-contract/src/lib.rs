#![no_std]

use std_evm::{Address, Storage, TransientStorage, U256, contract, storage, transient};

pub struct MyFirstContract;

#[storage]
pub struct ContractStorage {
    total_supply: U256,
    balances: Mapping<Address, U256>,
}

/// Transient state — cleared at the end of the *transaction*, not the call.
/// Slots number from 0 in their own address space, unrelated to the
/// persistent slots above.
#[transient]
pub struct TempStorage {
    withdrawing: bool,
    pending: Mapping<Address, U256>,
}

#[contract]
impl MyFirstContract {
    #[constructor]
    pub fn init(s: &mut Storage<ContractStorage>, initial_supply: U256) {
        s.set_total_supply(initial_supply);
    }

    #[payable]
    pub fn deposit(s: &mut Storage<ContractStorage>, to: Address, amount: U256) {
        s.balances_mut().set(to, amount);
    }

    /// Takes both handles — the dispatcher constructs and threads each one.
    ///
    /// `withdrawing_guard()` sets the transient flag and clears it when the
    /// guard drops, so an early return can't leave it set for the rest of the
    /// transaction.
    pub fn withdraw(
        s: &mut Storage<ContractStorage>,
        t: &mut TransientStorage<TempStorage>,
        to: Address,
        amount: U256,
    ) {
        let _guard = t.withdrawing_guard();

        // Park the in-flight amount where a reentrant call can read it —
        // ~100 gas, against ~20,000 for a persistent slot.
        t.pending_mut().set(to, amount);

        // Not real accounting: `U256` has no arithmetic yet, so this writes
        // the balance rather than debiting it.
        s.balances_mut().set(to, amount);

        // Transient slots clear at transaction end, not here — so the clear
        // is ours to do. The `_guard` above covers only `withdrawing`.
        t.pending_mut().set(to, U256::ZERO);
    }

    pub fn total_supply(s: &Storage<ContractStorage>) -> U256 {
        s.total_supply()
    }

    pub fn balance_of(s: &Storage<ContractStorage>, who: Address) -> U256 {
        s.balances().get(who)
    }
}
