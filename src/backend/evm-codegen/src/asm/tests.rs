//! Encoding and label-resolution tests — no EVM involved.
//!
//! These assert on the exact bytes, because the bytes are the contract with
//! everything downstream. `evm-testkit` covers whether those bytes *do* the
//! right thing when executed.

use super::*;

fn asm(fork: Fork) -> Asm {
    Asm::new(fork)
}

fn cancun() -> Asm {
    asm(Fork::Cancun)
}

// --- PUSH width ---------------------------------------------------------

#[test]
fn push_picks_the_narrowest_width() {
    let mut a = cancun();
    a.push(1u8);
    assert_eq!(a.finish().unwrap(), vec![0x60, 0x01], "PUSH1");

    let mut a = cancun();
    a.push(0x0100u16);
    assert_eq!(a.finish().unwrap(), vec![0x61, 0x01, 0x00], "PUSH2");

    let mut a = cancun();
    a.push(0xffu16);
    assert_eq!(a.finish().unwrap(), vec![0x60, 0xff], "0x00ff narrows to PUSH1");

    let mut a = cancun();
    a.push(0x0102_0304u32);
    assert_eq!(a.finish().unwrap(), vec![0x63, 0x01, 0x02, 0x03, 0x04], "PUSH4");
}

#[test]
fn push_of_a_full_word_is_push32() {
    let word = [0xabu8; 32];
    let mut a = cancun();
    a.push(word);
    let code = a.finish().unwrap();

    assert_eq!(code[0], 0x7f, "PUSH32");
    assert_eq!(&code[1..], &word[..]);
    assert_eq!(code.len(), 33);
}

#[test]
fn push_zero_is_push0_from_shanghai() {
    let mut a = cancun();
    a.push(0u8);
    assert_eq!(a.finish().unwrap(), vec![0x5f], "PUSH0");
}

#[test]
fn push_zero_falls_back_to_push1_before_shanghai() {
    // PUSH0 (EIP-3855) does not exist pre-Shanghai — emitting it would be an
    // invalid instruction, so the extra byte is not optional.
    let mut a = asm(Fork::London);
    a.push(0u8);
    assert_eq!(a.finish().unwrap(), vec![0x60, 0x00]);
}

// --- labels and jumps ---------------------------------------------------

#[test]
fn forward_jump_resolves_and_lands_on_a_jumpdest() {
    let mut a = cancun();
    let end = a.label();
    a.push_label(end).op("JUMP");
    let target = a.offset();
    a.bind(end).op("STOP");

    let code = a.finish().unwrap();

    // PUSH2 <target> JUMP JUMPDEST STOP
    assert_eq!(code[0], 0x61);
    assert_eq!(u16::from_be_bytes([code[1], code[2]]) as usize, target);
    assert_eq!(code[3], 0x56, "JUMP");
    assert_eq!(code[target], 0x5b, "the resolved address must hold a JUMPDEST");
}

#[test]
fn backward_jump_resolves() {
    let mut a = cancun();
    let top = a.label();
    a.bind(top); // JUMPDEST at offset 0
    a.push(1u8).op("POP"); // a body to jump back over
    a.push_label(top).op("JUMP");

    let code = a.finish().unwrap();

    // The trailing `PUSH2 <imm> JUMP` puts the immediate at len-3.
    let imm = code.len() - 3;
    assert_eq!(u16::from_be_bytes([code[imm], code[imm + 1]]), 0);
    assert_eq!(code[0], 0x5b, "the target must be a JUMPDEST");
}

#[test]
fn a_label_can_be_referenced_more_than_once() {
    let mut a = cancun();
    let l = a.label();
    a.push_label(l).op("POP").push_label(l).op("POP");
    a.bind(l).op("STOP");

    let code = a.finish().unwrap();
    let first = u16::from_be_bytes([code[1], code[2]]);
    let second = u16::from_be_bytes([code[5], code[6]]);
    assert_eq!(first, second);
    assert_eq!(code[first as usize], 0x5b);
}

#[test]
fn label_references_are_always_push2() {
    // Even at offset 3, which would fit in a PUSH1 — narrowing needs a
    // fixpoint, so the width is fixed. If this ever becomes variable, the
    // fixup patching in `finish` has to change with it.
    let mut a = cancun();
    let l = a.label();
    a.push_label(l).op("JUMP");
    a.bind(l);

    let code = a.finish().unwrap();
    // PUSH2 (3 bytes) + JUMP (1) puts the JUMPDEST at offset 4 — small enough
    // for a PUSH1, still emitted as a PUSH2.
    assert_eq!(code[0], 0x61, "PUSH2 regardless of how small the target is");
    assert_eq!(u16::from_be_bytes([code[1], code[2]]), 4);
    assert_eq!(code[4], 0x5b);
}

