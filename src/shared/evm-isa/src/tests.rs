use crate::{Fork, OPCODES, OpForm, by_byte, by_mnemonic};
use std::collections::HashSet;

#[test]
fn table_size() {
    assert_eq!(OPCODES.len(), 149);
}

#[test]
fn no_duplicate_bytes() {
    let mut seen = HashSet::new();
    for spec in OPCODES {
        assert!(
            seen.insert(spec.byte),
            "duplicate byte 0x{:02x} ({})",
            spec.byte,
            spec.mnemonic
        );
    }
}

#[test]
fn no_duplicate_mnemonics() {
    let mut seen = HashSet::new();
    for spec in OPCODES {
        assert!(
            seen.insert(spec.mnemonic),
            "duplicate mnemonic {}",
            spec.mnemonic
        );
    }
}

#[test]
fn lookup_round_trips_every_entry() {
    for spec in OPCODES {
        assert_eq!(by_mnemonic(spec.mnemonic).unwrap().byte, spec.byte);
        assert_eq!(by_byte(spec.byte).unwrap().mnemonic, spec.mnemonic);
    }
}

#[test]
fn unknown_lookups_return_none() {
    assert!(by_mnemonic("NOTANOPCODE").is_none());
    assert!(by_byte(0x0c).is_none()); // unassigned byte between SIGNEXTEND and LT
}

#[test]
fn push_dup_swap_byte_ranges_are_contiguous() {
    for n in 1..=32u8 {
        let spec = by_mnemonic(&std::format!("PUSH{n}")).unwrap();
        assert_eq!(spec.byte, 0x5f + n);
        assert_eq!(spec.stack_in, 0);
        assert_eq!(spec.stack_out, 1);
    }
    for n in 1..=16u8 {
        let dup = by_mnemonic(&std::format!("DUP{n}")).unwrap();
        assert_eq!(dup.byte, 0x7f + n);
        assert_eq!(dup.stack_in, n);
        assert_eq!(dup.stack_out, n + 1);

        let swap = by_mnemonic(&std::format!("SWAP{n}")).unwrap();
        assert_eq!(swap.byte, 0x8f + n);
        assert_eq!(swap.stack_in, n + 1);
        assert_eq!(swap.stack_out, n + 1);
    }
}

#[test]
fn op_form_recovers_push_dup_swap_families() {
    assert_eq!(
        crate::op_form(by_mnemonic("PUSH0").unwrap()),
        OpForm::Push(0)
    );
    assert_eq!(
        crate::op_form(by_mnemonic("PUSH17").unwrap()),
        OpForm::Push(17)
    );
    assert_eq!(crate::op_form(by_mnemonic("DUP9").unwrap()), OpForm::Dup(9));
    assert_eq!(
        crate::op_form(by_mnemonic("SWAP16").unwrap()),
        OpForm::Swap(16)
    );
    assert_eq!(crate::op_form(by_mnemonic("ADD").unwrap()), OpForm::Simple);
}

#[test]
fn fork_ordering_is_chronological() {
    assert!(Fork::Frontier < Fork::Homestead);
    assert!(Fork::Homestead < Fork::Byzantium);
    assert!(Fork::Byzantium < Fork::Constantinople);
    assert!(Fork::Constantinople < Fork::Istanbul);
    assert!(Fork::Istanbul < Fork::Berlin);
    assert!(Fork::Berlin < Fork::London);
    assert!(Fork::London < Fork::Shanghai);
    assert!(Fork::Shanghai < Fork::Cancun);
    assert!(Fork::Cancun < Fork::Prague);
}

#[test]
fn derived_predicates() {
    let sload = by_mnemonic("SLOAD").unwrap();
    let sstore = by_mnemonic("SSTORE").unwrap();
    let tload = by_mnemonic("TLOAD").unwrap();
    let tstore = by_mnemonic("TSTORE").unwrap();
    let add = by_mnemonic("ADD").unwrap();
    let timestamp = by_mnemonic("TIMESTAMP").unwrap();
    let dup1 = by_mnemonic("DUP1").unwrap();

    assert!(sload.reads_storage());
    assert!(!sload.writes_storage());
    assert!(tload.reads_storage());

    assert!(sstore.writes_storage());
    assert!(!sstore.reads_storage());
    assert!(tstore.writes_storage());

    assert!(add.is_pure());
    assert!(!timestamp.is_pure());

    assert!(dup1.is_stack_manip());
    assert!(!add.is_stack_manip());
}
