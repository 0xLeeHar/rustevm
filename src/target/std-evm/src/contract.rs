// contract.rs — constructor wiring `#[constructor]` targets.

/// The initcode-side entry point. `#[constructor]`'s tagged method becomes
/// this trait's `deploy` impl, which the codegen backend places in the
/// initcode section (separate from the runtime dispatcher).
pub trait Contract {
    fn deploy(calldata: &[u8]);
}
