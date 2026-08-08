//! `default_repair` is the interesting half of this file: each `Repair::None`
//! it returns is a claim that the operands' well-formedness already proves the
//! result is in range. A wrong claim is a silent miscompile, so every one is
//! pinned here with the reasoning that justifies it.

use evm_isa::by_mnemonic;

use super::*;

// --- the general rule ------------------------------------------------------

#[test]
fn arithmetic_narrower_than_a_word_repairs_to_its_width() {
    assert_eq!(Op::Add.default_repair(Type::U8), Repair::Mask { bits: 8 });
    assert_eq!(Op::Add.default_repair(Type::I8), Repair::Sext { bits: 8 });
    assert_eq!(Op::Mul.default_repair(Type::U64), Repair::Mask { bits: 64 });
    assert_eq!(Op::Sub.default_repair(Type::I32), Repair::Sext { bits: 32 });
}

#[test]
fn arithmetic_at_the_full_word_width_needs_no_repair() {
    // The EVM is a 256-bit machine; there is nothing above `u256` to fix.
    assert_eq!(Op::Add.default_repair(Type::U256), Repair::None);
    assert_eq!(Op::Mul.default_repair(Type::I256), Repair::None);
}

// --- the exceptions, each with its proof -----------------------------------

#[test]
fn a_comparison_needs_no_repair_because_the_evm_yields_zero_or_one() {
    assert_eq!(Op::Icmp(Cmp::Lt).default_repair(Type::BOOL), Repair::None);
    assert_eq!(Op::Icmp(Cmp::Eq).default_repair(Type::BOOL), Repair::None);
}

#[test]
fn bitwise_combination_of_in_range_operands_stays_in_range() {
    // Above the width, unsigned operands are all zero and signed operands are
    // all copies of their sign bits — so the result's high bits agree with its
    // own bit w-1 either way.
    for op in [Op::And, Op::Or, Op::Xor] {
        assert_eq!(op.default_repair(Type::U8), Repair::None, "{op:?} on u8");
        assert_eq!(op.default_repair(Type::I8), Repair::None, "{op:?} on i8");
    }
}

#[test]
fn not_needs_a_mask_only_when_the_result_is_unsigned() {
    // EVM NOT flips all 256 bits. A sign-extended word stays sign-extended; a
    // zero-extended one becomes all ones above its width.
    assert_eq!(Op::Not.default_repair(Type::U8), Repair::Mask { bits: 8 });
    assert_eq!(Op::Not.default_repair(Type::I8), Repair::None);
}

#[test]
fn unsigned_division_cannot_leave_the_range_its_operands_were_in() {
    // a / b <= a, and a % b < b.
    assert_eq!(Op::Div.default_repair(Type::U8), Repair::None);
    assert_eq!(Op::Rem.default_repair(Type::U8), Repair::None);
    // Signed division can: INT_MIN / -1 overflows.
    assert_eq!(Op::Div.default_repair(Type::I8), Repair::Sext { bits: 8 });
}

#[test]
fn a_logical_right_shift_of_an_unsigned_value_stays_in_range() {
    assert_eq!(Op::Shr.default_repair(Type::U8), Repair::None);
    // A left shift moves bits up and out, so it always needs repairing.
    assert_eq!(Op::Shl.default_repair(Type::U8), Repair::Mask { bits: 8 });
    assert_eq!(Op::Sar.default_repair(Type::I8), Repair::Sext { bits: 8 });
}

#[test]
fn a_value_that_came_from_elsewhere_well_formed_needs_no_repair() {
    // Select returns one of its operands; a call returns what the callee's own
    // repairs produced; an alloca returns a frame offset.
    assert_eq!(Op::Select.default_repair(Type::U8), Repair::None);
    assert_eq!(Op::Call { callee: Func::new(0) }.default_repair(Type::U8), Repair::None);
    assert_eq!(Op::Alloca { bytes: 32 }.default_repair(Type::PTR), Repair::None);
}

#[test]
fn a_load_repairs_because_memory_holds_whatever_was_put_there() {
    assert_eq!(Op::Load.default_repair(Type::U8), Repair::Mask { bits: 8 });
    assert_eq!(Op::SLoad.default_repair(Type::U256), Repair::None);
    assert_eq!(Op::SLoad.default_repair(Type::U8), Repair::Mask { bits: 8 });
}

// --- effects ---------------------------------------------------------------

#[test]
fn the_pure_operations_are_the_ones_that_only_touch_their_operands() {
    for op in [Op::Add, Op::Xor, Op::Icmp(Cmp::Lt), Op::Convert, Op::Select, Op::Exp] {
        assert!(op.effects().is_pure(), "{op:?} should be pure");
    }
}

