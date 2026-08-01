//! Execution tests — the ones `evm-codegen`'s byte-level tests cannot express.
//!
//! Everything here asserts on what the EVM *did*: what came back, what landed
//! in storage, what the gas cost, whether a jump found its `JUMPDEST`.

use evm_codegen::Asm;
use evm_isa::Fork;

use super::*;

/// Return the top of the stack as the call's 32-byte output.
fn ret_top(a: &mut Asm) {
    a.push(0u8) // MSTORE(offset = 0, value = <top>)
        .op("MSTORE")
        .push(32u8) // RETURN(offset = 0, size = 32)
        .push(0u8)
        .op("RETURN");
}

fn word_of(address: Address) -> [u8; 32] {
    U256::from_be_slice(address.as_slice()).to_be_bytes()
}

// --- the loop `evm-codegen` cannot close on its own ------------------------

#[test]
fn arithmetic_produces_the_right_answer() {
    let mut a = Asm::new(Fork::Cancun);
    a.push(2u8).push(3u8).op("ADD");
    ret_top(&mut a);

    let mut tk = Testkit::new(Fork::Cancun);
    assert_eq!(tk.run_asm(a).returned_u64(), 5);
}

#[test]
fn a_forward_jump_lands_where_the_label_was_bound() {
    // The assembler's own tests prove the PUSH2 holds the right offset. This
    // proves the EVM accepts that offset as a jump destination and resumes
    // there — the skipped ADD would turn the 1 into a 3.
    let mut a = Asm::new(Fork::Cancun);
    let end = a.label();
    a
        .push(1u8)
        .push_label(end)
        .op("JUMP")
        .push(2u8)
        .op("ADD").bind(end);

    ret_top(&mut a);

    let mut tk = Testkit::new(Fork::Cancun);
    assert_eq!(tk.run_asm(a).returned_u64(), 1);
}

#[test]
fn a_jump_to_a_non_jumpdest_halts() {
    // Offset 1 is the PUSH1 immediate, not an instruction.
    let mut a = Asm::new(Fork::Cancun);
    a.push(1u8).op("JUMP");

    let mut tk = Testkit::new(Fork::Cancun);
    let out = tk.run_asm(a);

    assert!(out.is_halt(), "expected a halt, got {out}");
    assert_eq!(out.output, Vec::<u8>::new(), "a halt returns nothing");
}

// --- fork gating ----------------------------------------------------------

#[test]
fn push0_runs_on_shanghai_and_halts_on_london() {
    // The assembler refuses to emit PUSH0 pre-Shanghai; this checks the other
    // half of that claim — that the byte really is invalid there, and that the
    // fork the Testkit was built with is the fork revm executed against.
    let push0 = [0x5f];

    let mut shanghai = Testkit::new(Fork::Shanghai);
    assert!(shanghai.run(&push0).is_success());

    let mut london = Testkit::new(Fork::London);
    assert!(london.run(&push0).is_halt());
}

// --- state ----------------------------------------------------------------

#[test]
fn sstore_survives_the_transaction() {
    let mut a = Asm::new(Fork::Cancun);
    a.push(42u8) // SSTORE(key = 7, value = 42)
        .push(7u8)
        .op("SSTORE")
        .op("STOP");

    let mut tk = Testkit::new(Fork::Cancun);
    assert!(tk.run_asm(a).is_success());
    assert_eq!(tk.storage(Testkit::CONTRACT, U256::from(7)), U256::from(42));
}

#[test]
fn sload_reads_seeded_storage() {
    let mut tk = Testkit::new(Fork::Cancun);
    tk.set_storage(Testkit::CONTRACT, U256::from(1), U256::from(99));

    let mut a = Asm::new(Fork::Cancun);
    a.push(1u8).op("SLOAD");
    ret_top(&mut a);

    let code = a.finish().unwrap();
    assert_eq!(tk.run(&code).returned_u64(), 99);
}

#[test]
fn calldata_reaches_the_contract() {
    let mut a = Asm::new(Fork::Cancun);
    a.push(0u8).op("CALLDATALOAD");
    ret_top(&mut a);
    let code = a.finish().unwrap();

    let mut calldata = [0u8; 32];
    calldata[24..].copy_from_slice(&0xdead_beefu64.to_be_bytes());

    let mut tk = Testkit::new(Fork::Cancun);
    assert_eq!(tk.run_with(&code, &calldata).returned_u64(), 0xdead_beef);
}

