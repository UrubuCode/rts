//! Compiled code, actually run.
//!
//! Everything before this file checks that we produce something a verifier
//! accepts. This one checks that what we produce computes the right answer, by
//! compiling it into this process's memory and calling it. That is a different
//! claim, and it is the one that cannot be satisfied by being internally
//! consistent.

use cranelift_module::{Linkage, Module};
use rts_cranelift::ir::{
    CmpOp, ConstDecl, FuncBuilder, FuncRegistry, Function, NumOp, ScalarBits, Signature, ValueId,
};
use rts_cranelift::mem::{ObjectLayout, RegionBase, RegionBases};
use rts_cranelift::repr::{RefKind, Repr};
use rts_cranelift::target::{
    MachineModule, Placing, Visibility, executable_memory, object_file, place_in_object,
};
use rts_cranelift::types::TypeRegistry;

fn function(params: &[Repr], returns: &[Repr]) -> Function {
    Function::new(Signature {
        params: params.to_vec(),
        returns: returns.to_vec(),
        ..Signature::default()
    })
}

fn param(func: &Function, index: usize) -> ValueId {
    func.block(func.entry).expect("entry exists").params[index]
}

/// Compiles one function into memory and returns its address.
fn compile_one(
    name: &str,
    id: rts_cranelift::ir::FuncId,
    func: &Function,
    funcs: &FuncRegistry,
) -> *const u8 {
    let mut jit = executable_memory().expect("this machine can host its own code");
    let machine_id = {
        let mut module = MachineModule::new(&mut jit);
        module
            .declare(id, name, Linkage::Export, funcs)
            .expect("declaring succeeds");
        module
            .define(id, func, funcs, &TypeRegistry::new())
            .expect("defining succeeds");
        module.declarations().machine_id(id).expect("declared")
    };
    jit.finalize_definitions().expect("finalizing succeeds");
    let address = jit.get_finalized_function(machine_id);
    // The module owns the memory the code lives in, so it has to outlive the
    // pointer. Leaking it is what a test can honestly do; a real host keeps it.
    std::mem::forget(jit);
    address
}

#[test]
fn a_compiled_function_computes_what_it_was_built_to() {
    let types = TypeRegistry::new();
    let mut funcs = FuncRegistry::new();
    let shape = funcs.declare_signature(Signature {
        params: vec![Repr::I64, Repr::I64],
        returns: vec![Repr::I64],
        ..Signature::default()
    });
    let id = funcs.declare_function(shape);

    let mut func = function(&[Repr::I64, Repr::I64], &[Repr::I64]);
    let (x, y) = (param(&func, 0), param(&func, 1));
    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let sum = b.arith(NumOp::Add, x, y).expect("proven");
    b.ret(&[sum]);

    let address = compile_one("add", id, &func, &funcs);
    let add: extern "C" fn(i64, i64) -> i64 = unsafe { std::mem::transmute(address) };
    assert_eq!(add(2, 40), 42);
}

#[test]
fn a_branch_chooses_at_run_time_and_not_at_build_time() {
    let types = TypeRegistry::new();
    let mut funcs = FuncRegistry::new();
    let shape = funcs.declare_signature(Signature {
        params: vec![Repr::I64],
        returns: vec![Repr::I64],
        ..Signature::default()
    });
    let id = funcs.declare_function(shape);

    let mut func = function(&[Repr::I64], &[Repr::I64]);
    let x = param(&func, 0);
    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);

    let positive = b.create_block();
    let negative = b.create_block();
    let zero = b.declare_const(ConstDecl::Scalar {
        repr: Repr::I64,
        bits: ScalarBits(0),
    });
    let zero = b.use_const(zero);
    let is_negative = b.compare(CmpOp::Lt, x, zero).expect("proven");
    b.branch(is_negative, (negative, &[]), (positive, &[]))
        .expect("proven boolean");

    let mut b = FuncBuilder::new(&mut func, &types, negative);
    let negated = b.arith(NumOp::Sub, zero, x).expect("proven");
    b.ret(&[negated]);

    let mut b = FuncBuilder::new(&mut func, &types, positive);
    b.ret(&[x]);

    let address = compile_one("abs", id, &func, &funcs);
    let abs: extern "C" fn(i64) -> i64 = unsafe { std::mem::transmute(address) };
    assert_eq!(abs(-7), 7);
    assert_eq!(abs(7), 7);
}