#[test]
fn memory_and_storage_operations_carry_the_effect_that_names_them() {
    assert_eq!(Op::Load.effects(), Effects::READS_MEMORY);
    assert_eq!(Op::Store.effects(), Effects::WRITES_MEMORY);
    assert_eq!(Op::SLoad.effects(), Effects::READS_STORAGE);
    assert_eq!(Op::SStore.effects(), Effects::WRITES_STORAGE);
    assert_eq!(Op::TLoad.effects(), Effects::READS_TRANSIENT);
    assert_eq!(Op::TStore.effects(), Effects::WRITES_TRANSIENT);
}

#[test]
fn an_alloca_is_not_pure_so_that_two_of_them_never_collapse_into_one() {
    // Its offset is a function of the frame layout alone, which makes it look
    // CSE-able. Two allocas must stay two distinct addresses.
    assert!(!Op::Alloca { bytes: 32 }.effects().is_pure());
}

#[test]
fn an_internal_call_is_conservatively_assumed_to_do_anything() {
    assert_eq!(Op::Call { callee: Func::new(0) }.effects(), Effects::ALL);
}

#[test]
fn a_raw_opcode_takes_its_effects_from_the_table() {
    let sload = by_mnemonic("SLOAD").unwrap();
    assert_eq!(Op::Opcode(sload).effects(), Effects::READS_STORAGE);

    let add = by_mnemonic("ADD").unwrap();
    assert!(Op::Opcode(add).effects().is_pure());
}

// --- shape -----------------------------------------------------------------

#[test]
fn checked_arithmetic_is_the_only_thing_with_two_results() {
    assert_eq!(Op::AddOverflow.result_count(), 2);
    assert_eq!(Op::SubOverflow.result_count(), 2);
    assert_eq!(Op::MulOverflow.result_count(), 2);
    assert_eq!(Op::Add.result_count(), 1);
}

#[test]
fn the_storing_operations_produce_nothing() {
    for op in [Op::Store, Op::SStore, Op::TStore, Op::MemCopy] {
        assert_eq!(op.result_count(), 0, "{op:?}");
    }
}

#[test]
fn a_raw_opcode_produces_what_the_table_says_it_pushes() {
    assert_eq!(Op::Opcode(by_mnemonic("SLOAD").unwrap()).result_count(), 1);
    assert_eq!(Op::Opcode(by_mnemonic("SSTORE").unwrap()).result_count(), 0);
    assert_eq!(Op::Opcode(by_mnemonic("CALL").unwrap()).result_count(), 1);
}

// --- terminators -----------------------------------------------------------

#[test]
fn a_branch_reports_both_of_its_targets() {
    let term = Terminator::Brif {
        cond: Value::new(0),
        then: BlockCall::new(Block::new(1), [Value::new(2)]),
        otherwise: BlockCall::new(Block::new(2), []),
    };

    let targets: Vec<_> = term.block_calls().iter().map(|call| call.block).collect();
    assert_eq!(targets, [Block::new(1), Block::new(2)]);
}

#[test]
fn a_switch_reports_its_default_last() {
    let term = Terminator::Switch {
        on: Value::new(0),
        cases: vec![
            (1u8.into(), BlockCall::new(Block::new(1), [])),
            (2u8.into(), BlockCall::new(Block::new(2), [])),
        ],
        default: BlockCall::new(Block::new(3), []),
    };

    let targets: Vec<_> = term.block_calls().iter().map(|call| call.block).collect();
    assert_eq!(targets, [Block::new(1), Block::new(2), Block::new(3)]);
}

#[test]
fn terminator_arguments_include_the_values_passed_along_an_edge() {
    // A block parameter is fed from here, so an edge argument is a use like
    // any other — the dominance check in `verify` depends on seeing them.
    let term = Terminator::Brif {
        cond: Value::new(0),
        then: BlockCall::new(Block::new(1), [Value::new(5)]),
        otherwise: BlockCall::new(Block::new(2), [Value::new(6)]),
    };

    assert_eq!(term.args(), [Value::new(0), Value::new(5), Value::new(6)]);
}

#[test]
fn a_halt_ends_the_frame_but_a_return_does_not() {
    assert!(Terminator::Stop.is_halt());
    assert!(Terminator::Invalid.is_halt());
    assert!(
        Terminator::Revert {
            ptr: Value::new(0),
            len: Value::new(1)
        }
        .is_halt()
    );

    // A `Return` jumps back to an internal caller; the frame lives on.
    assert!(!Terminator::Return(vec![]).is_halt());
    assert!(!Terminator::Jump(BlockCall::new(Block::new(1), [])).is_halt());
}

#[test]
fn unreachable_is_a_separate_terminator_from_the_invalid_opcode() {
    // Both lower to 0xfe. Only one of them may be reasoned from, and
    // conflating them is how a revert eventually gets optimized away.
    assert_ne!(Terminator::Unreachable, Terminator::Invalid);
}
