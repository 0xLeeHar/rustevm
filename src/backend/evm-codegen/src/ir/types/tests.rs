//! Repair is where correctness lives, so it gets a table.
//!
//! Every case here is read off the representation invariant in the module doc:
//! given a well-formed value of `from`, what has to happen for it to be a
//! well-formed value of `to`. Getting one of these wrong is a silent
//! miscompile, not a crash, which is why they are enumerated rather than
//! spot-checked.

use super::*;

// --- widening: the cases that should cost nothing -------------------------

#[test]
fn widening_to_the_same_signedness_needs_no_repair() {
    assert_eq!(Repair::for_convert(Type::U8, Type::U32), Repair::None);
    assert_eq!(Repair::for_convert(Type::I8, Type::I64), Repair::None);
    assert_eq!(Repair::for_convert(Type::U32, Type::U256), Repair::None);
}

#[test]
fn widening_an_unsigned_value_into_a_signed_one_needs_no_repair() {
    // `u8` guarantees bits 8..256 are zero, so bit 31 — the sign bit of the
    // `i32` — is one of the zeros it already promised.
    assert_eq!(Repair::for_convert(Type::U8, Type::I32), Repair::None);
}

#[test]
fn a_conversion_to_the_same_type_is_free() {
    assert_eq!(Repair::for_convert(Type::U64, Type::U64), Repair::None);
    assert_eq!(Repair::for_convert(Type::PTR, Type::PTR), Repair::None);
}

#[test]
fn a_pointer_converts_as_the_u32_it_is() {
    assert_eq!(Repair::for_convert(Type::PTR, Type::U32), Repair::None);
    assert_eq!(Repair::for_convert(Type::U32, Type::PTR), Repair::None);
    assert_eq!(Repair::for_convert(Type::U64, Type::PTR), Repair::Mask { bits: 32 });
}

// --- widening: the case that does not come free ---------------------------

#[test]
fn widening_a_signed_value_into_an_unsigned_one_masks_off_the_sign() {
    // -1 as `i8` is a full word of ones. As a `u32` it must be 0x0000_ffff_ffff
    // truncated to 32 bits, so the sign bits above 32 have to go.
    assert_eq!(Repair::for_convert(Type::I8, Type::U32), Repair::Mask { bits: 32 });
    assert_eq!(Repair::for_convert(Type::I32, Type::U64), Repair::Mask { bits: 64 });
}

// --- same width, different signedness -------------------------------------

#[test]
fn reinterpreting_an_unsigned_value_as_signed_sign_extends() {
    // `u8` 0xff and `i8` -1 are the same eight bits and different words.
    assert_eq!(Repair::for_convert(Type::U8, Type::I8), Repair::Sext { bits: 8 });
    assert_eq!(Repair::for_convert(Type::U64, Type::I64), Repair::Sext { bits: 64 });
}

#[test]
fn reinterpreting_a_signed_value_as_unsigned_masks() {
    assert_eq!(Repair::for_convert(Type::I8, Type::U8), Repair::Mask { bits: 8 });
    assert_eq!(Repair::for_convert(Type::I64, Type::U64), Repair::Mask { bits: 64 });
}

// --- narrowing ------------------------------------------------------------

#[test]
fn narrowing_takes_its_repair_from_the_destination_alone() {
    assert_eq!(Repair::for_convert(Type::U64, Type::U32), Repair::Mask { bits: 32 });
    assert_eq!(Repair::for_convert(Type::I64, Type::U32), Repair::Mask { bits: 32 });
    assert_eq!(Repair::for_convert(Type::U64, Type::I32), Repair::Sext { bits: 32 });
    assert_eq!(Repair::for_convert(Type::I64, Type::I32), Repair::Sext { bits: 32 });
}

// --- the full-word normalisation ------------------------------------------

#[test]
fn a_repair_to_the_full_word_width_is_no_repair_at_all() {
    // `AND 0xff..ff` is 33 bytes of PUSH32 to accomplish nothing. Normalising
    // here means no lowering path has to notice.
    assert_eq!(Repair::mask(256), Repair::None);
    assert_eq!(Repair::sext(256), Repair::None);
    assert_eq!(Repair::for_convert(Type::I256, Type::U256), Repair::None);
    assert_eq!(Repair::for_convert(Type::U256, Type::I256), Repair::None);
}

#[test]
fn narrowing_from_the_full_word_still_repairs() {
    assert_eq!(Repair::for_convert(Type::U256, Type::U64), Repair::Mask { bits: 64 });
    assert_eq!(Repair::for_convert(Type::U256, Type::I64), Repair::Sext { bits: 64 });
}

// --- types ----------------------------------------------------------------

#[test]
fn a_pointer_reports_itself_as_a_thirty_two_bit_unsigned() {
    assert_eq!(Type::PTR.bits(), 32);
    assert!(!Type::PTR.is_signed());
    assert_eq!(Type::PTR.as_int(), Type::U32.as_int());
}

#[test]
fn a_pointer_is_still_a_distinct_type_from_u32() {
    // They behave identically and compare unequal on purpose: `verify` uses
    // the distinction to catch arithmetic that treats an address as a number.
    assert_ne!(Type::PTR, Type::U32);
}

#[test]
fn a_width_outside_one_to_two_hundred_and_fifty_six_is_not_well_formed() {
    assert!(Type::U8.is_well_formed());
    assert!(Type::BOOL.is_well_formed());
    assert!(Type::U256.is_well_formed());
    assert!(!Type::uint(0).is_well_formed());
    assert!(!Type::uint(257).is_well_formed());
}

#[test]
fn types_render_as_they_do_in_a_dump() {
    assert_eq!(Type::U64.to_string(), "u64");
    assert_eq!(Type::I8.to_string(), "i8");
    assert_eq!(Type::BOOL.to_string(), "u1");
    assert_eq!(Type::PTR.to_string(), "ptr");
}

#[test]
fn a_signature_renders_its_returns_only_when_it_has_them() {
    assert_eq!(Signature::new([], []).to_string(), "()");
    assert_eq!(Signature::new([Type::U64], []).to_string(), "(u64)");
    assert_eq!(
        Signature::new([Type::U64, Type::U64], [Type::U64]).to_string(),
        "(u64, u64) -> u64"
    );
    assert_eq!(Signature::new([], [Type::U8, Type::BOOL]).to_string(), "() -> (u8, u1)");
}
