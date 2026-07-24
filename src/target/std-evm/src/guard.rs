// guard.rs — the callvalue guard `#[contract]` calls on non-`#[payable]` methods.

/// Reject a non-payable call that arrived with ETH attached.
///
/// `#[contract]` calls this at the top of every method's `Method::call`
/// unless the method is tagged `#[payable]`. Solidity's default is
/// non-payable — `solc` injects exactly this guard into every function that
/// doesn't opt out — so `#[payable]` *suppresses* this call rather than
/// adding behaviour; see `design/std-evm-macros-spec.md`.
///
/// A no-op until `evm-sys`'s `CALLVALUE`/`REVERT` bindings are backed by
/// real codegen. The call site in generated code doesn't change once they
/// are — only this function's body does.
pub fn guard_not_payable() {
    // TODO: revert if evm_sys::context::callvalue() != U256::ZERO, once
    // opcode codegen makes that call meaningful rather than unreachable.
}
