//! Type / ABI mapping between Rust types and EVM's 256-bit word model.
//!
//! # EVM type model
//!
//! The EVM has exactly one primitive type: a 256-bit unsigned integer (U256).
//! Everything else — booleans, addresses, byte arrays, signed integers, etc. —
//! is a convention layered on top.
//!
//! # Layout of locals in memory
//!
//! During compilation each MIR local is assigned a *slot*: a 32-byte-aligned
//! region of EVM memory.  Slot `n` starts at byte offset `n * 32`.
//!
//! ```text
//! offset 0x00..0x20   local _0  (return place)
//! offset 0x20..0x40   local _1
//! offset 0x40..0x60   local _2
//! ...
//! ```
//!
//! Larger types (structs, arrays) will need multi-slot layouts; that is not
//! yet implemented.

use rustc_abi::Size;
use rustc_middle::mir::{Body, Local};
use rustc_middle::ty::{Ty, TyCtxt, TyKind};

/// The number of bytes in one EVM word / stack slot.
pub const WORD_BYTES: u32 = 32;

// ── EvmLayout ────────────────────────────────────────────────────────────────

/// Memory layout for all locals in a single MIR function body.
///
/// Each local gets exactly one 32-byte slot (sufficient for scalar types).
/// Aggregate types that don't fit in 32 bytes are not yet supported.
pub struct EvmLayout {
    /// Slot index for each MIR local.  `slots[local.index()] = slot_index`.
    slots: Vec<u32>,
}

impl EvmLayout {
    /// Compute the layout for all locals in `body`.
    pub fn for_body<'tcx>(tcx: TyCtxt<'tcx>, body: &Body<'tcx>) -> Self {
        let slots: Vec<u32> = (0..body.local_decls.len() as u32).collect();
        EvmLayout { slots }
    }

    /// Return the byte offset of `local`'s slot in EVM memory.
    pub fn slot_of(&self, local: Local) -> u32 {
        self.slots[local.index()] * WORD_BYTES
    }

    /// Total memory required for all locals (bytes).
    pub fn total_bytes(&self) -> u32 {
        self.slots.len() as u32 * WORD_BYTES
    }
}

// ── EvmTy ─────────────────────────────────────────────────────────────────────

/// The EVM representation of a Rust type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EvmTy {
    /// A single 256-bit stack word.  Covers bool, all integer types ≤ 256 bits,
    /// raw pointers, and function pointers.
    Word,
    /// A multi-word value stored in memory.
    Memory { size_bytes: u64 },
    /// The never type — no runtime representation.
    Diverging,
    /// Unit `()` — no stack slots consumed or produced.
    Unit,
}

impl EvmTy {
    /// Map a Rust type to its EVM representation.
    pub fn from_rust_ty<'tcx>(tcx: TyCtxt<'tcx>, ty: Ty<'tcx>) -> Self {
        match ty.kind() {
            // Scalars that fit in a single 256-bit word.
            TyKind::Bool
            | TyKind::Char
            | TyKind::Int(_)
            | TyKind::Uint(_)
            | TyKind::Float(_)
            | TyKind::RawPtr(_, _)
            | TyKind::Ref(_, _, _)
            | TyKind::FnPtr(_) => EvmTy::Word,

            // Zero-sized types.
            TyKind::Tuple(tys) if tys.is_empty() => EvmTy::Unit,
            TyKind::Never => EvmTy::Diverging,

            // Function definitions don't occupy a stack slot.
            TyKind::FnDef(_, _) => EvmTy::Unit,

            // Aggregates — stored in memory (multi-slot).
            // TODO: compute the real size via tcx.layout_of.
            TyKind::Adt(_, _) | TyKind::Array(_, _) | TyKind::Tuple(_) | TyKind::Slice(_) => {
                EvmTy::Memory { size_bytes: 32 }
            }

            _ => todo!("EvmTy::from_rust_ty: unsupported type {:?}", ty),
        }
    }

    /// Does this type produce a value on the EVM stack?
    pub fn is_stack_value(&self) -> bool {
        matches!(self, EvmTy::Word)
    }
}
