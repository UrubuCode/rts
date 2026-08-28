//! Attribution, checked against real compiled code.
//!
//! Two questions, both arriving as an address: which part of the program is this,
//! and which function is it in. These tests compile something, ask both, and
//! check the answers name what was actually asked for.

use cranelift_jit::JITModule;
use cranelift_module::Linkage;
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

/// Compiles a function, finalizes it, and puts it on a map where it landed.
fn compile_and_place(name: &str, func: &Function) -> (CodeMap, usize) {
    let types = TypeRegistry::new();
    let mut funcs = FuncRegistry::new();
    let shape = funcs.declare_signature(Signature {
        params: vec![Repr::I64],
        returns: vec![Repr::I64],
        ..Signature::default()
    });
    let id = funcs.declare_function(shape);

    let mut jit = executable_memory().expect("host");
    let (machine_id, placements) = {
        let mut module = MachineModule::new(&mut jit);
        module
            .declare(id, name, Linkage::Export, &funcs)
            .expect("declared");
        module.define(id, func, &funcs, &types).expect("defined");
        let machine_id = module.declarations().machine_id(id).expect("declared");
        (machine_id, module.into_placements())
    };

    // Nothing has an address until this happens, which is why placing is a
    // separate step and why what it needs is handed over rather than asked for.
    jit.finalize_definitions().expect("finalized");
    let address = jit.get_finalized_function(machine_id) as usize;
    std::mem::forget(jit);

    let mut map = CodeMap::new();
    assert!(
        placements.place(&mut map, id, address),
        "a defined function has code to place"
    );
    (map, address)
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
fn a_return_address_is_said_back_as_a_function_and_a_place() {
    let func = three_additions([11, 22, 33]);
    let (mut map, address) = compile_and_place("named", &func);

    let attribution = map.attribute(address).expect("the entry is inside it");
    assert_eq!(
        attribution.function, "named",
        "a stack trace can name the frame"
    );
    assert_eq!(attribution.offset, 0);

    // Somewhere inside, both halves answer together — which is the only
    // question anyone actually has.
    let inside = (0..64)
        .filter_map(|step| map.attribute(address + step))
        .find(|found| found.position.is_known())
        .expect("some address in it came from somewhere");
    assert!(
        [11, 22, 33].contains(&inside.position.raw()),
        "and the place it names is one the program was built from"
    );

    assert!(
        map.attribute(address - 1).is_none(),
        "just before it is not inside it, and saying otherwise names the wrong frame"
    );
    let _ = &mut map;
}

#[test]
fn a_function_that_was_declared_and_never_defined_is_not_placed() {
    let types = TypeRegistry::new();
    let mut funcs = FuncRegistry::new();
    let shape = funcs.declare_signature(Signature::default());
    let id = funcs.declare_function(shape);

    let mut jit = executable_memory().expect("host");
    let placements = {
        let mut module = MachineModule::new(&mut jit);
        module
            .declare(id, "promised", Linkage::Export, &funcs)
            .expect("declared");
        module.into_placements()
    };
    let _ = types;

    let mut map = CodeMap::new();
    assert!(
        !placements.place(&mut map, id, 0x1000),
        "it has a name and no code, and mapping zero bytes of it would put a hole \n         in the map at a real address"
    );
    assert!(map.is_empty());
    std::mem::forget(jit);
}

/// The map a whole placed program carries, which nothing built until now.
///
/// The tests above prove `CodeMap` works when a test assembles one by hand.
/// That is what it did for as long as it existed: `MachineModule::place`,
/// `CodeMap` and `PositionMap` were complete and tested, and every caller of
/// any of them was in this file. So the map a RUNNING program would need was
/// never built, and `entry/throw.rs` says what that cost — a stack trace with
/// no source position, because "nothing maps an address back to one at run
/// time".
///
/// This pins the other half: `place_in_memory` builds the map itself, so an
/// address taken out of a real placement answers with the function it is in.
#[test]
fn a_placed_program_carries_a_map_of_itself() {
    use rts_cranelift::target::{Placing, Visibility, place_in_memory};

    let types = TypeRegistry::new();
    let mut funcs = FuncRegistry::new();
    let shape = funcs.declare_signature(Signature {
        params: vec![Repr::I64],
        returns: vec![Repr::I64],
        ..Signature::default()
    });
    let id = funcs.declare_function(shape);
    let body = three_additions([70, 71, 72]);

    let placing = [Placing {
        id,
        name: "mapped",
        visibility: Visibility::Exported,
        body: Some(&body),
    }];

    // SAFETY: nothing is expected from outside, so there is no address whose
    // signature this test could get wrong.
    let placed = unsafe { place_in_memory(&placing, &[], &funcs, &types, None) }.expect("placed");

    let address = placed.address_of(id).expect("defined") as usize;
    let map = placed.code_map();
    assert_eq!(map.len(), 1, "one function was placed, so one range is mapped");

    let at_entry = map.attribute(address).expect("the entry address is in the program");
    assert_eq!(
        at_entry.function, "mapped",
        "an address inside a placed function names that function"
    );
    assert_eq!(
        at_entry.offset, 0,
        "the entry address is the start of its function"
    );

    // A byte past the end belongs to nothing, which is the property that keeps
    // a walk from naming a neighbour for a return address that left the program.
    let (_, length) = map
        .iter()
        .next()
        .map(|range| (range.name.clone(), range.length))
        .expect("one range");
    assert!(
        map.attribute(address + length).is_none(),
        "an address past the end of the only function is attributed to nothing"
    );
}