// --- reverts --------------------------------------------------------------

#[test]
fn a_bare_revert_has_no_reason() {
    let mut a = Asm::new(Fork::Cancun);
    a.push(0u8).push(0u8).op("REVERT");

    let mut tk = Testkit::new(Fork::Cancun);
    let out = tk.run_asm(a);

    assert!(out.is_revert());
    assert_eq!(out.revert_reason(), None);
}

#[test]
fn an_error_string_revert_decodes() {
    // `Error(string)` as solc lays it out: selector, offset to the tail,
    // length, then the bytes.
    let mut selector = [0u8; 32];
    selector[..4].copy_from_slice(&[0x08, 0xc3, 0x79, 0xa0]);
    let mut text = [0u8; 32];
    text[..4].copy_from_slice(b"nope");

    let mut a = Asm::new(Fork::Cancun);
    a.push(selector).push(0u8).op("MSTORE");
    a.push(32u64).push(4u8).op("MSTORE");
    a.push(4u64).push(36u8).op("MSTORE");
    a.push(text).push(68u8).op("MSTORE");
    a.push(100u8).push(0u8).op("REVERT"); // REVERT(offset = 0, size = 100)

    let mut tk = Testkit::new(Fork::Cancun);
    let out = tk.run_asm(a);

    assert!(out.is_revert(), "{out}");
    assert_eq!(out.revert_reason().as_deref(), Some("nope"));
}

// --- deployment -----------------------------------------------------------

#[test]
fn deployed_code_is_the_runtime_code_and_runs() {
    let mut a = Asm::new(Fork::Cancun);
    a.push(5u8);
    ret_top(&mut a);
    let runtime = a.finish().unwrap();

    let mut tk = Testkit::new(Fork::Cancun);
    let deployed = tk.deploy_runtime(&runtime);
    let address = deployed.created_address();

    assert_eq!(tk.code(address), runtime, "CODECOPY copied the wrong bytes");
    assert_eq!(tk.call(address, &[]).returned_u64(), 5);
}

#[test]
fn the_deployer_prologue_handles_a_runtime_too_big_for_a_one_byte_push() {
    // The prologue's own length feeds the offset it pushes, which decides the
    // prologue's length. Anything past 255 bytes is where a non-converged
    // fixpoint would start copying from the wrong place.
    let mut a = Asm::new(Fork::Cancun);
    for _ in 0..300 {
        a.op("JUMPDEST");
    }
    a.push(5u8);
    ret_top(&mut a);
    let runtime = a.finish().unwrap();
    assert!(runtime.len() > 300);

    let mut tk = Testkit::new(Fork::Cancun);
    let address = tk.deploy_runtime(&runtime).created_address();

    assert_eq!(tk.code(address), runtime);
    assert_eq!(tk.call(address, &[]).returned_u64(), 5);
}

// --- tracing --------------------------------------------------------------

#[test]
fn a_trace_names_every_instruction_executed() {
    let mut a = Asm::new(Fork::Cancun);
    a.push(2u8).push(3u8).op("ADD");
    ret_top(&mut a);

    let mut tk = Testkit::new(Fork::Cancun).tracing();
    let out = tk.run_asm(a);
    let trace = out.trace.as_ref().expect("tracing was turned on");

    let mnemonics: Vec<_> = trace.steps.iter().map(|s| s.mnemonic).collect();
    assert_eq!(
        mnemonics,
        ["PUSH1", "PUSH1", "ADD", "PUSH0", "MSTORE", "PUSH1", "PUSH0", "RETURN"]
    );

    let add = &trace.steps[2];
    assert_eq!(add.gas_cost, 3, "ADD is 3 gas");
    assert_eq!(add.stack, vec![U256::from(2), U256::from(3)], "operands, deepest first");
}

#[test]
fn a_trace_is_absent_unless_asked_for() {
    let mut a = Asm::new(Fork::Cancun);
    a.op("STOP");

    let mut tk = Testkit::new(Fork::Cancun);
    assert!(tk.run_asm(a).trace.is_none());
}

const CALLEE: Address = address!("00000000000000000000000000000000000000b0");

