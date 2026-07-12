//! Per-crate compilation context.
//!
//! [`EvmContext`] holds everything needed to lower a single Rust crate from
//! MIR to EVM bytecode.  It is created by [`EvmCodegenBackend::codegen_crate`]
//! and consumed by [`EvmContext::compile`], which returns an [`OngoingCodegen`]
//! that is later passed to `join_codegen` / `link`.
//!
//! # Lowering strategy
//!
//! The EVM is a 256-bit stack machine.  Rust's MIR operates on typed *places*
//! (memory locations) and *operands* (values).  The rough mapping is:
//!
//! | MIR concept       | EVM equivalent                        |
//! |-------------------|---------------------------------------|
//! | Local             | Memory slot at a fixed offset         |
//! | `Operand::Move`   | `MLOAD` the local's slot              |
//! | `Operand::Copy`   | `MLOAD` the local's slot              |
//! | `Operand::Const`  | `PUSH<N>` the constant value          |
//! | `BinOp::Add`      | `ADD` opcode                          |
//! | evm-sys intrinsic | Corresponding opcode (see [`intrinsics`]) |
//! | `Return`          | `RETURN` / `STOP`                    |
//!
//! Layout of a compiled contract in memory at runtime:
//! ```text
//! [ locals area ][ scratch ]
//! ^
//! 0x00
//! ```
//!
//! Each local occupies one 32-byte (256-bit) slot.  Slot address of local `n`
//! is `n * 32`.

use rustc_codegen_ssa::CodegenResults;
use rustc_codegen_ssa::back::write::CodegenContext;
use rustc_data_structures::fx::FxIndexMap;
use rustc_metadata::EncodedMetadata;
use rustc_middle::dep_graph::{WorkProduct, WorkProductId};
use rustc_middle::mir::mono::MonoItem;
use rustc_middle::ty::{Instance, TyCtxt};

use crate::abi::EvmLayout;
use crate::emit::Emitter;
use crate::intrinsics;

// ── OngoingCodegen ────────────────────────────────────────────────────────────

/// Opaque handle returned by [`EvmContext::compile`].
///
/// Passed through `join_codegen` unchanged; the actual linking happens in
/// [`emit::link`].
pub struct OngoingCodegen {
    pub results: CodegenResults,
    pub work_products: FxIndexMap<WorkProductId, WorkProduct>,
}

// ── EvmContext ────────────────────────────────────────────────────────────────

/// Per-crate compilation context.
pub struct EvmContext<'tcx> {
    tcx: TyCtxt<'tcx>,
    metadata: EncodedMetadata,
    need_metadata_module: bool,
}

impl<'tcx> EvmContext<'tcx> {
    pub fn new(
        tcx: TyCtxt<'tcx>,
        metadata: EncodedMetadata,
        need_metadata_module: bool,
    ) -> Self {
        EvmContext { tcx, metadata, need_metadata_module }
    }

    /// Lower all reachable monomorphized items to EVM bytecode.
    pub fn compile(self) -> OngoingCodegen {
        let tcx = self.tcx;

        // Collect the set of mono items that need code generation.
        // `collect_and_partition_mono_items` returns all reachable instances
        // after monomorphisation together with their partition (codegen unit)
        // assignment.
        let (items, _cgus) = tcx.collect_and_partition_mono_items(());

        for item in items.iter() {
            match item {
                MonoItem::Fn(instance) => self.codegen_fn(*instance),
                MonoItem::Static(def_id) => self.codegen_static(*def_id),
                MonoItem::GlobalAsm(item_id) => self.codegen_global_asm(*item_id),
            }
        }

        // TODO: collect the emitted bytecode from each function and assemble
        //       the final contract (deploy code + runtime code).
        //       For now, return an empty result so the pipeline can be exercised.
        OngoingCodegen {
            results: build_codegen_results(tcx, self.metadata, self.need_metadata_module),
            work_products: FxIndexMap::default(),
        }
    }

    // ── per-item lowering ────────────────────────────────────────────────────

    /// Lower a monomorphized function to a sequence of EVM opcodes.
    fn codegen_fn(&self, instance: Instance<'tcx>) {
        let tcx = self.tcx;

        // Some instances have no MIR body (e.g. intrinsics, foreign functions).
        // Check whether this is an evm-sys intrinsic first.
        if let Some(opcode) = intrinsics::recognize(tcx, instance) {
            // evm-sys intrinsics are inlined at call sites; no standalone
            // function body is emitted for them.
            let _ = opcode;
            return;
        }

        // Retrieve the optimised MIR body.
        let mir = tcx.instance_mir(instance.def);
        let layout = EvmLayout::for_body(tcx, mir);
        let mut emitter = Emitter::new();

        self.lower_body(instance, mir, &layout, &mut emitter);

        // TODO: register the resulting bytecode with the module / linker.
        let _ = emitter;
    }