#[test]
fn a_widened_value_survives_being_narrowed_back() {
    let types = TypeRegistry::new();
    let mut funcs = FuncRegistry::new();
    let shape = funcs.declare_signature(Signature {
        params: vec![Repr::I32],
        returns: vec![Repr::I32],
        ..Signature::default()
    });
    let id = funcs.declare_function(shape);

    let mut func = function(&[Repr::I32], &[Repr::I32]);
    let x = param(&func, 0);
    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);

    let ok = b.create_block();
    let fail = b.create_block();
    b.add_block_param(ok, Repr::I32);
    let widened = b.widen(x);
    b.guard(widened, Repr::I32, (ok, &[]), (fail, &[]))
        .expect("well formed");

    let narrowed = func.block(ok).expect("exists").params[0];
    let mut b = FuncBuilder::new(&mut func, &types, ok);
    b.ret(&[narrowed]);

    // Unreachable in fact — the value was just widened from an integer — but the
    // guard's failure path is not optional, which is the point.
    let mut b = FuncBuilder::new(&mut func, &types, fail);
    let zero = b.declare_const(ConstDecl::Scalar {
        repr: Repr::I32,
        bits: ScalarBits(0),
    });
    let zero = b.use_const(zero);
    b.ret(&[zero]);

    let address = compile_one("roundtrip", id, &func, &funcs);
    let roundtrip: extern "C" fn(i32) -> i32 = unsafe { std::mem::transmute(address) };

    for value in [0, 1, -1, i32::MAX, i32::MIN, 12345] {
        assert_eq!(
            roundtrip(value),
            value,
            "the encoding has to survive a round trip, including at the extremes"
        );
    }
}

#[test]
fn a_call_reaches_the_function_it_named() {
    let types = TypeRegistry::new();
    let mut funcs = FuncRegistry::new();
    let shape = funcs.declare_signature(Signature {
        params: vec![Repr::I64],
        returns: vec![Repr::I64],
        ..Signature::default()
    });
    let callee_id = funcs.declare_function(shape);
    let caller_id = funcs.declare_function(shape);

    // The callee doubles its argument.
    let mut callee = function(&[Repr::I64], &[Repr::I64]);
    let x = param(&callee, 0);
    let entry = callee.entry;
    let mut b = FuncBuilder::new(&mut callee, &types, entry);
    let doubled = b.arith(NumOp::Add, x, x).expect("proven");
    b.ret(&[doubled]);

    // The caller doubles it again by calling the callee twice.
    let mut caller = function(&[Repr::I64], &[Repr::I64]);
    let x = param(&caller, 0);
    let entry = caller.entry;
    let mut b = FuncBuilder::new(&mut caller, &types, entry);
    let once = b.call(&funcs, callee_id, &[x]).expect("shape matches");
    let twice = b.call(&funcs, callee_id, &once).expect("shape matches");
    b.ret(&twice);

    let mut jit = executable_memory().expect("this machine can host its own code");
    let caller_machine_id = {
        let mut module = MachineModule::new(&mut jit);
        module
            .declare(callee_id, "double", Linkage::Local, &funcs)
            .expect("declared");
        module
            .declare(caller_id, "quadruple", Linkage::Export, &funcs)
            .expect("declared");
        module
            .define(callee_id, &callee, &funcs, &types)
            .expect("defined");
        module
            .define(caller_id, &caller, &funcs, &types)
            .expect("defined");
        module
            .declarations()
            .machine_id(caller_id)
            .expect("declared")
    };
    jit.finalize_definitions().expect("finalized");
    let address = jit.get_finalized_function(caller_machine_id);
    std::mem::forget(jit);

    let quadruple: extern "C" fn(i64) -> i64 = unsafe { std::mem::transmute(address) };
    assert_eq!(
        quadruple(3),
        12,
        "a call site names a callee and reaches it"
    );
}

