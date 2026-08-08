//! Value types, function signatures, and the width repair that keeps them
//! honest.
//!
//! # The representation invariant
//!
//! Every EVM stack slot is 256 bits, but Rust's integers are not. This IR
//! reconciles the two with one rule, and everything in this module exists to
//! state or maintain it:
//!
//! > A well-formed value of type `u<w>` has bits `w..256` **zero**. A value of
//! > type `i<w>` has bits `w..256` **equal to bit `w-1`** — it is sign-extended
//! > across the whole word.
//!
//! Design doc §14 describes lowering as three steps, the second of which is
//! "repair to Rust width — *this is the step you'll forget and it silently
//! miscompiles*". Making the repair a [`Repair`] field that the builder fills
//! in from the operation and its result type is how that step stops being
//! forgettable: there is no way to spell an unrepaired narrow value.
//!
//! The invariant is not just insurance, it pays for itself:
//!
//! - **Signed comparison is free.** §14 notes signed compares need their
//!   operands sign-extended first. Under the invariant they already are, so
//!   `SLT` on two `i32`s is one opcode with no preparation.
//! - **Widening casts emit nothing.** `u8 → u32`, `i8 → i64` and `u8 → i32`
//!   are all [`Repair::None`], so the casts that dominate real code are free.
//! - **`NOT` on a signed value needs no mask**, because flipping a
//!   sign-extended word leaves a sign-extended word. §14 warns that EVM `NOT`
//!   flips all 256 bits; that only needs repairing for unsigned types.
//! - **A future known-bits lattice starts pre-seeded** — `u8` means "bits
//!   8..256 known zero" before any analysis runs.

use std::fmt;

/// Width of an EVM stack slot. A type this wide needs no repair, because there
/// are no bits above it to disagree with.
const WORD_BITS: u16 = 256;

/// An integer type: a width and a signedness.
///
/// Signedness lives on the *type*, not on the operation — a departure from
/// LLVM and Cranelift, and the thing that makes the representation invariant
/// above statable at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct IntTy {
    /// `1..=256`. Checked by `verify`, not by construction, so that building
    /// IR stays infallible and every diagnostic arrives from one place.
    pub bits: u16,
    pub signed: bool,
}

/// The type of a value.
///
/// Deliberately has no aggregates. Structs, arrays and enums are a [`Ptr`] and
/// an `Alloca`: rustc computes their layouts and the shim keeps them on the
/// side, so the IR only ever needs "what fits in a machine word, and how wide".
///
/// [`Ptr`]: Type::Ptr
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Type {
    Int(IntTy),
    /// A byte offset into EVM memory.
    ///
    /// Behaves exactly as a `u32` — the target spec sets `target-pointer-width`
    /// to 32 — but is a distinct type so that `verify` can reject arithmetic
    /// that treats an address as a number by accident.
    Ptr,
}

impl Type {
    pub const BOOL: Type = Type::uint(1);
    pub const U8: Type = Type::uint(8);
    pub const I8: Type = Type::sint(8);
    pub const U16: Type = Type::uint(16);
    pub const I16: Type = Type::sint(16);
    pub const U32: Type = Type::uint(32);
    pub const I32: Type = Type::sint(32);
    pub const U64: Type = Type::uint(64);
    pub const I64: Type = Type::sint(64);
    pub const U128: Type = Type::uint(128);
    pub const I128: Type = Type::sint(128);
    pub const U256: Type = Type::uint(256);
    pub const I256: Type = Type::sint(256);
    pub const PTR: Type = Type::Ptr;

    pub const fn uint(bits: u16) -> Type {
        Type::Int(IntTy { bits, signed: false })
    }

    pub const fn sint(bits: u16) -> Type {
        Type::Int(IntTy { bits, signed: true })
    }

    /// The integer type this behaves as. [`Ptr`](Type::Ptr) is a `u32`.
    pub const fn as_int(self) -> IntTy {
        match self {
            Type::Int(int) => int,
            Type::Ptr => IntTy {
                bits: 32,
                signed: false,
            },
        }
    }

    pub const fn bits(self) -> u16 {
        self.as_int().bits
    }

    pub const fn is_signed(self) -> bool {
        self.as_int().signed
    }

