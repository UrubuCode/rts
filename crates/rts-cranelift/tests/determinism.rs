//! Compiling one program twice produces the same thing — including in parallel.
//!
//! # Why this file exists now and not before
//!
//! Rule 13 has always required determinism: an id or a layout that depended on
//! iteration order would differ between two compilations of one program, and a
//! shape tree built from it would disagree with the code that reads it. Until
//! functions began compiling concurrently, nothing here could plausibly violate
//! it — the work was one thread in source order.
//!
//! Concurrency makes the requirement breakable and testable in the same change,
//! which is when a test earns its place. What could break it is not compilation
//! itself — `Context::compile` is a pure function of a context and an ISA — but
//! anything that hands out a NUMBER while a worker runs. The split is built so
//! nothing does: everything that assigns an id stays serial, and the definitions
//! are replayed in the order they were prepared.
//!
//! # Why every fixture here is large
//!
//! The parallel path is taken only past a threshold, so a two-function program
//! would exercise the serial path twice and prove nothing about the one at risk.

use rts_cranelift::ir::inst::NumOp;
use rts_cranelift::ir::{ConstDecl, FuncBuilder, FuncRegistry, Function, ScalarBits, Signature};
use rts_cranelift::repr::Repr;
use cranelift_module::Linkage;
use rts_cranelift::target::{MachineModule, executable_memory};
use rts_cranelift::types::TypeRegistry;

/// A function that adds a distinct constant to its argument.
///
/// Distinct per index on purpose: a program of identical functions would agree
/// whatever order it was compiled in, which is the one thing this must not
/// accidentally prove.
fn adder(types: &TypeRegistry, addend: u64) -> Function {
    let mut func = Function::new(Signature {
        params: vec![Repr::I64],
        returns: vec![Repr::I64],
        ..Signature::default()
    });
    let argument = func.block(func.entry).expect("entry").params[0];
    let entry = func.entry;

    let mut builder = FuncBuilder::new(&mut func, types, entry);
    let constant = builder.declare_const(ConstDecl::Scalar {
        repr: Repr::I64,
        bits: ScalarBits(addend),
    });
    let constant = builder.use_const(constant);
    let sum = builder
        .arith(NumOp::Add, argument, constant)
        .expect("two proven integers add");
    builder.ret(&[sum]);
    func
}

/// Compiles a program of `count` functions and reports what each became.
///
/// The name and the size of the emitted code per function, sorted by name — so
/// that this test does not itself depend on an iteration order, which would make
/// it pass for the wrong reason.
fn compiled(count: usize) -> Vec<(String, usize)> {
    let types = TypeRegistry::new();
    let mut funcs = FuncRegistry::new();
    let shape = funcs.declare_signature(Signature {
        params: vec![Repr::I64],
        returns: vec![Repr::I64],
        ..Signature::default()
    });

    let bodies: Vec<(rts_cranelift::ir::FuncId, Function)> = (0..count)
        .map(|index| {
            (
                funcs.declare_function(shape),
                adder(&types, index as u64 + 1),
            )
        })
        .collect();

    let mut jit = executable_memory().expect("this machine hosts its own code");
    let placements = {
        let mut module = MachineModule::new(&mut jit);
        for (index, (id, _)) in bodies.iter().enumerate() {
            module
                .declare(*id, &format!("adder{index}"), Linkage::Export, &funcs)
                .expect("declared");
        }
        // `prepare` every function first, then hand the whole batch to
        // `compile_and_define` — which is what the parallel path needs and what
        // `define` per function would never reach. `define` prepares one and
        // compiles a batch of one, so it is always below the threshold: a test
        // written through it would have exercised the serial path as many times
        // as there are functions and proved nothing about the other one.
        //
        // This is the shape `place_in_memory` uses, and using it here is what
        // makes the assertion below about the path that is actually at risk.
        let prepared: Vec<_> = bodies
            .iter()
            .map(|(id, func)| {
                module
                    .prepare(*id, func, &funcs, &types)
                    .expect("prepared")
            })
            .collect();
        module.compile_and_define(prepared).expect("compiled");
        module.into_placements()
    };

    let mut recorded: Vec<(String, usize)> = bodies
        .iter()
        .filter_map(|(id, _)| {
            placements
                .emitted(*id)
                .map(|(name, size)| (name.to_owned(), size))
        })
        .collect();
    recorded.sort();
    recorded
}

#[test]
fn a_program_compiled_twice_produces_the_same_code() {
    let first = compiled(20);
    let second = compiled(20);
    assert_eq!(first.len(), 20, "every function was emitted");
    assert!(first.iter().all(|(_, size)| *size > 0), "each has code");
    assert_eq!(
        first, second,
        "two compilations of one program disagreed — rule 13"
    );
}

#[test]
fn the_size_of_a_function_does_not_depend_on_how_many_it_was_compiled_with() {
    // Below the threshold the work is serial; above it, on a pool. The same
    // function must compile to the same code either way, which is what makes the
    // threshold a performance decision rather than a semantic one.
    let small = compiled(4);
    let large = compiled(20);
    for (name, size) in &small {
        let found = large
            .iter()
            .find(|(other, _)| other == name)
            .map(|(_, size)| *size);
        assert_eq!(
            Some(*size),
            found,
            "{name} became a different size in a larger program"
        );
    }
}

#[test]
fn a_repeated_compilation_is_stable_over_many_rounds() {
    // Once is luck. A scheduler that happened to finish in order twice would
    // pass the first test and fail the day the machine was busy, which is the
    // failure a determinism test exists to catch before a user does.
    let reference = compiled(16);
    for round in 0..8 {
        assert_eq!(reference, compiled(16), "round {round} disagreed");
    }
}

#[test]
fn one_function_still_compiles() {
    // The threshold keeps this serial, and it is the shape every other test in
    // this crate compiles — so a split that broke it would have broken them.
    // Said on purpose rather than relied on by coincidence.
    let only = compiled(1);
    assert_eq!(only.len(), 1);
    assert!(only[0].1 > 0, "it produced code");
}
