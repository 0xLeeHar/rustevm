/// Ethereum hardforks, in chronological order.
///
/// `Ord` is load-bearing. It makes gating a comparison rather than a match
/// arm per fork: `if spec.min_fork <= target_fork { /* available */ }`.
/// Declaration order defines the ordering — keep variants chronological.
/// Reordering them silently breaks every gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Fork {
    Frontier,
    Homestead,
    Byzantium,
    Constantinople,
    Istanbul,
    Berlin,
    London,
    Shanghai,
    Cancun,
    Prague,
}