    /// Whether the width is one the EVM can hold. `verify` reports the rest.
    pub const fn is_well_formed(self) -> bool {
        let bits = self.bits();
        bits >= 1 && bits <= WORD_BITS
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Type::Ptr => write!(f, "ptr"),
            Type::Int(IntTy { bits, signed }) => {
                write!(f, "{}{bits}", if *signed { 'i' } else { 'u' })
            }
        }
    }
}

/// How the raw 256-bit result of an operation is brought back into its result
/// type's range, restoring the representation invariant.
///
/// The builder sets this from the operation and the result type — a frontend
/// never writes one, which is what makes §14's step 2 unforgettable. A future
/// `evm-opt` weakens it to [`None`](Repair::None) when range analysis proves
/// the raw result is already in range; that is mask elision, and because it is
/// a field write rather than a graph edit it shows up in a dump diff as a
/// single token.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Repair {
    /// Nothing to do: a 256-bit result, a comparison, a widening convert — or
    /// a mask an optimizer has proved redundant.
    None,
    /// Clear bits `bits..256`. `AND ((1 << bits) - 1)`.
    Mask { bits: u16 },
    /// Replicate bit `bits-1` across `bits..256`. `SIGNEXTEND` when the width
    /// is byte-aligned, a `SHL`/`SAR` pair otherwise — which encoding to use is
    /// the stackifier's decision, not the IR's.
    Sext { bits: u16 },
}

impl Repair {
    /// A mask, normalised: repairing to the full word width is a no-op, and
    /// emitting `AND 0xff..ff` for it would be three wasted bytes.
    pub const fn mask(bits: u16) -> Repair {
        if bits >= WORD_BITS {
            Repair::None
        } else {
            Repair::Mask { bits }
        }
    }

    /// A sign-extension, normalised the same way as [`mask`](Repair::mask).
    pub const fn sext(bits: u16) -> Repair {
        if bits >= WORD_BITS {
            Repair::None
        } else {
            Repair::Sext { bits }
        }
    }

    /// The repair that turns a well-formed value of `from` into a well-formed
    /// value of `to`.
    ///
    /// This is where casts stop being frightening: one total function covering
    /// truncation, zero-extension, sign-extension and signedness
    /// reinterpretation, derived from nothing but the invariant. Both
    /// directions are read off the destination's requirement and whether the
    /// source already happens to satisfy it.
    pub const fn for_convert(from: Type, to: Type) -> Repair {
        let (from, to) = (from.as_int(), to.as_int());

        if to.signed {
            // The destination must be sign-extended from `to.bits`. A signed
            // source of no greater width already is. An unsigned source is too,
            // but only if it is *strictly* narrower — at equal width its top
            // bit is a value bit, and reinterpreting it as a sign bit is
            // exactly the case that needs work (`u8 -> i8` on `0xff`).
            let already = if from.signed {
                from.bits <= to.bits
            } else {
                from.bits < to.bits
            };
            if already { Repair::None } else { Repair::sext(to.bits) }
        } else {
            // The destination must be zero above `to.bits`. Only an unsigned
            // source of no greater width already is; a signed one may carry
            // sign bits up there.
            if !from.signed && from.bits <= to.bits {
                Repair::None
            } else {
                Repair::mask(to.bits)
            }
        }
    }
}

impl fmt::Display for Repair {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Repair::None => write!(f, "norepair"),
            Repair::Mask { bits } => write!(f, "mask{bits}"),
            Repair::Sext { bits } => write!(f, "sext{bits}"),
        }
    }
}

/// A function's parameter and return types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signature {
    pub params: Vec<Type>,
    pub returns: Vec<Type>,
}

impl Signature {
    pub fn new(params: impl IntoIterator<Item = Type>, returns: impl IntoIterator<Item = Type>) -> Self {
        Signature {
            params: params.into_iter().collect(),
            returns: returns.into_iter().collect(),
        }
    }
}

impl fmt::Display for Signature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "(")?;
        for (i, ty) in self.params.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{ty}")?;
        }
        write!(f, ")")?;

        match self.returns.as_slice() {
            [] => Ok(()),
            [one] => write!(f, " -> {one}"),
            many => {
                write!(f, " -> (")?;
                for (i, ty) in many.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{ty}")?;
                }
                write!(f, ")")
            }
        }
    }
}

#[cfg(test)]
mod tests;