/// A contract that `CALL`s [`CALLEE`] and returns what it returned.
fn traced_nested_call() -> Outcome {
    let mut callee = Asm::new(Fork::Cancun);
    callee.push(5u8);
    ret_top(&mut callee);

    // CALL(gas, address, value, argsOffset, argsSize, retOffset, retSize),
    // pushed in reverse so `gas` ends up on top.
    let mut caller = Asm::new(Fork::Cancun);
    caller
        .push(32u8)
        .push(0u8)
        .push(0u8)
        .push(0u8)
        .push(0u8)
        .push(word_of(CALLEE))
        .op("GAS")
        .op("CALL")
        .op("POP")
        .push(32u8) // return what the callee wrote to memory[0..32]
        .push(0u8)
        .op("RETURN");

    let mut tk = Testkit::new(Fork::Cancun).tracing();
    tk.set_code(CALLEE, &callee.finish().unwrap());
    tk.run_asm(caller)
}

#[test]
fn a_nested_call_is_traced_one_level_deeper() {
    let out = traced_nested_call();

    assert_eq!(out.returned_u64(), 5);
    let depths: Vec<_> = out.trace.as_ref().unwrap().steps.iter().map(|s| s.depth).collect();
    assert!(depths.contains(&0), "the outer frame");
    assert!(depths.contains(&1), "the callee's frame");
}

#[test]
fn a_call_is_charged_what_it_kept_not_what_it_forwarded() {
    // Measured naively, a CALL looks like it cost every bit of gas it handed
    // to the child, because the child is still holding it when the
    // instruction ends. What it cost the caller is the gap to the caller's
    // next instruction.
    let out = traced_nested_call();
    let steps = &out.trace.as_ref().unwrap().steps;

    let at = steps.iter().position(|s| s.mnemonic == "CALL").expect("a CALL");
    let resumed = steps[at + 1..]
        .iter()
        .find(|s| s.depth == steps[at].depth)
        .expect("the caller resumes after the call");

    assert_eq!(
        steps[at].gas_cost,
        steps[at].gas_remaining - resumed.gas_remaining,
        "a CALL costs the caller the gap either side of it"
    );
    assert!(
        steps[at].gas_cost < 10_000,
        "a cold CALL is ~2600 gas, not the {} forwarded",
        steps[at].gas_cost
    );
}

#[test]
fn a_call_to_a_codeless_account_is_charged_correctly_too() {
    // There is no child frame to observe here — nothing to execute — so the
    // cost has to come from knowing CALL opens a frame, not from noticing one.
    const CODELESS: Address = address!("00000000000000000000000000000000000000e0");

    let mut a = Asm::new(Fork::Cancun);
    a.push(0u8)
        .push(0u8)
        .push(0u8)
        .push(0u8)
        .push(0u8)
        .push(word_of(CODELESS))
        .op("GAS")
        .op("CALL")
        .op("POP")
        .op("STOP");

    let mut tk = Testkit::new(Fork::Cancun).tracing();
    let out = tk.run_asm(a);
    let steps = &out.trace.as_ref().unwrap().steps;

    assert!(
        steps.iter().all(|s| s.depth == 0),
        "an account with no code runs no instructions"
    );

    let at = steps.iter().position(|s| s.mnemonic == "CALL").expect("a CALL");
    assert_eq!(
        steps[at].gas_cost,
        steps[at].gas_remaining - steps[at + 1].gas_remaining,
        "a CALL costs the caller the gap either side of it"
    );
    assert!(steps[at].gas_cost < 10_000, "got {}", steps[at].gas_cost);
}

// --- assertion ergonomics -------------------------------------------------

#[test]
#[should_panic(expected = "expected success")]
fn asserting_on_the_output_of_a_failed_call_reports_the_failure() {
    let mut a = Asm::new(Fork::Cancun);
    a.push(0u8).push(0u8).op("REVERT");

    let mut tk = Testkit::new(Fork::Cancun);
    tk.run_asm(a).returned_u64();
}

#[test]
#[should_panic(expected = "does not fit in u64")]
fn a_value_too_wide_for_u64_is_not_silently_truncated() {
    let mut a = Asm::new(Fork::Cancun);
    a.push([0xffu8; 32]);
    ret_top(&mut a);

    let mut tk = Testkit::new(Fork::Cancun);
    tk.run_asm(a).returned_u64();
}
