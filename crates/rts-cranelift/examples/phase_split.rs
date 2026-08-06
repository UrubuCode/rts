//! Where compile time actually goes: our lowering, or the code generator's.
//!
//! # Why this has to be measured before anything is redesigned
//!
//! Functions now compile on a pool, but only `Context::compile` does — the
//! `prepare` phase, which is *our* lowering, stays serial because it hands out
//! ids in reach order. Making that phase parallel too is possible and is real
//! work, and it is worth exactly its share of the total.
//!
//! If the code generator is ninety-five percent of it, parallelising lowering
//! buys almost nothing and the honest answer is to say so. If lowering is a
//! third, it is the next thing to do. Neither is knowable by reading, and the
//! last three diagnoses in this repository that were made by reading were wrong.
//!
//! Run with `cargo run --release --example phase_split -p rts-cranelift`.

use cranelift_module::Linkage;
use std::time::Instant;

use rts_cranelift::ir::inst::NumOp;
use rts_cranelift::ir::{ConstDecl, FuncBuilder, FuncRegistry, Function, ScalarBits, Signature};
use rts_cranelift::repr::Repr;
use rts_cranelift::target::{MachineModule, executable_memory};
use rts_cranelift::types::TypeRegistry;

fn main() {
    if cfg!(debug_assertions) {
        println!("DEBUG — these are not numbers");
    }
    println!("phase split, {} functions per program\n", COUNT);

    for width in [1usize, 8, 32] {
        measure(width);
    }
}

/// How many functions each program has.
const COUNT: usize = 200;

/// A function of `width` additions, so that a bigger body can be asked for
/// without changing how many functions there are.
fn body(types: &TypeRegistry, width: usize) -> Function {
    let mut func = Function::new(Signature {
        params: vec![Repr::I64],
        returns: vec![Repr::I64],
        ..Signature::default()
    });
    let argument = func.block(func.entry).expect("entry").params[0];
    let entry = func.entry;

    let mut builder = FuncBuilder::new(&mut func, types, entry);
    let mut running = argument;
    for step in 0..width {
        let constant = builder.declare_const(ConstDecl::Scalar {
            repr: Repr::I64,
            bits: ScalarBits(step as u64 + 1),
        });
        let constant = builder.use_const(constant);
        running = builder
            .arith(NumOp::Add, running, constant)
            .expect("two proven integers add");
    }
    builder.ret(&[running]);
    func
}

/// Times the two phases apart, for a program of `COUNT` functions.
fn measure(width: usize) {
    let types = TypeRegistry::new();
    let mut funcs = FuncRegistry::new();
    let shape = funcs.declare_signature(Signature {
        params: vec![Repr::I64],
        returns: vec![Repr::I64],
        ..Signature::default()
    });
    let bodies: Vec<(rts_cranelift::ir::FuncId, Function)> = (0..COUNT)
        .map(|_| (funcs.declare_function(shape), body(&types, width)))
        .collect();

    let mut jit = executable_memory().expect("this machine hosts its own code");
    let mut module = MachineModule::new(&mut jit);
    for (index, (id, _)) in bodies.iter().enumerate() {
        module
            .declare(*id, &format!("f{index}"), Linkage::Export, &funcs)
            .expect("declared");
    }

    // The serial half: our lowering, plus everything that assigns an id.
    let started = Instant::now();
    let prepared: Vec<_> = bodies
        .iter()
        .map(|(id, func)| module.prepare(*id, func, &funcs, &types).expect("prepared"))
        .collect();
    let preparing = started.elapsed();

    // The parallel half: the code generator.
    let started = Instant::now();
    module.compile_and_define(prepared).expect("compiled");
    let compiling = started.elapsed();

    let total = preparing + compiling;
    let share = preparing.as_secs_f64() / total.as_secs_f64() * 100.0;
    println!(
        "  {width:>3} additions each: prepare {:>7.2} ms ({share:>4.1}%)   compile {:>7.2} ms   total {:>7.2} ms",
        preparing.as_secs_f64() * 1000.0,
        compiling.as_secs_f64() * 1000.0,
        total.as_secs_f64() * 1000.0,
    );

    // Consumed, so that a phase optimised away would show as a number too good
    // to be true rather than as a fast one.
    let placements = module.into_placements();
    let placed = placements.defined().count();
    assert_eq!(placed, COUNT, "every function was compiled");
}
