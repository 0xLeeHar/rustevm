//! The cases worth pinning are the ones where `evm-isa`'s category is not the
//! whole story — those are where a plausible-looking classification silently
//! licenses an illegal reordering.

use evm_isa::by_mnemonic;

use super::*;

fn effects_of(mnemonic: &str) -> Effects {
    Effects::of_opcode(by_mnemonic(mnemonic).expect("mnemonic is in the table"))
}

// --- where the category is not enough --------------------------------------

#[test]
fn the_copy_family_writes_memory_despite_being_categorised_as_context() {
    // These are `OpCategory::Context` because of where their source comes
    // from. Believing that would let an optimizer hoist an MLOAD across one.
    for mnemonic in ["CALLDATACOPY", "CODECOPY", "EXTCODECOPY", "RETURNDATACOPY"] {
        let effects = effects_of(mnemonic);
        assert!(
            effects.contains(Effects::WRITES_MEMORY),
            "{mnemonic} writes memory, whatever its category says"
        );
        assert!(!effects.is_pure(), "{mnemonic} is not pure");
    }
}

#[test]
fn keccak256_reads_memory_rather_than_being_arithmetic() {
    let effects = effects_of("KECCAK256");
    assert!(effects.contains(Effects::READS_MEMORY));
    assert!(
        !effects.contains(Effects::WRITES_MEMORY),
        "it hashes a range, it does not write one"
    );
}

#[test]
fn a_log_reads_the_memory_its_payload_comes_from() {
    for mnemonic in ["LOG0", "LOG1", "LOG2", "LOG3", "LOG4"] {
        let effects = effects_of(mnemonic);
        assert!(effects.contains(Effects::LOGS), "{mnemonic}");
        assert!(
            effects.contains(Effects::READS_MEMORY),
            "{mnemonic} takes its data from memory"
        );
    }
}

#[test]
fn the_four_storage_opcodes_share_a_category_and_no_effects() {
    assert_eq!(effects_of("SLOAD"), Effects::READS_STORAGE);
    assert_eq!(effects_of("SSTORE"), Effects::WRITES_STORAGE);
    assert_eq!(effects_of("TLOAD"), Effects::READS_TRANSIENT);
    assert_eq!(effects_of("TSTORE"), Effects::WRITES_TRANSIENT);
}

// --- the ordinary cases ----------------------------------------------------

#[test]
fn arithmetic_and_bitwise_opcodes_are_pure() {
    for mnemonic in [
        "ADD", "MUL", "SDIV", "LT", "SLT", "EQ", "ISZERO", "AND", "XOR", "NOT", "SHL",
    ] {
        assert!(effects_of(mnemonic).is_pure(), "{mnemonic} should be pure");
    }
}

#[test]
fn reading_the_environment_is_not_pure_even_though_it_changes_nothing() {
    // GAS differs on every execution and BALANCE can move under a re-entrant
    // call, so neither is foldable or CSE-able across an external call.
    for mnemonic in ["TIMESTAMP", "CALLER", "GAS", "BALANCE", "NUMBER"] {
        let effects = effects_of(mnemonic);
        assert!(!effects.is_pure(), "{mnemonic} should not be pure");
        assert!(effects.contains(Effects::READS_CONTEXT), "{mnemonic}");
    }
}

#[test]
fn an_external_call_is_a_barrier_to_everything() {
    for mnemonic in ["CALL", "DELEGATECALL", "STATICCALL", "CREATE", "CREATE2"] {
        let effects = effects_of(mnemonic);
        assert!(effects.contains(Effects::EXTERNAL), "{mnemonic}");
        assert!(
            effects.contains(Effects::READS_MEMORY),
            "{mnemonic} passes its arguments in memory"
        );
        assert!(
            effects.contains(Effects::WRITES_MEMORY),
            "{mnemonic} writes returndata back"
        );
    }
}

#[test]
fn mload_reads_and_mstore_writes() {
    assert_eq!(effects_of("MLOAD"), Effects::READS_MEMORY);
    assert_eq!(effects_of("MSTORE"), Effects::WRITES_MEMORY);
    assert_eq!(effects_of("MSTORE8"), Effects::WRITES_MEMORY);
    assert!(effects_of("MCOPY").contains(Effects::READS_MEMORY));
    assert!(effects_of("MCOPY").contains(Effects::WRITES_MEMORY));
}

// --- the safety net --------------------------------------------------------

#[test]
fn an_opcode_this_module_has_not_classified_is_conservatively_impure() {
    // JUMP is `Control`: EIR owns control flow, so it never reaches
    // `Op::Opcode`. The point is the fallback — a table entry nobody has
    // thought about must never come back pure.
    assert_eq!(effects_of("JUMP"), Effects::ALL);
    assert_eq!(effects_of("STOP"), Effects::ALL);
    assert!(!Effects::ALL.is_removable());
}

#[test]
fn no_opcode_in_the_table_panics_or_comes_back_accidentally_pure() {
    use evm_isa::OPCODES;

    for spec in OPCODES {
        let effects = Effects::of_opcode(spec);
        let expected_pure = matches!(
            spec.category,
            OpCategory::Arithmetic | OpCategory::Comparison | OpCategory::Bitwise | OpCategory::StackManip
        );
        assert_eq!(
            effects.is_pure(),
            expected_pure,
            "{} is classified pure = {}, expected {expected_pure}",
            spec.mnemonic,
            effects.is_pure()
        );
    }
}

// --- set algebra -----------------------------------------------------------

#[test]
fn contains_asks_for_every_effect_and_intersects_for_any() {
    let both = Effects::READS_MEMORY.union(Effects::WRITES_STORAGE);

    assert!(both.contains(Effects::READS_MEMORY));
    assert!(both.contains(both));
    assert!(!both.contains(Effects::READS_MEMORY.union(Effects::LOGS)));
    assert!(both.intersects(Effects::READS_MEMORY.union(Effects::LOGS)));
    assert!(!both.intersects(Effects::LOGS));
}

#[test]
fn only_a_pure_instruction_is_removable() {
    assert!(Effects::PURE.is_removable());
    assert!(!Effects::READS_CONTEXT.is_removable());
    assert!(!Effects::LOGS.is_removable());
}

#[test]
fn effects_render_as_a_readable_list() {
    assert_eq!(Effects::PURE.to_string(), "pure");
    assert_eq!(Effects::READS_MEMORY.to_string(), "rmem");
    assert_eq!(
        Effects::READS_MEMORY.union(Effects::WRITES_MEMORY).to_string(),
        "rmem|wmem"
    );
}