    /// Lower a single MIR body.
    fn lower_body(
        &self,
        _instance: Instance<'tcx>,
        mir: &rustc_middle::mir::Body<'tcx>,
        layout: &EvmLayout,
        emitter: &mut Emitter,
    ) {
        use rustc_middle::mir::{BasicBlockData, Rvalue, StatementKind, TerminatorKind};

        for (bb_idx, bb) in mir.basic_blocks.iter_enumerated() {
            self.lower_basic_block(bb, layout, emitter);
        }
    }

    fn lower_basic_block(
        &self,
        bb: &rustc_middle::mir::BasicBlockData<'tcx>,
        layout: &EvmLayout,
        emitter: &mut Emitter,
    ) {
        use rustc_middle::mir::StatementKind;

        for stmt in &bb.statements {
            match &stmt.kind {
                StatementKind::Assign(box (place, rvalue)) => {
                    self.lower_assign(place, rvalue, layout, emitter);
                }
                // StorageLive / StorageDead are hints; we manage memory
                // statically, so ignore them.
                StatementKind::StorageLive(_) | StatementKind::StorageDead(_) => {}
                // Nops and other non-code-generating statements.
                _ => {}
            }
        }

        if let Some(terminator) = &bb.terminator {
            self.lower_terminator(terminator, layout, emitter);
        }
    }

    fn lower_assign(
        &self,
        place: &rustc_middle::mir::Place<'tcx>,
        rvalue: &rustc_middle::mir::Rvalue<'tcx>,
        layout: &EvmLayout,
        emitter: &mut Emitter,
    ) {
        use rustc_middle::mir::{BinOp, Rvalue};

        // Push the computed value onto the EVM stack…
        match rvalue {
            Rvalue::Use(operand) => {
                self.lower_operand(operand, layout, emitter);
            }
            Rvalue::BinaryOp(op, box (lhs, rhs)) => {
                // Push operands left-to-right; EVM pops right-to-left (top is
                // the *last* pushed), so push rhs first.
                self.lower_operand(rhs, layout, emitter);
                self.lower_operand(lhs, layout, emitter);
                match op {
                    BinOp::Add => emitter.op(crate::emit::Opcode::ADD),
                    BinOp::Sub => emitter.op(crate::emit::Opcode::SUB),
                    BinOp::Mul => emitter.op(crate::emit::Opcode::MUL),
                    BinOp::Div => emitter.op(crate::emit::Opcode::DIV),
                    BinOp::Rem => emitter.op(crate::emit::Opcode::MOD),
                    BinOp::BitAnd => emitter.op(crate::emit::Opcode::AND),
                    BinOp::BitOr => emitter.op(crate::emit::Opcode::OR),
                    BinOp::BitXor => emitter.op(crate::emit::Opcode::XOR),
                    BinOp::Shl => emitter.op(crate::emit::Opcode::SHL),
                    BinOp::Shr => emitter.op(crate::emit::Opcode::SHR),
                    BinOp::Eq => emitter.op(crate::emit::Opcode::EQ),
                    BinOp::Lt => emitter.op(crate::emit::Opcode::LT),
                    BinOp::Gt => emitter.op(crate::emit::Opcode::GT),
                    _ => todo!("BinOp::{op:?} lowering"),
                }
            }
            Rvalue::UnaryOp(op, operand) => {
                use rustc_middle::mir::UnOp;
                self.lower_operand(operand, layout, emitter);
                match op {
                    UnOp::Not => emitter.op(crate::emit::Opcode::NOT),
                    UnOp::Neg => {
                        // EVM has no NEG; emulate as 0 - x.
                        self.lower_operand(operand, layout, emitter);
                        emitter.push_u256(0u8.into());
                        // stack: [x, 0]
                        emitter.op(crate::emit::Opcode::SUB);
                    }
                    _ => todo!("UnOp::{op:?} lowering"),
                }
            }
            _ => todo!("Rvalue lowering: {rvalue:?}"),
        }

        // …then store it into `place`.
        let slot = layout.slot_of(place.local);
        emitter.push_u256((slot as u64).into());
        emitter.op(crate::emit::Opcode::MSTORE);
    }

    fn lower_operand(
        &self,
        operand: &rustc_middle::mir::Operand<'tcx>,
        layout: &EvmLayout,
        emitter: &mut Emitter,
    ) {
        use rustc_middle::mir::Operand;

        match operand {
            Operand::Copy(place) | Operand::Move(place) => {
                // Load from the local's memory slot.
                let slot = layout.slot_of(place.local);
                emitter.push_u256((slot as u64).into());
                emitter.op(crate::emit::Opcode::MLOAD);
            }
            Operand::Constant(box constant) => {
                self.lower_constant(constant, emitter);
            }
        }
    }

    fn lower_constant(
        &self,
        constant: &rustc_middle::mir::ConstOperand<'tcx>,
        emitter: &mut Emitter,
    ) {
        use rustc_middle::mir::Const;
        use rustc_middle::ty::ScalarInt;

        match constant.const_ {
            Const::Val(val, ty) => {
                if let Some(scalar) = val.try_to_scalar_int() {
                    // Fits in 128 bits; zero-extend to 256.
                    let v: u128 = scalar.to_uint(scalar.size());
                    emitter.push_u256(v.into());
                } else {
                    todo!("non-scalar constant lowering");
                }
            }
            _ => todo!("non-value constant lowering"),
        }
    }

