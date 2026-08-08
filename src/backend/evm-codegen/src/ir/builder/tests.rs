//! Building by hand — the surface step 2 exists for.
//!
//! Nothing here executes; these check that the builder produces the structure
//! it claims to and refuses what it says it refuses. What those structures
//! *do* is `stackify`'s tests.

use evm_isa::Fork;

use super::*;
use crate::ir::{Module, ValueDef};

fn sig(params: impl IntoIterator<Item = Type>, returns: impl IntoIterator<Item = Type>) -> Signature {
    Signature::new(params, returns)
}

// --- the worked example from the design doc -------------------------------

#[test]
fn a_checked_addition_with_a_panic_branch_builds() {
    // `fn add(a: u64, b: u64) -> u64 { a + b }` with overflow checks on.
    let mut f = FunctionBuilder::new("add", sig([Type::U64, Type::U64], [Type::U64]));
    let (a, b) = (f.param(0), f.param(1));
    let ok = f.create_block();
    let panic = f.create_block();
    f.set_block_kind(panic, BlockKind::Panic);

    let (sum, overflowed) = f.ins().add_overflow(Type::U64, a, b);
    f.note("_3 = CheckedAdd(_1, _2)");
    f.brif(overflowed, panic, &[], ok, &[]);

    f.switch_to(ok);
    f.ret(&[sum]);

    f.switch_to(panic);
    let zero = f.ins().iconst(Type::PTR, 0u8);
    f.revert(zero, zero);

    let func = f.finish().expect("builds");
    assert_eq!(func.num_blocks(), 3);
    assert_eq!(func.value_type(sum), Type::U64);
    assert_eq!(func.value_type(overflowed), Type::BOOL);
    assert_eq!(func.block(panic).kind, BlockKind::Panic);
}

#[test]
fn a_loop_carries_its_counter_in_a_block_parameter() {
    // The shape a phi node would otherwise take: the merge point declares what
    // it needs, and every predecessor supplies it on the edge.
    let mut f = FunctionBuilder::new("sum_to", sig([Type::U64], [Type::U64]));
    let limit = f.param(0);

    let head = f.create_block_with(&[Type::U64, Type::U64]); // (i, acc)
    let body = f.create_block();
    let done = f.create_block();

    let zero = f.ins().iconst(Type::U64, 0u8);
    f.jump(head, &[zero, zero]);

    f.switch_to(head);
    let (i, acc) = (f.block_param(head, 0), f.block_param(head, 1));
    let more = f.ins().icmp(Cmp::Lt, i, limit);
    f.brif(more, body, &[], done, &[]);

    f.switch_to(body);
    let one = f.ins().iconst(Type::U64, 1u8);
    let next_i = f.ins().add(Type::U64, i, one);
    let next_acc = f.ins().add(Type::U64, acc, i);
    f.jump(head, &[next_i, next_acc]);

    f.switch_to(done);
    f.ret(&[acc]);

    let func = f.finish().expect("builds");
    assert_eq!(func.block_params(head).len(), 2);
    assert_eq!(func.value_type(i), Type::U64);
}

// --- repairs are filled in for you ----------------------------------------

#[test]
fn the_builder_fills_in_the_repair_so_a_frontend_cannot_forget_it() {
    let mut f = FunctionBuilder::new("f", sig([Type::U8, Type::U8], [Type::U8]));
    let (a, b) = (f.param(0), f.param(1));

    let sum = f.ins().add(Type::U8, a, b);
    f.ret(&[sum]);
    let func = f.finish().unwrap();

    let def = func.value_def(sum);
    let ValueDef::Result { inst, .. } = def else {
        panic!("a sum is an instruction result");
    };
    assert_eq!(func.inst(inst).repair, Repair::Mask { bits: 8 }, "u8 addition masks");
}

#[test]
fn a_convert_takes_its_repair_from_both_types() {
    let mut f = FunctionBuilder::new("f", sig([Type::U8], [Type::I8]));
    let a = f.param(0);

    let widened = f.ins().convert(Type::U32, a);
    let reinterpreted = f.ins().convert(Type::I8, a);
    f.ret(&[reinterpreted]);
    let func = f.finish().unwrap();

    let repair_of = |value| {
        let ValueDef::Result { inst, .. } = func.value_def(value) else {
            panic!("not an instruction result");
        };
        func.inst(inst).repair
    };

    assert_eq!(repair_of(widened), Repair::None, "u8 -> u32 costs nothing");
    assert_eq!(
        repair_of(reinterpreted),
        Repair::Sext { bits: 8 },
        "u8 -> i8 sign-extends"
    );
}

// --- what the raw opcode hatch refuses ------------------------------------

#[test]
fn stack_manipulating_opcodes_are_rejected_by_the_raw_opcode_node() {
    for mnemonic in ["PUSH1", "DUP1", "SWAP1", "POP"] {
        let mut f = FunctionBuilder::new("f", sig([], []));
        f.ins().opcode(mnemonic, Type::U256, &[]);
        f.stop();

        let error = f.finish().expect_err("the stackifier owns the stack");
        assert!(
            matches!(error, IrError::ReservedOpcode { .. }),
            "{mnemonic} gave {error:?}"
        );
    }
}

