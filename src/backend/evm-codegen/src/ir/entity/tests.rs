//! Handles are thin enough that the only things worth pinning are the round
//! trip and the rendering — the latter because it is part of the dump format.

use super::*;

#[test]
fn an_index_survives_the_round_trip() {
    assert_eq!(Value::new(0).index(), 0);
    assert_eq!(Value::new(7).index(), 7);
    assert_eq!(Block::new(u32::MAX as usize).index(), u32::MAX as usize);
}

#[test]
fn handles_render_the_way_a_dump_prints_them() {
    assert_eq!(Value::new(3).to_string(), "v3");
    assert_eq!(Inst::new(0).to_string(), "i0");
    assert_eq!(Block::new(12).to_string(), "block12");
    assert_eq!(Func::new(1).to_string(), "@f1");
}

#[test]
fn handles_of_different_kinds_are_different_types() {
    // Not a runtime assertion so much as a compile-time one: the arenas are
    // all `Vec`s indexed by `usize`, and only the newtypes stop a `Block`
    // being used to look up a `Value`.
    let value = Value::new(1);
    let block = Block::new(1);
    assert_eq!(value.index(), block.index());
    assert_ne!(value.to_string(), block.to_string());
}

#[test]
fn handles_are_ordered_by_index_so_they_can_key_a_sorted_map() {
    let mut values = [Value::new(3), Value::new(1), Value::new(2)];
    values.sort();
    assert_eq!(values, [Value::new(1), Value::new(2), Value::new(3)]);
}
