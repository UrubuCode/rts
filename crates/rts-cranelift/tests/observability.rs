//! Attribution, checked against real compiled code.
//!
//! Two questions, both arriving as an address: which part of the program is this,
//! and which function is it in. These tests compile something, ask both, and
//! check the answers name what was actually asked for.

use cranelift_jit::JITModule;
use cranelift_module::{Linkage, Module};
use rts_cranelift::fault::Position;
use rts_cranelift::ir::{FuncBuilder, FuncRegistry, Function, NumOp, Signature};
use rts_cranelift::observe::CodeMap;
use rts_cranelift::repr::Repr;
use rts_cranelift::target::{MachineModule, executable_memory};
use rts_cranelift::types::TypeRegistry;

/// Builds a function of three additions, each said to come from somewhere.
fn three_additions(positions: [u32; 3]) -> Function {
    let types = TypeRegistry::new();
    let mut func = Function::new(Signature {
        params: vec![Repr::I64],
        returns: vec![Repr::I64],
        ..Signature::default()
    });
    let x = func.block(func.entry).expect("entry").params[0];
    let entry = func.entry;

    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let a = b.arith(NumOp::Add, x, x).expect("proven");
    let c = b.arith(NumOp::Add, a, a).expect("proven");
    let d = b.arith(NumOp::Add, c, c).expect("proven");
    b.ret(&[d]);

    let insts = func.block(entry).expect("entry").insts.clone();
    for (inst, position) in insts.iter().zip(positions) {
        func.set_position(*inst, Position(position));
    }
    func
}

/// Compiles a function and hands back the module and what was recorded about it.
fn compile(
    name: &str,
    func: &Function,
) -> (
    JITModule,
    cranelift_module::FuncId,
    rts_cranelift::observe::PositionMap,
) {
    let types = TypeRegistry::new();
    let mut funcs = FuncRegistry::new();
    let shape = funcs.declare_signature(Signature {
        params: vec![Repr::I64],
        returns: vec![Repr::I64],
        ..Signature::default()
    });
    let id = funcs.declare_function(shape);

    let mut jit = executable_memory().expect("host");
    let (machine_id, positions) = {
        let mut module = MachineModule::new(&mut jit);
        module
            .declare(id, name, Linkage::Export, &funcs)
            .expect("declared");
        module.define(id, func, &funcs, &types).expect("defined");
        (
            module.declarations().machine_id(id).expect("declared"),
            module.positions(id).expect("defined").clone(),
        )
    };
    jit.finalize_definitions().expect("finalized");
    (jit, machine_id, positions)
}

#[test]
fn every_position_a_program_was_built_from_comes_back() {
    let func = three_additions([11, 22, 33]);
    let (jit, _, positions) = compile("triple", &func);
    std::mem::forget(jit);

    let reported = positions.positions();
    for expected in [11, 22, 33] {
        assert!(
            reported.contains(&Position(expected)),
            "a place the program was built from went missing: {expected}"
        );
    }
}

#[test]
fn an_address_inside_the_code_says_where_it_came_from() {
    let func = three_additions([11, 22, 33]);
    let (jit, _, positions) = compile("triple", &func);
    std::mem::forget(jit);

    assert!(
        !positions.is_empty(),
        "lowering said where things came from"
    );

    // Every byte the map covers answers, and every answer is one of ours.
    let known = positions.positions();
    let mut answered = 0;
    for offset in 0..256u32 {
        if let Some(position) = positions.at(offset) {
            assert!(
                known.contains(&position),
                "an address answered with a position nothing asked for"
            );
            answered += 1;
        }
    }
    assert!(
        answered > 0,
        "some address in the function has to be attributable"
    );
}

#[test]
fn a_program_that_said_nothing_is_attributed_to_nothing() {
    let types = TypeRegistry::new();
    let mut func = Function::new(Signature {
        params: vec![Repr::I64],
        returns: vec![Repr::I64],
        ..Signature::default()
    });
    let x = func.block(func.entry).expect("entry").params[0];
    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let doubled = b.arith(NumOp::Add, x, x).expect("proven");
    b.ret(&[doubled]);

    let (jit, _, positions) = compile("silent", &func);
    std::mem::forget(jit);

    assert!(
        positions.positions().is_empty(),
        "nothing said where this came from, so nothing claims to know"
    );
}

#[test]
fn a_return_address_finds_the_function_it_is_in() {
    let func = three_additions([1, 2, 3]);
    let (jit, machine_id, _) = compile("named", &func);
    let address = jit.get_finalized_function(machine_id) as usize;
    std::mem::forget(jit);

    let mut map = CodeMap::new();
    map.record("named", address, 64);

    let (range, offset) = map.at(address + 8).expect("inside");
    assert_eq!(range.name, "named", "a stack trace can name the frame");
    assert_eq!(offset, 8, "and say how far into it the address was");

    assert!(
        map.at(address - 1).is_none(),
        "just before it is not inside it, and saying otherwise names the wrong frame"
    );
}
