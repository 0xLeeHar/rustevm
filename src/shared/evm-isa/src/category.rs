/// Coarse classification of an opcode's role, following the Yellow Paper's
/// own opcode groupings (Environmental Information, Block Information,
/// System operations, ...) mapped onto a fixed, small set of buckets that
/// `evm-opt`'s state analysis and the scheduler's special-casing key off of.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OpCategory {
    Arithmetic,
    Comparison,
    Bitwise,
    Storage,
    Memory,
    Context,
    Block,
    Control,
    StackManip,
    Call,
    Log,
    Terminating,
}
