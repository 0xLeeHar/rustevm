//! Handles into a function's arenas.
//!
//! Every entity is a `u32` index rather than a reference. Two reasons, both
//! load-bearing:
//!
//! - The rustc shim maps `BackendTypes`' associated types onto these (design
//!   doc §13), which needs `Copy` types carrying no lifetime.
//! - A pass rewriting one instruction while reading another is routine, and
//!   indices make that a borrow of the arena rather than two borrows of the
//!   same graph.
//!
//! The `Display` impls are what the `--emit=evm-ir` dump prints, so they are
//! part of the format, not a debugging convenience.

use std::fmt;

/// Declares an index newtype. `$prefix` is how it renders in a dump.
macro_rules! entity {
    ($(#[$doc:meta])* $name:ident, $prefix:literal) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(u32);

        impl $name {
            /// Construct from an arena position.
            ///
            /// `pub(crate)` on purpose: an entity is only meaningful as an
            /// index into the arena that produced it, so handing them out to
            /// callers to mint would let a `Value` from one function address a
            /// slot in another.
            pub(crate) fn new(index: usize) -> Self {
                $name(index as u32)
            }

            /// Position in the arena that defined it.
            pub fn index(self) -> usize {
                self.0 as usize
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, concat!($prefix, "{}"), self.0)
            }
        }
    };
}

entity!(
    /// A single definition — an instruction result or a block parameter.
    ///
    /// The naive stackifier derives a value's frame slot directly from its
    /// index, so the arena *is* the slot map (see `stackify::frame`).
    Value,
    "v"
);

entity!(
    /// One instruction in a function's arena.
    ///
    /// Terminators are not instructions and have no `Inst` — they are a field
    /// on the block.
    Inst,
    "i"
);

entity!(
    /// A basic block: parameters, straight-line instructions, one terminator.
    Block,
    "block"
);

entity!(
    /// A function within a module. May be declared before it is defined, so a
    /// call site can name a callee whose body does not exist yet.
    Func,
    "@f"
);

#[cfg(test)]
mod tests;
