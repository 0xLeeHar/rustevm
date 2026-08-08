//! Declaration-versus-definition bookkeeping. The mistakes here are the ones
//! that would otherwise surface as a caller writing arguments into the wrong
//! frame slots.

use evm_isa::Fork;

use super::*;
use crate::ir::builder::FunctionBuilder;
use crate::ir::types::Type;

fn trivial(name: &str, sig: Signature) -> Function {
    let mut f = FunctionBuilder::new(name, sig);
    f.stop();
    f.finish().expect("builds")
}

#[test]
fn a_declaration_reserves_a_handle_a_name_and_a_signature() {
    let mut m = Module::new(Fork::Cancun);
    let sig = Signature::new([Type::U64], [Type::U64]);
    let func = m.declare("double", sig.clone());

    assert_eq!(m.name(func), "double");
    assert_eq!(m.signature(func), &sig);
    assert!(m.get(func).is_none(), "declared, not defined");
    assert_eq!(m.len(), 1);
}

#[test]
fn defining_a_body_whose_signature_disagrees_is_rejected() {
    let mut m = Module::new(Fork::Cancun);
    let func = m.declare("f", Signature::new([Type::U64], [Type::U64]));

    let body = trivial("f", Signature::new([Type::U8], [Type::U8]));

    assert!(matches!(m.define(func, body), Err(IrError::SignatureMismatch { .. })));
}

#[test]
fn defining_a_function_twice_is_rejected() {
    let mut m = Module::new(Fork::Cancun);
    let sig = Signature::new([], []);
    let func = m.declare("f", sig.clone());

    m.define(func, trivial("f", sig.clone())).unwrap();

    assert!(matches!(
        m.define(func, trivial("f", sig)),
        Err(IrError::AlreadyDefined { .. })
    ));
}

#[test]
fn a_module_with_a_declared_but_undefined_function_does_not_verify() {
    let mut m = Module::new(Fork::Cancun);
    let sig = Signature::new([], []);
    let defined = m.declare("defined", sig.clone());
    m.declare("missing", sig.clone());
    m.define(defined, trivial("defined", sig)).unwrap();
    m.set_entry(defined);

    assert!(matches!(m.verify(), Err(IrError::Undefined { .. })));
}

#[test]
fn a_module_with_no_entry_does_not_verify() {
    let mut m = Module::new(Fork::Cancun);
    let sig = Signature::new([], []);
    let func = m.declare("f", sig.clone());
    m.define(func, trivial("f", sig)).unwrap();

    assert_eq!(m.verify(), Err(IrError::NoEntry));
}

#[test]
fn a_complete_module_verifies() {
    let mut m = Module::new(Fork::Cancun);
    let sig = Signature::new([], []);
    let func = m.declare("main", sig.clone());
    m.define(func, trivial("main", sig)).unwrap();
    m.set_entry(func);

    assert_eq!(m.verify(), Ok(()));
    assert_eq!(m.entry(), Some(func));
}

#[test]
fn functions_are_listed_in_declaration_order() {
    // The stackifier emits bodies in this order, so it is part of the layout,
    // not an implementation detail.
    let mut m = Module::new(Fork::Cancun);
    let sig = Signature::new([], []);
    let first = m.declare("first", sig.clone());
    let second = m.declare("second", sig);

    assert_eq!(m.funcs().collect::<Vec<_>>(), [first, second]);
    assert!(m.has_func(second));
    assert!(!m.has_func(Func::new(2)));
}