    fn lower_terminator(
        &self,
        terminator: &rustc_middle::mir::Terminator<'tcx>,
        layout: &EvmLayout,
        emitter: &mut Emitter,
    ) {
        use rustc_middle::mir::TerminatorKind;

        match &terminator.kind {
            TerminatorKind::Return => {
                // The contract's return value convention (ABI encoding) will be
                // handled by a wrapper; for now emit STOP.
                emitter.op(crate::emit::Opcode::STOP);
            }
            TerminatorKind::Unreachable => {
                emitter.op(crate::emit::Opcode::INVALID);
            }
            TerminatorKind::Goto { target } => {
                // Forward jump — address resolved in a second pass.
                emitter.jump(*target);
            }
            TerminatorKind::SwitchInt { discr, targets } => {
                self.lower_switch(discr, targets, layout, emitter);
            }
            TerminatorKind::Call { func, args, destination, target, .. } => {
                self.lower_call(func, args, destination, *target, layout, emitter);
            }
            _ => todo!("terminator lowering: {:?}", terminator.kind),
        }
    }

    fn lower_switch(
        &self,
        discr: &rustc_middle::mir::Operand<'tcx>,
        targets: &rustc_middle::mir::SwitchTargets,
        layout: &EvmLayout,
        emitter: &mut Emitter,
    ) {
        // Emit a chain of EQ / JUMPI pairs.
        self.lower_operand(discr, layout, emitter);
        for (val, target) in targets.iter() {
            // stack: [discr]
            emitter.op(crate::emit::Opcode::DUP1);
            emitter.push_u256(val.into());
            emitter.op(crate::emit::Opcode::EQ);
            emitter.jumpi(target);
        }
        // Discard the remaining discriminant and fall through to the
        // otherwise branch.
        emitter.op(crate::emit::Opcode::POP);
        emitter.jump(targets.otherwise());
    }

    fn lower_call(
        &self,
        func: &rustc_middle::mir::Operand<'tcx>,
        args: &[rustc_middle::mir::Spanned<rustc_middle::mir::Operand<'tcx>>],
        destination: &rustc_middle::mir::Place<'tcx>,
        target: Option<rustc_middle::mir::BasicBlock>,
        layout: &EvmLayout,
        emitter: &mut Emitter,
    ) {
        let tcx = self.tcx;

        // Resolve to a concrete instance so we can check for intrinsics.
        let instance = match func {
            rustc_middle::mir::Operand::Constant(box c) => {
                if let rustc_middle::mir::Const::Val(_, ty) = c.const_ {
                    if let Some(def) = ty.fn_def() {
                        Some(Instance::new(def.0, def.1))
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            _ => None,
        };

        if let Some(inst) = instance {
            if let Some(opcode) = intrinsics::recognize(tcx, inst) {
                // Push arguments onto the EVM stack (right-to-left so that the
                // first argument is on top after all pushes).
                for arg in args.iter().rev() {
                    self.lower_operand(&arg.node, layout, emitter);
                }
                emitter.op(opcode);

                // If the intrinsic produces a value, store it.
                if !ty_is_unit(tcx, inst) {
                    let slot = layout.slot_of(destination.local);
                    emitter.push_u256((slot as u64).into());
                    emitter.op(crate::emit::Opcode::MSTORE);
                }

                if let Some(next_bb) = target {
                    emitter.jump(next_bb);
                }
                return;
            }
        }

        todo!("general function call lowering");
    }

    /// Lower a static item.
    fn codegen_static(&self, def_id: rustc_hir::def_id::DefId) {
        // Statics become entries in contract storage or are embedded in
        // bytecode as constants, depending on their attributes.
        // TODO: implement static lowering.
        let _ = def_id;
    }

    /// Emit inline assembly.
    fn codegen_global_asm(&self, item_id: rustc_hir::ItemId) {
        // `global_asm!` inside an EVM contract: forward raw hex bytes
        // directly to the bytecode buffer.
        // TODO: parse `global_asm!` content as EVM hex and emit verbatim.
        let _ = item_id;
    }
}

// ── helpers ───────────────────────────────────────────────────────────────────

/// Returns `true` if the return type of `instance` is `()`.
fn ty_is_unit<'tcx>(tcx: TyCtxt<'tcx>, instance: Instance<'tcx>) -> bool {
    let sig = tcx
        .fn_sig(instance.def_id())
        .instantiate(tcx, instance.args)
        .skip_binder();
    sig.output().is_unit()
}

/// Build an empty [`CodegenResults`] for the current crate.
///
/// A real implementation would populate `modules` with the compiled bytecode.
fn build_codegen_results<'tcx>(
    tcx: TyCtxt<'tcx>,
    metadata: EncodedMetadata,
    _need_metadata_module: bool,
) -> CodegenResults {
    CodegenResults {
        modules: vec![],
        allocator_module: None,
        metadata_module: None,
        metadata,
        crate_info: rustc_codegen_ssa::back::link::CrateInfo::new(tcx, "evm".to_string()),
    }
}
