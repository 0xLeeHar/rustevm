//! Arena invariants. Two of these are load-bearing for the stackifier rather
//! than merely tidy: value indices are the frame slot map, and entry-block
//! parameters occupy the first slots.

use super::*;
use crate::ir::inst::{BlockCall, InstData, Op};
use crate::ir::types::Repair;

fn empty_inst(op: Op, ty: Type, args: Vec<Value>) -> InstData {
    InstData {
        op,
        ty,
        repair: op.default_repair(ty),
        args,
        results: Vec::new(),
        note: None,
    }
}

fn func_with_two_u64_params() -> Function {
    Function::new("f", Signature::new([Type::U64, Type::U64], [Type::U64]))
}

// --- the two invariants the stackifier depends on --------------------------

#[test]
fn the_entry_block_gets_one_parameter_per_signature_parameter() {
    let f = func_with_two_u64_params();

    assert_eq!(f.entry(), Block::new(0), "the entry block is always block0");
    assert_eq!(f.block_params(f.entry()).len(), 2);
    assert_eq!(f.value_type(f.block_params(f.entry())[0]), Type::U64);
}

#[test]
fn parameters_are_the_first_values_so_they_are_the_first_frame_slots() {
    // A caller writes into a callee's parameter slots knowing nothing but its
    // handle. That only works because parameters come first, before any
    // instruction has had a chance to mint a value.
    let f = func_with_two_u64_params();

    assert_eq!(f.block_params(f.entry()), [Value::new(0), Value::new(1)]);
}

#[test]
fn a_value_index_is_its_definition_order() {
    let mut f = func_with_two_u64_params();
    let entry = f.entry();

    let inst = f.append_inst(
        entry,
        empty_inst(Op::Add, Type::U64, vec![Value::new(0), Value::new(1)]),
        &[Type::U64],
    );

    // Two parameters were minted first, so the sum is v2.
    assert_eq!(f.results(inst), [Value::new(2)]);
    assert_eq!(f.num_values(), 3);
}

// --- results ---------------------------------------------------------------

#[test]
fn a_two_result_instruction_mints_both_in_order() {
    let mut f = func_with_two_u64_params();
    let entry = f.entry();

    let inst = f.append_inst(
        entry,
        empty_inst(Op::AddOverflow, Type::U64, vec![Value::new(0), Value::new(1)]),
        &[Type::U64, Type::BOOL],
    );

    let results = f.results(inst);
    assert_eq!(results, [Value::new(2), Value::new(3)]);
    assert_eq!(f.value_type(results[0]), Type::U64);
    assert_eq!(f.value_type(results[1]), Type::BOOL, "the overflow bit is a bool");
}

#[test]
fn an_instruction_with_no_results_mints_no_values() {
    let mut f = func_with_two_u64_params();
    let entry = f.entry();
    let before = f.num_values();

    f.append_inst(
        entry,
        empty_inst(Op::Store, Type::U64, vec![Value::new(0), Value::new(1)]),
        &[],
    );

    assert_eq!(f.num_values(), before);
}

#[test]
fn a_result_knows_the_instruction_that_defined_it() {
    let mut f = func_with_two_u64_params();
    let entry = f.entry();

    let inst = f.append_inst(
        entry,
        empty_inst(Op::Add, Type::U64, vec![Value::new(0), Value::new(1)]),
        &[Type::U64],
    );
    let sum = f.results(inst)[0];

    assert_eq!(f.value_def(sum), ValueDef::Result { inst, index: 0 });
}

#[test]
fn a_parameter_knows_the_block_that_declared_it() {
    let f = func_with_two_u64_params();
    let second = f.block_params(f.entry())[1];

    assert_eq!(
        f.value_def(second),
        ValueDef::Param {
            block: f.entry(),
            index: 1
        }
    );
}

// --- blocks ----------------------------------------------------------------

#[test]
fn a_block_parameter_is_a_value_of_the_type_it_was_declared_with() {
    let mut f = func_with_two_u64_params();
    let loop_head = f.create_block();

    let counter = f.append_param(loop_head, Type::U32);

    assert_eq!(f.value_type(counter), Type::U32);
    assert_eq!(f.block_params(loop_head), [counter]);
}

#[test]
fn a_new_block_starts_with_no_terminator() {
    let mut f = func_with_two_u64_params();
    let block = f.create_block();

    assert_eq!(f.terminator(block), None, "verify rejects one that stays this way");

    f.set_terminator(block, Terminator::Jump(BlockCall::new(f.entry(), [])));
    assert!(f.terminator(block).is_some());
}

#[test]
fn blocks_are_listed_in_index_order_with_the_entry_first() {
    let mut f = func_with_two_u64_params();
    let second = f.create_block();
    let third = f.create_block();

    assert_eq!(f.blocks().collect::<Vec<_>>(), [f.entry(), second, third]);
}

#[test]
fn a_block_is_normal_until_it_is_marked_otherwise() {
    let mut f = func_with_two_u64_params();
    let panic = f.create_block();

    assert_eq!(f.block(panic).kind, BlockKind::Normal);
    f.set_block_kind(panic, BlockKind::Panic);
    assert_eq!(f.block(panic).kind, BlockKind::Panic);
}

// --- mutation, for the optimizer ------------------------------------------

#[test]
fn a_repair_can_be_weakened_in_place_which_is_all_mask_elision_is() {
    let mut f = func_with_two_u64_params();
    let entry = f.entry();
    let inst = f.append_inst(
        entry,
        empty_inst(Op::Add, Type::U64, vec![Value::new(0), Value::new(1)]),
        &[Type::U64],
    );

    assert_eq!(f.inst(inst).repair, Repair::Mask { bits: 64 });
    f.inst_mut(inst).repair = Repair::None;
    assert_eq!(f.inst(inst).repair, Repair::None, "a field write, not a graph edit");
}

#[test]
fn a_note_survives_onto_the_instruction_for_the_dump_to_print() {
    let mut f = func_with_two_u64_params();
    let entry = f.entry();
    let inst = f.append_inst(
        entry,
        empty_inst(Op::Add, Type::U64, vec![Value::new(0), Value::new(1)]),
        &[Type::U64],
    );

    f.set_note(inst, "_3 = Add(_1, _2)");

    assert_eq!(f.inst(inst).note.as_deref(), Some("_3 = Add(_1, _2)"));
}

// --- bounds ----------------------------------------------------------------

#[test]
fn a_function_can_say_whether_a_handle_belongs_to_it() {
    // `verify` needs this: a `Value` is just an index, so one from another
    // function would otherwise silently address a slot in this one.
    let f = func_with_two_u64_params();

    assert!(f.has_value(Value::new(1)));
    assert!(!f.has_value(Value::new(2)));
    assert!(f.has_block(Block::new(0)));
    assert!(!f.has_block(Block::new(1)));
}
