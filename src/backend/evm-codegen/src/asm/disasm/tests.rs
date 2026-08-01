//! Decoding tests — the interesting cases are all about where an instruction
//! ends, because that is the only thing a disassembler can get wrong in a way
//! that corrupts everything after it.

use evm_isa::Fork;

use super::*;
use crate::Asm;

/// Mnemonic and immediate of each decoded instruction, for terse assertions.
fn decode(code: &[u8]) -> Vec<(&'static str, Vec<u8>)> {
    disassemble(code)
        .map(|i| (i.mnemonic(), i.immediate.to_vec()))
        .collect()
}

// --- the straightforward cases ---------------------------------------------

#[test]
fn decodes_operands_and_advances_past_them() {
    // PUSH1 0x02, PUSH1 0x03, ADD
    let instructions: Vec<_> = disassemble(&[0x60, 0x02, 0x60, 0x03, 0x01]).collect();

    assert_eq!(instructions.len(), 3);
    assert_eq!(instructions[0].pc, 0);
    assert_eq!(instructions[0].mnemonic(), "PUSH1");
    assert_eq!(instructions[0].immediate, &[0x02]);
    assert_eq!(instructions[1].pc, 2, "the immediate is not an instruction");
    assert_eq!(instructions[2].pc, 4);
    assert_eq!(instructions[2].mnemonic(), "ADD");
    assert_eq!(instructions[2].immediate, &[] as &[u8]);
}

#[test]
fn push32_takes_all_thirty_two_bytes() {
    let mut code = vec![0x7f];
    code.extend_from_slice(&[0xab; 32]);
    code.push(0x00); // STOP

    let instructions: Vec<_> = disassemble(&code).collect();

    assert_eq!(instructions[0].immediate, &[0xab; 32]);
    assert_eq!(instructions[0].size(), 33);
    assert_eq!(instructions[1].pc, 33);
    assert_eq!(instructions[1].mnemonic(), "STOP");
}

#[test]
fn push0_carries_no_immediate() {
    // PUSH0 is its own opcode, not a PUSH with a zero-length operand to skip.
    let instructions: Vec<_> = disassemble(&[0x5f, 0x01]).collect();

    assert_eq!(instructions[0].mnemonic(), "PUSH0");
    assert_eq!(instructions[0].immediate, &[] as &[u8]);
    assert!(!instructions[0].truncated);
    assert_eq!(instructions[1].pc, 1, "PUSH0 is one byte wide");
}

#[test]
fn empty_code_decodes_to_nothing() {
    assert_eq!(disassemble(&[]).count(), 0);
    assert_eq!(listing(&[]).to_string(), "");
}

// --- the cases that come from real bytecode --------------------------------

#[test]
fn a_push_running_off_the_end_keeps_what_was_there() {
    // PUSH4 with only two bytes behind it. Contract metadata decodes into
    // this constantly; giving up or panicking here would make the tool
    // useless on anything from a chain.
    let instructions: Vec<_> = disassemble(&[0x63, 0xaa, 0xbb]).collect();

    assert_eq!(instructions.len(), 1);
    assert_eq!(instructions[0].mnemonic(), "PUSH4");
    assert_eq!(instructions[0].immediate, &[0xaa, 0xbb]);
    assert!(instructions[0].truncated);
    assert_eq!(instructions[0].missing(), 2);
    assert_eq!(instructions[0].immediate_width(), 4);
}

#[test]
fn a_push_at_the_very_last_byte_is_truncated_not_skipped() {
    let instructions: Vec<_> = disassemble(&[0x01, 0x60]).collect();

    assert_eq!(instructions.len(), 2, "the trailing PUSH1 is still an instruction");
    assert_eq!(instructions[1].mnemonic(), "PUSH1");
    assert!(instructions[1].truncated);
    assert_eq!(instructions[1].missing(), 1);
}

