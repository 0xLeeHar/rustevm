// storage.rs — a handle proving view/mutate access at the type level.

/// A storage handle. `&Storage` on a method signature is *provably* view —
/// enforced by the borrow checker, not an attribute a developer can get
/// wrong; `&mut Storage` is mutating. See `design/std-evm-macros-spec.md`'s
/// "state mutability" section: this is why `#[view]`/`#[pure]` attributes
/// were deliberately omitted.
///
/// Zero-sized placeholder for now. Real `SLOAD`/`SSTORE` wiring is blocked
/// on `evm-sys`'s opcode bindings becoming more than
/// `unreachable_unchecked()` stubs (the `#[evm_opcode]` macro and
/// `rustc_codegen_evm`'s real codegen aren't implemented yet) — this exists
/// so `&Storage`/`&mut Storage` method parameters type-check today.
pub struct Storage(());

impl Storage {
    pub fn new() -> Storage {
        Storage(())
    }
}

impl Default for Storage {
    fn default() -> Self {
        Storage::new()
    }
}
