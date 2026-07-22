/// The gas-cost *rule shape* that applies to an opcode.
///
/// This records which rule applies and its fixed parameters — it does not
/// compute a final cost. Composing that (memory expansion, refunds, EIP-2929
/// warm/cold bookkeeping across a whole call frame) is `evm-opt`'s job; this
/// table only needs to expose enough to prioritize optimizations (e.g.
/// eliminating an `SSTORE` saves far more than eliding an `ADD`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GasClass {
    /// The canonical EVM zero-gas tier (`STOP`, `RETURN`, `REVERT`).
    Zero,
    /// A constant cost tier, e.g. `ADD` = 3, `MUL` = 5.
    Fixed(u16),
    /// EIP-2929 accessed-set cost: expensive on first touch, cheap on repeat
    /// access within the same transaction.
    ColdWarm { cold: u16, warm: u16 },
    /// Cost scales linearly with the number of 32-byte words touched (the
    /// per-word rate only — any flat base component is not tracked here).
    PerWord(u16),
    /// `EXP`'s dynamic component: gas per significant byte of the exponent.
    PerByteOfExponent(u16),
    /// State write with EIP-2200/3529 state-dependent cost and refunds.
    StorageWrite,
    /// Pure memory-expansion cost; the quadratic+linear formula itself lives
    /// in `evm-opt`.
    MemoryExpansion,
    /// A multi-component formula this table doesn't decompose (the `CALL`
    /// family, `CREATE`/`CREATE2`, `SELFDESTRUCT`, `LOG0`-`LOG4`) — `evm-opt`
    /// owns the full rule.
    Complex,
    /// Consumes all remaining gas (`INVALID`).
    All,
}