#[test]
fn a_byte_outside_the_table_decodes_without_stopping_the_scan() {
    // 0x0c is unassigned. Decoding has to carry on past it — the bytes after
    // an unknown opcode are still instructions.
    let instructions: Vec<_> = disassemble(&[0x0c, 0x01]).collect();

    assert_eq!(instructions.len(), 2);
    assert_eq!(instructions[0].mnemonic(), "INVALID");
    assert_eq!(instructions[0].opcode, 0x0c);
    assert!(instructions[0].spec.is_none());
    assert_eq!(instructions[1].mnemonic(), "ADD");
}

#[test]
fn the_designated_invalid_opcode_is_a_real_entry() {
    // 0xfe is INVALID by specification and is in the table; it should not be
    // confused with a byte that simply has no entry.
    let instructions: Vec<_> = disassemble(&[0xfe]).collect();

    assert!(instructions[0].spec.is_some(), "0xfe is a specified opcode");
}

// --- jump destinations ------------------------------------------------------

#[test]
fn jumpdests_finds_the_real_ones() {
    // JUMPDEST, ADD, JUMPDEST
    let found: Vec<_> = jumpdests(&[0x5b, 0x01, 0x5b]).collect();
    assert_eq!(found, vec![0, 2]);
}

#[test]
fn a_5b_inside_an_immediate_is_not_a_jumpdest() {
    // PUSH1 0x5b, JUMPDEST — the first 0x5b is data. A byte scan would report
    // offset 1, and jumping there aborts the frame.
    let code = [0x60, 0x5b, 0x5b];
    let found: Vec<_> = jumpdests(&code).collect();

    assert_eq!(found, vec![2], "only the one that is an instruction");
    assert!(!code[..2].is_empty(), "offset 1 really does hold a 0x5b byte");
    assert_eq!(code[1], 0x5b);
}

// --- rendering ---------------------------------------------------------------

#[test]
fn a_listing_reads_as_one_instruction_per_line() {
    let code = [0x60, 0x02, 0x5f, 0x01, 0x0c, 0x63, 0xaa];

    assert_eq!(
        listing(&code).to_string(),
        "0000  PUSH1 0x02\n\
         0002  PUSH0\n\
         0003  ADD\n\
         0004  INVALID 0x0c\n\
         0005  PUSH4 0xaa (truncated, 3 byte(s) missing)\n"
    );
}

// --- against the assembler ---------------------------------------------------

#[test]
fn decoding_the_assembler_output_recovers_what_was_emitted() {
    let mut a = Asm::new(Fork::Cancun);
    a.push(2u8)
        .push(0u8)
        .dup(3)
        .swap(16)
        .op("ADD")
        .op("JUMPDEST")
        .op("STOP");
    let code = a.finish().unwrap();

    assert_eq!(
        decode(&code)
            .into_iter()
            .map(|(mnemonic, _)| mnemonic)
            .collect::<Vec<_>>(),
        ["PUSH1", "PUSH0", "DUP3", "SWAP16", "ADD", "JUMPDEST", "STOP"]
    );
}

#[test]
fn minimal_width_code_reassembles_to_the_same_bytes() {
    // Only holds where every immediate is already as narrow as it can be —
    // `Asm::push` narrows, so a PUSH2 holding 0x0001 would come back a byte
    // shorter. That is documented, not a defect; this pins the cases that do
    // hold, which is enough to catch a mis-sized immediate.
    let mut a = Asm::new(Fork::Cancun);
    a.push(0xabu8)
        .push(0x1234u16)
        .push([0xff; 32])
        .push(0u8)
        .dup(1)
        .swap(2)
        .op("MSTORE")
        .op("JUMPDEST")
        .op("RETURN");
    let code = a.finish().unwrap();

    let mut round_tripped = Asm::new(Fork::Cancun);
    for instruction in disassemble(&code) {
        let spec = instruction.spec.expect("assembler emits only real opcodes");
        match op_form(spec) {
            OpForm::Simple => round_tripped.op(spec.mnemonic),
            OpForm::Push(_) => {
                let mut word = [0u8; 32];
                word[32 - instruction.immediate.len()..].copy_from_slice(instruction.immediate);
                round_tripped.push(word)
            }
            OpForm::Dup(n) => round_tripped.dup(n),
            OpForm::Swap(n) => round_tripped.swap(n),
        };
    }

    assert_eq!(round_tripped.finish().unwrap(), code);
}
