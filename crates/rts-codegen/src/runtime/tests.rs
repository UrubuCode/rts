//! The tests of `runtime/mod.rs`, apart so that file stops growing past its
//! ceiling: a child module, so `use super::*` reaches what it did inline.

use super::*;

#[test]
fn asking_twice_yields_one_function_not_two() {
    let mut funcs = FuncRegistry::new();
    let mut calls = RuntimeCalls::new();
    let first = calls.declare(&mut funcs, RuntimeOp::Add);
    let second = calls.declare(&mut funcs, RuntimeOp::Add);
    assert_eq!(
        first, second,
        "a FuncId carries no name, so two ids for one operation are two \
         different functions as far as the machine can tell"
    );
    assert_eq!(funcs.len(), 1);
}

#[test]
fn nothing_is_declared_until_something_asks() {
    let mut funcs = FuncRegistry::new();
    let mut calls = RuntimeCalls::new();
    calls.declare(&mut funcs, RuntimeOp::Add);
    // A program that never concatenates must not carry a relocation to the
    // string path, and a compilation that declared everything up front
    // would link every operation into every program.
    assert_eq!(calls.declared().count(), 1);
    assert_eq!(funcs.len(), 1);
}

#[test]
fn the_slot_is_the_discriminant_and_the_list_agrees_with_it() {
    // `declare` indexes by `op as usize`, so a variant added to the enum
    // without being added to ALL — or added to ALL out of order — would
    // write one operation's id into another's slot. Silent, and this is the
    // check that makes it loud.
    for (position, op) in RuntimeOp::ALL.iter().enumerate() {
        assert_eq!(*op as usize, position, "{op:?} is out of place in ALL");
    }
}

#[test]
fn to_boolean_returns_a_proven_boolean_because_a_branch_needs_one() {
    // The reason control flow could not be emitted before calls, stated as
    // a test rather than only as prose: the machine's `branch` refuses
    // anything but `Repr::Bool`, and this is the only route to one from a
    // tagged JavaScript value.
    assert_eq!(RuntimeOp::ToBoolean.signature().returns, vec![Repr::Bool]);
    assert_eq!(RuntimeOp::ToBoolean.signature().params, vec![UNPROVEN]);
}

#[test]
fn every_symbol_is_distinct() {
    let mut seen: Vec<&str> = RuntimeOp::ALL.iter().map(|op| op.symbol()).collect();
    seen.sort_unstable();
    let count = seen.len();
    seen.dedup();
    assert_eq!(
        seen.len(),
        count,
        "two operations claiming one symbol link to one function"
    );
}
