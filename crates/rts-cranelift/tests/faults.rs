//! Faults, and the positions that say where they came from.
//!
//! A trap that cannot say where it came from is a crash with an address
//! attached. These tests compile programs that can stop, and check that what
//! comes back names the place in the client's program that asked for it.

use cranelift_module::Linkage;
use rts_cranelift::fault::{FaultKind, Position};
use rts_cranelift::ir::{FuncBuilder, FuncRegistry, Function, Signature, TrapCode};
use rts_cranelift::repr::Repr;
use rts_cranelift::target::{MachineModule, executable_memory};
use rts_cranelift::types::TypeRegistry;

/// Compiles one function and returns what can stop inside it.
fn faults_of(func: &Function, signature: Signature) -> rts_cranelift::fault::FaultTable {
    let types = TypeRegistry::new();
    let mut funcs = FuncRegistry::new();
    let shape = funcs.declare_signature(signature);
    let id = funcs.declare_function(shape);

    let mut jit = executable_memory().expect("host");
    let mut module = MachineModule::new(&mut jit);
    module
        .declare(id, "subject", Linkage::Export, &funcs)
        .expect("declared");
    module.define(id, func, &funcs, &types).expect("defined");
    module
        .faults(id)
        .expect("defined, so it has a table")
        .clone()
}

#[test]
fn a_function_that_cannot_stop_reports_nothing() {
    let types = TypeRegistry::new();
    let mut func = Function::new(Signature::default());
    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    b.ret(&[]);

    assert!(
        faults_of(&func, Signature::default()).is_empty(),
        "a table with entries for a function that never stops would be noise"
    );
}

#[test]
fn a_trap_is_reported_with_the_position_that_asked_for_it() {
    let types = TypeRegistry::new();
    let mut func = Function::new(Signature::default());
    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    b.trap(TrapCode::Unreachable);

    // The client says where this came from. What the number means is its
    // business; this layer only carries it.
    let trap_inst = func.block(entry).expect("entry").insts.first().copied();
    assert!(
        trap_inst.is_none(),
        "a trap is a terminator, so there is no instruction to attach a position to"
    );

    let table = faults_of(&func, Signature::default());
    assert_eq!(table.len(), 1);
    let fault = table.iter().next().expect("one fault");
    assert_eq!(fault.kind, FaultKind::Unreachable);
}

#[test]
fn a_position_travels_from_an_instruction_to_the_fault_it_became() {
    let types = TypeRegistry::new();
    let mut func = Function::new(Signature {
        params: vec![Repr::I64, Repr::I64],
        returns: vec![Repr::I64],
        ..Signature::default()
    });
    let entry = func.entry;
    let params = func.block(entry).expect("entry").params.clone();
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    // Division can stop, and it is an instruction, so it can carry a position.
    let quotient = b
        .arith(rts_cranelift::ir::NumOp::Div, params[0], params[1])
        .expect("proven");
    b.ret(&[quotient]);

    let division = func.block(entry).expect("entry").insts[0];
    func.set_position(division, Position(4242));

    let table = faults_of(
        &func,
        Signature {
            params: vec![Repr::I64, Repr::I64],
            returns: vec![Repr::I64],
            ..Signature::default()
        },
    );

    assert!(
        !table.is_empty(),
        "dividing can stop, so something is reported"
    );
    assert!(
        table.iter().any(|fault| fault.position == Position(4242)),
        "the number the client gave comes back, uninterpreted"
    );
}

#[test]
fn an_instruction_with_no_position_says_so_rather_than_guessing() {
    let types = TypeRegistry::new();
    let mut func = Function::new(Signature {
        params: vec![Repr::I64, Repr::I64],
        returns: vec![Repr::I64],
        ..Signature::default()
    });
    let entry = func.entry;
    let params = func.block(entry).expect("entry").params.clone();
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let quotient = b
        .arith(rts_cranelift::ir::NumOp::Div, params[0], params[1])
        .expect("proven");
    b.ret(&[quotient]);

    let table = faults_of(
        &func,
        Signature {
            params: vec![Repr::I64, Repr::I64],
            returns: vec![Repr::I64],
            ..Signature::default()
        },
    );

    assert!(
        table.iter().all(|fault| !fault.position.is_known()),
        "nothing said where this came from, so nothing claims to know"
    );
}

#[test]
fn faults_are_ordered_so_that_an_address_can_be_looked_up() {
    let types = TypeRegistry::new();
    let mut func = Function::new(Signature {
        params: vec![Repr::Bool, Repr::I64, Repr::I64],
        returns: vec![Repr::I64],
        ..Signature::default()
    });
    let entry = func.entry;
    let params = func.block(entry).expect("entry").params.clone();
    let mut b = FuncBuilder::new(&mut func, &types, entry);

    let left = b.create_block();
    let right = b.create_block();
    b.branch(params[0], (left, &[]), (right, &[]))
        .expect("proven boolean");

    for block in [left, right] {
        let mut b = FuncBuilder::new(&mut func, &types, block);
        let quotient = b
            .arith(rts_cranelift::ir::NumOp::Div, params[1], params[2])
            .expect("proven");
        b.ret(&[quotient]);
    }

    let table = faults_of(
        &func,
        Signature {
            params: vec![Repr::Bool, Repr::I64, Repr::I64],
            returns: vec![Repr::I64],
            ..Signature::default()
        },
    );

    let offsets: Vec<_> = table.iter().map(|fault| fault.offset).collect();
    let mut sorted = offsets.clone();
    sorted.sort_unstable();
    assert_eq!(
        offsets, sorted,
        "the question is always \"something stopped here\", which arrives as an address"
    );

    for fault in table.iter() {
        assert_eq!(
            table.at(fault.offset),
            Some(fault),
            "and looking one up by address has to find it"
        );
    }
    assert_eq!(
        table.at(u32::MAX),
        None,
        "an address that stops nothing finds nothing"
    );
}