#[test]
fn control_flow_opcodes_are_rejected_by_the_raw_opcode_node() {
    for mnemonic in ["JUMP", "JUMPI", "JUMPDEST", "PC"] {
        let mut f = FunctionBuilder::new("f", sig([], []));
        f.ins().opcode_void(mnemonic, &[]);
        f.stop();

        assert!(
            matches!(f.finish(), Err(IrError::ReservedOpcode { .. })),
            "{mnemonic} should be rejected — EIR owns control flow"
        );
    }
}

#[test]
fn terminating_opcodes_are_rejected_because_they_are_terminators() {
    for mnemonic in ["STOP", "RETURN", "REVERT", "SELFDESTRUCT", "INVALID"] {
        let mut f = FunctionBuilder::new("f", sig([], []));
        f.ins().opcode_void(mnemonic, &[]);
        f.stop();

        assert!(
            matches!(f.finish(), Err(IrError::ReservedOpcode { .. })),
            "{mnemonic} should be a Terminator, not an Op"
        );
    }
}

#[test]
fn an_unknown_mnemonic_is_reported_at_finish() {
    let mut f = FunctionBuilder::new("f", sig([], []));
    f.ins().opcode("FROBNICATE", Type::U256, &[]);
    f.stop();

    assert_eq!(
        f.finish().expect_err("FROBNICATE is not an opcode"),
        IrError::UnknownMnemonic("FROBNICATE".to_string())
    );
}

#[test]
fn an_ordinary_opcode_goes_through_the_hatch_untouched() {
    let mut f = FunctionBuilder::new("f", sig([], [Type::U256]));
    let caller = f.ins().opcode("CALLER", Type::U256, &[]);
    f.ret(&[caller]);

    let func = f.finish().expect("CALLER is not reserved");
    let ValueDef::Result { inst, .. } = func.value_def(caller) else {
        panic!("not a result");
    };
    assert_eq!(func.inst(inst).op.name(), "CALLER");
}

// --- sticky errors, as in Asm ---------------------------------------------

#[test]
fn the_first_builder_error_is_the_one_reported() {
    let mut f = FunctionBuilder::new("f", sig([], []));
    f.ins().opcode_void("FROBNICATE", &[]);
    f.ins().opcode_void("JUMP", &[]);
    f.stop();

    assert_eq!(
        f.finish().expect_err("both calls failed"),
        IrError::UnknownMnemonic("FROBNICATE".to_string()),
        "the later reserved-opcode error must not displace it"
    );
}

#[test]
fn nothing_is_emitted_after_an_error() {
    let mut f = FunctionBuilder::new("f", sig([], []));
    f.ins().opcode_void("FROBNICATE", &[]);
    let before = f.func.num_insts();
    f.ins().iconst(Type::U64, 1u8);

    assert_eq!(f.func.num_insts(), before, "later emits are no-ops");
}

#[test]
fn an_instruction_after_a_terminator_is_an_error() {
    // Not merely unreachable — it would silently vanish, and a frontend that
    // does it has lost track of where it is.
    let mut f = FunctionBuilder::new("f", sig([], []));
    f.stop();
    f.ins().iconst(Type::U64, 1u8);

    assert!(matches!(f.finish(), Err(IrError::NoCurrentBlock)));
}

#[test]
fn terminating_a_block_twice_is_an_error() {
    let mut f = FunctionBuilder::new("f", sig([], []));
    let second = f.create_block();
    f.stop();
    f.switch_to(second);
    f.stop();
    f.switch_to(second);
    f.stop();

    assert!(matches!(f.finish(), Err(IrError::AlreadyTerminated { .. })));
}

// --- module ----------------------------------------------------------------

#[test]
fn a_function_can_be_called_before_it_is_defined() {
    // Mutual recursion needs this, and so does the stackifier's first pass.
    let mut m = Module::new(Fork::Cancun);
    let callee_sig = sig([Type::U64], [Type::U64]);
    let callee = m.declare("callee", callee_sig.clone());

    let mut f = FunctionBuilder::new("caller", sig([], [Type::U64]));
    let arg = f.ins().iconst(Type::U64, 7u8);
    let result = f.ins().call(callee, &callee_sig, &[arg])[0];
    f.ret(&[result]);
    let caller = m.declare("caller", sig([], [Type::U64]));
    m.define(caller, f.finish().unwrap()).unwrap();

    // The callee still has no body; the call site already exists.
    assert!(m.get(callee).is_none());

    let mut g = FunctionBuilder::new("callee", callee_sig.clone());
    let x = g.param(0);
    g.ret(&[x]);
    m.define(callee, g.finish().unwrap()).unwrap();
    m.set_entry(caller);

    assert!(m.verify().is_ok());
    assert_eq!(m.get(callee).unwrap().sig, callee_sig);
}

#[test]
fn a_call_mints_one_value_per_return_in_the_callees_signature() {
    let two_returns = sig([], [Type::U64, Type::BOOL]);
    let mut m = Module::new(Fork::Cancun);
    let callee = m.declare("two", two_returns.clone());

    let mut f = FunctionBuilder::new("f", sig([], []));
    let results = f.ins().call(callee, &two_returns, &[]);
    f.stop();
    let func = f.finish().unwrap();

    assert_eq!(results.len(), 2);
    assert_eq!(func.value_type(results[0]), Type::U64);
    assert_eq!(func.value_type(results[1]), Type::BOOL);
}
