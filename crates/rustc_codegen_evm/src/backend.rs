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
use rustc_codegen_ssa::{CompiledModule, CompiledModules, CrateInfo, ModuleKind};
use rustc_codegen_ssa::traits::CodegenBackend;
use rustc_middle::dep_graph::WorkProductMap;
use rustc_middle::ty::TyCtxt;
use rustc_session::config::{OutputFilenames, OutputType};
use rustc_session::Session;

pub struct EvmCodegenBackend;

impl EvmCodegenBackend {
    pub fn new() -> Self {
        EvmCodegenBackend
    }
}

/// One codegen unit lowered to EVM bytecode, still in memory.
pub struct EvmModule {
    pub name: String,
    pub bytecode: Vec<u8>,
}

/// The opaque handle rustc passes from `codegen_crate` to `join_codegen`.
///
/// rustc only ever sees this as a `Box<dyn Any>`; `join_codegen` downcasts it
/// back to this exact type, so the two methods must agree on it.
pub struct OngoingCodegen {
    pub modules: Vec<EvmModule>,
}

impl CodegenBackend for EvmCodegenBackend {
    fn name(&self) -> &'static str {
        "evm"
    }

    fn target_cpu(&self, sess: &Session) -> String {
        sess.opts.cg.target_cpu.clone().unwrap_or_else(|| "generic".to_string())
    }

    fn codegen_crate<'tcx>(&self, tcx: TyCtxt<'tcx>) -> Box<dyn Any> {
        let cgus = tcx.collect_and_partition_mono_items(()).codegen_units;

        let modules = cgus
            .iter()
            .map(|cgu| EvmModule {
                name: cgu.name().to_string(),
                // TODO: lower `cgu.items()` from MIR to EVM bytecode.
                bytecode: Vec::new(),
            })
            .collect();

        Box::new(OngoingCodegen { modules })
    }

    fn join_codegen(&self, ongoing_codegen: Box<dyn Any>, sess: &Session, outputs: &OutputFilenames, _crate_info: &CrateInfo) -> (CompiledModules, WorkProductMap) {
        let ongoing = *ongoing_codegen
            .downcast::<OngoingCodegen>()
            .expect("`join_codegen` was handed a box that `codegen_crate` did not produce");

        let modules = ongoing
            .modules
            .into_iter()
            .map(|module| {
                let path = outputs.temp_path_for_cgu(OutputType::Object, &module.name);
                if let Err(err) = std::fs::write(&path, &module.bytecode) {
                    sess.dcx().fatal(format!("failed to write {}: {err}", path.display()));
                }

                CompiledModule {
                    name: module.name,
                    kind: ModuleKind::Regular,
                    object: Some(path),
                    global_asm_object: None,
                    dwarf_object: None,
                    bytecode: None,
                    assembly: None,
                    llvm_ir: None,
                    links_from_incr_cache: Vec::new(),
                }
            })
            .collect();

        (CompiledModules { modules, allocator_module: None }, WorkProductMap::default())
    }
}