#[test]
fn defining_before_declaring_is_refused() {
    let types = TypeRegistry::new();
    let mut funcs = FuncRegistry::new();
    let shape = funcs.declare_signature(Signature::default());
    let id = funcs.declare_function(shape);

    let mut func = function(&[], &[]);
    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    b.ret(&[]);

    let mut jit = executable_memory().expect("host");
    let mut module = MachineModule::new(&mut jit);
    assert!(
        module.define(id, &func, &funcs, &types).is_err(),
        "declaring is what produces the identifier a call site refers to"
    );
}

#[test]
fn the_same_program_goes_to_either_destination() {
    let types = TypeRegistry::new();
    let mut funcs = FuncRegistry::new();
    let shape = funcs.declare_signature(Signature {
        params: vec![Repr::F64],
        returns: vec![Repr::F64],
        ..Signature::default()
    });
    let id = funcs.declare_function(shape);

    let mut func = function(&[Repr::F64], &[Repr::F64]);
    let x = param(&func, 0);
    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let doubled = b.arith(NumOp::Add, x, x).expect("proven");
    b.ret(&[doubled]);

    for mut destination in [
        Box::new(executable_memory().expect("host")) as Box<dyn Module>,
        Box::new(object_file("twice").expect("host")) as Box<dyn Module>,
    ] {
        let mut module = MachineModule::new(destination.as_mut());
        module
            .declare(id, "twice", Linkage::Export, &funcs)
            .expect("declared");
        module
            .define(id, &func, &funcs, &types)
            .expect("the pipeline before the destination is the same one");
    }
}

/// A field read through a heap whose base is a runtime-provided symbol,
/// compiled to an object file, leaves an undefined reference to that symbol
/// for a linker to resolve.
///
/// This is the case `RegionBase::Symbol` exists for: an ahead-of-time binary
/// cannot bake the region's address in as a constant, because the region does
/// not exist until the binary runs. What it CAN do is name a cell the runtime
/// will write at startup — and this checks that naming actually reaches the
/// object file, rather than only that lowering does not panic (the
/// `unreachable!` it used to hit would panic on exactly this program).
#[test]
fn a_symbolic_heap_base_is_an_undefined_reference_in_an_object_file() {
    let mut types = TypeRegistry::new();
    let point = types.declare(&[Repr::I64, Repr::I64]);
    let stride = ObjectLayout::of(point, &types).size;

    let mut funcs = FuncRegistry::new();
    let shape = funcs.declare_signature(Signature {
        params: vec![Repr::Ref(RefKind::Aggregate(point))],
        returns: vec![Repr::I64],
        ..Signature::default()
    });
    let id = funcs.declare_function(shape);

    let mut func = function(&[Repr::Ref(RefKind::Aggregate(point))], &[Repr::I64]);
    let object = param(&func, 0);
    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let y = b.field_load(object, point, 1).expect("field 1 exists");
    b.ret(&[y]);

    const SYMBOL: &str = "test_symbolic_region_base";
    let bases = RegionBases::single(RegionBase::Symbol(SYMBOL.into()), stride);

    let object_module = object_file("heap_symbol").expect("host");
    let program = [Placing {
        id,
        name: "read_y",
        visibility: Visibility::Exported,
        body: Some(&func),
    }];
    let bytes = place_in_object(object_module, &program, &funcs, &types, Some(bases))
        .expect("a symbolic base compiles rather than hitting the old `unreachable!`");

    let file = object::File::parse(bytes.as_slice()).expect("a well-formed object file");
    use object::{Object, ObjectSymbol};
    let found = file.symbols().find(|symbol| symbol.name() == Ok(SYMBOL));
    let symbol = found.unwrap_or_else(|| {
        panic!("`{SYMBOL}` never appears in the object at all — the field load did not reach it")
    });
    assert!(
        symbol.is_undefined(),
        "`{SYMBOL}` is declared as something this compilation defines, not as the \
         runtime-provided cell a linker must still resolve"
    );
}
