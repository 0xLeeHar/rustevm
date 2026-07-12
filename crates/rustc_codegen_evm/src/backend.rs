//! [`EvmCodegenBackend`] — the [`CodegenBackend`] implementation.
//!
//! This is the main entry point that rustc calls during compilation.  The
//! three key phases are:
//!
//! 1. **`codegen_crate`** — given the full `TyCtxt`, lower MIR to EVM bytecode
//!    and return an opaque handle to the in-progress compilation.
//! 2. **`join_codegen`** — wait for any parallel codegen work to finish and
//!    collect the final [`CodegenResults`].
//! 3. **`link`** — write the compiled bytecode to the output path.

use std::any::Any;

use rustc_codegen_ssa::back::metadata::MetadataLoaderDyn;
use rustc_codegen_ssa::traits::CodegenBackend;
use rustc_codegen_ssa::CodegenResults;
use rustc_data_structures::fx::FxIndexMap;
use rustc_metadata::EncodedMetadata;
use rustc_middle::dep_graph::{WorkProduct, WorkProductId};
use rustc_middle::ty::TyCtxt;
use rustc_session::Session;
use rustc_session::output::OutputFilenames;

use crate::context::EvmContext;
use crate::emit;

// ── Backend struct ────────────────────────────────────────────────────────────

/// The EVM codegen backend.
///
/// Zero-sized — all mutable state lives in the per-crate [`EvmContext`] that
/// is created inside [`codegen_crate`](Self::codegen_crate).
pub struct EvmCodegenBackend;

impl EvmCodegenBackend {
    pub fn new() -> Self {
        EvmCodegenBackend
    }
}

// ── CodegenBackend impl ───────────────────────────────────────────────────────

impl CodegenBackend for EvmCodegenBackend {
    // Translated diagnostic resources — none yet.
    fn locale_resource(&self) -> &'static str {
        ""
    }

    fn metadata_loader(&self) -> Box<MetadataLoaderDyn> {
        // Use rustc_codegen_ssa's built-in ELF/macho metadata reader.
        // The EVM backend doesn't need a custom one.
        Box::new(rustc_codegen_ssa::back::metadata::DefaultMetadataLoader)
    }

    fn provide(&self, providers: &mut rustc_middle::query::Providers) {
        // Register EVM-specific query overrides here.
        // For example, if we want to change how `is_reachable_non_generic` is
        // computed.  Currently no overrides are needed.
        let _ = providers;
    }

    fn provide_extern(&self, providers: &mut rustc_middle::query::ExternProviders) {
        let _ = providers;
    }

    /// Phase 1: lower MIR → EVM bytecode.
    ///
    /// Called once per crate, on the main thread, with the fully-populated
    /// `TyCtxt`.  Returns an opaque `Box<dyn Any>` that is passed to
    /// [`join_codegen`](Self::join_codegen) after any parallel work finishes.
    fn codegen_crate(
        &self,
        tcx: TyCtxt<'_>,
        metadata: EncodedMetadata,
        need_metadata_module: bool,
    ) -> Box<dyn Any> {
        let mut ctx = EvmContext::new(tcx, metadata, need_metadata_module);
        let ongoing = ctx.compile();
        Box::new(ongoing)
    }

    /// Phase 2: collect parallel codegen work.
    ///
    /// The EVM backend currently does all codegen on the main thread inside
    /// `codegen_crate`, so this method just unpacks the result.
    fn join_codegen(
        &self,
        ongoing_codegen: Box<dyn Any>,
        _sess: &Session,
        _outputs: &OutputFilenames,
    ) -> (CodegenResults, FxIndexMap<WorkProductId, WorkProduct>) {
        let ongoing = *ongoing_codegen
            .downcast::<crate::context::OngoingCodegen>()
            .expect("wrong type returned from codegen_crate");
        (ongoing.results, ongoing.work_products)
    }

    /// Phase 3: write bytecode to disk.
    ///
    /// Called after `join_codegen` with the final [`CodegenResults`].
    fn link(
        &self,
        sess: &Session,
        codegen_results: CodegenResults,
        outputs: &OutputFilenames,
    ) -> Result<(), rustc_errors::ErrorGuaranteed> {
        emit::link(sess, codegen_results, outputs)
    }
}