#[test]
fn unbound_label_is_an_error() {
    let mut a = cancun();
    let l = a.label();
    a.push_label(l).op("JUMP");

    assert_eq!(a.finish(), Err(AsmError::UnboundLabel(l)));
}

#[test]
fn binding_twice_is_an_error() {
    let mut a = cancun();
    let l = a.label();
    a.bind(l).bind(l);

    assert_eq!(a.finish(), Err(AsmError::LabelAlreadyBound(l)));
}

// --- rejections ---------------------------------------------------------

#[test]
fn unknown_mnemonic_is_an_error() {
    let mut a = cancun();
    a.op("SSTORE2");

    assert_eq!(a.finish(), Err(AsmError::UnknownMnemonic("SSTORE2".into())));
}

#[test]
fn opcode_newer_than_the_target_fork_is_an_error() {
    // TLOAD is Cancun (EIP-1153). Assembling it for Shanghai would produce
    // bytecode that reverts on-chain rather than failing here.
    let mut a = asm(Fork::Shanghai);
    a.op("TLOAD");

    assert_eq!(
        a.finish(),
        Err(AsmError::OpcodeTooNew {
            mnemonic: "TLOAD",
            min_fork: Fork::Cancun,
            target: Fork::Shanghai,
        })
    );
}

#[test]
fn the_same_opcode_assembles_on_a_late_enough_fork() {
    let mut a = cancun();
    a.op("TLOAD");
    assert_eq!(a.finish().unwrap(), vec![0x5c]);
}

#[test]
fn operand_carrying_opcodes_are_rejected_by_op() {
    // `op("PUSH1")` would emit 0x60 with no immediate, swallowing the next
    // opcode byte as data — corrupting everything after it, silently.
    for (mnemonic, helper) in [("PUSH1", "push"), ("DUP1", "dup"), ("SWAP1", "swap")] {
        let mut a = cancun();
        a.op(mnemonic);
        assert_eq!(
            a.finish(),
            Err(AsmError::UseHelperInstead { mnemonic, helper }),
            "{mnemonic} must route through `{helper}`"
        );
    }
}

#[test]
fn dup_and_swap_depths_are_bounded() {
    for n in [0u8, 17] {
        let mut a = cancun();
        a.dup(n);
        assert_eq!(a.finish(), Err(AsmError::BadStackDepth { helper: "dup", n }));

        let mut a = cancun();
        a.swap(n);
        assert_eq!(a.finish(), Err(AsmError::BadStackDepth { helper: "swap", n }));
    }
}

#[test]
fn dup_and_swap_encode_to_the_right_bytes() {
    let mut a = cancun();
    a.dup(1).dup(16).swap(1).swap(16);
    assert_eq!(a.finish().unwrap(), vec![0x80, 0x8f, 0x90, 0x9f]);
}

#[test]
fn the_first_error_is_the_one_reported() {
    // Errors are sticky and later emits are dropped, so a cascade doesn't
    // bury the actual mistake.
    let mut a = cancun();
    a.op("NOPE").dup(99).op("ALSO_NOPE");

    assert_eq!(a.finish(), Err(AsmError::UnknownMnemonic("NOPE".into())));
}

#[test]
fn nothing_is_emitted_after_an_error() {
    let mut a = cancun();
    a.push(1u8).op("NOPE").push(2u8);
    assert!(a.finish().is_err());
}

// --- misc ---------------------------------------------------------------

#[test]
fn offset_tracks_emitted_bytes() {
    let mut a = cancun();
    assert_eq!(a.offset(), 0);
    a.push(1u8); // PUSH1 01
    assert_eq!(a.offset(), 2);
    a.op("STOP");
    assert_eq!(a.offset(), 3);
}

#[test]
fn empty_program_assembles_to_empty_bytecode() {
    assert_eq!(cancun().finish().unwrap(), Vec::<u8>::new());
}

#[test]
fn deployability_reflects_eip170() {
    let mut a = cancun();
    assert!(a.is_deployable());
    for _ in 0..MAX_CONTRACT_SIZE + 1 {
        a.op("STOP");
    }
    assert!(!a.is_deployable());
    // ...but it still assembles: initcode and test fragments are not runtime
    // code, so the limit is the caller's to apply.
    assert!(a.finish().is_ok());
}
