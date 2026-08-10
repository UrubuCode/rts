//! Memory access, compiled and run against a real buffer.
//!
//! A reference here is an index, and the whole claim of this module is that
//! turning one into an address is arithmetic. These tests build a heap by hand,
//! compile a function that reads and writes it, and check that the bytes moved —
//! which is the only way to find out whether the arithmetic is right.

use cranelift_codegen::isa::CallConv;
use cranelift_codegen::settings::{self, Configurable};
use cranelift_codegen::verifier::verify_function;
use cranelift_module::{Linkage, Module};
use rts_cranelift::ir::{FuncBuilder, FuncRegistry, Function, NumOp, Region, Signature, ValueId};
use rts_cranelift::lower::{Capability, Environment, Heap, LowerError, lower_in};
use rts_cranelift::mem::{HeaderLayout, ObjectLayout, RegionBase, RegionBases};
use rts_cranelift::repr::{RefKind, Repr};
use rts_cranelift::target::{MachineModule, executable_memory};
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

/// A heap of `count` objects of one type, laid out contiguously.
///
/// Leaked deliberately: compiled code holds its address as a constant, so the
/// buffer has to outlive everything that might run. A real host owns it.
fn heap_for(ty: rts_cranelift::types::TypeId, types: &TypeRegistry, count: usize) -> RegionBases {
    let layout = ObjectLayout::of(ty, types);
    let bytes = vec![0u8; layout.size as usize * count];
    let leaked = Box::leak(bytes.into_boxed_slice());
    RegionBases::single(RegionBase::Immediate(leaked.as_ptr() as u64), layout.size)
}

/// The address of object `index` in that heap.
fn object_at(bases: &RegionBases, base: u64, index: usize) -> *mut u8 {
    (base as usize + index * bases.stride() as usize) as *mut u8
}

/// Compiles one function, with a heap, into memory and returns its address.
fn compile_with_heap(
    name: &str,
    id: rts_cranelift::ir::FuncId,
    func: &Function,
    funcs: &FuncRegistry,
    types: &TypeRegistry,
    bases: &RegionBases,
) -> *const u8 {
    // Whatever this host calls things, since the code is about to run on it.
    let call_conv = rts_cranelift::target::host_isa()
        .expect("host")
        .default_call_conv();
    let lowered = lower_in(
        func,
        call_conv,
        Environment {
            outside: None,
            heap: Some(Heap {
                bases,
                types,
                base: None,
            }),
        },
    )
    .expect("lowering succeeds")
    // Lowering answers the body and which entry points it named; this test is
    // about the body.
    .func;

    let mut flags = settings::builder();
    flags
        .set("enable_verifier", "true")
        .expect("a real setting");
    if let Err(errors) = verify_function(&lowered, &settings::Flags::new(flags)) {
        panic!("the code generator rejected our output:\n{errors}\n{lowered}");
    }

    let mut jit = executable_memory().expect("host");
    let machine_id = {
        let mut module = MachineModule::new(&mut jit);
        module
            .declare(id, name, Linkage::Export, funcs)
            .expect("declared");
        let mut context = cranelift_codegen::Context::new();
        context.func = lowered;
        let machine_id = module.declarations().machine_id(id).expect("declared");
        jit.define_function(machine_id, &mut context)
            .expect("defined");
        machine_id
    };
    jit.finalize_definitions().expect("finalized");
    let address = jit.get_finalized_function(machine_id);
    std::mem::forget(jit);
    address
}

#[test]
fn a_field_read_is_a_load_from_a_computed_address() {
    let mut types = TypeRegistry::new();
    let point = types.declare(&[Repr::I64, Repr::I64]);
    let bases = heap_for(point, &types, 4);

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

    let base = match bases.addressing() {
        rts_cranelift::mem::Addressing::Single {
            base: RegionBase::Immediate(address),
            ..
        } => *address,
        other => panic!("expected one region, got {other:?}"),
    };

    // Write the second field of object two, by hand, the way the heap says.
    let layout = ObjectLayout::of(point, &types);
    let object_two = object_at(&bases, base, 2);
    unsafe {
        let field = object_two.offset(layout.field_offsets[1] as isize) as *mut i64;
        field.write(4242);
    }

    let address = compile_with_heap("read_y", id, &func, &funcs, &types, &bases);
    let read_y: extern "C" fn(i64) -> i64 = unsafe { std::mem::transmute(address) };
    assert_eq!(
        read_y(2),
        4242,
        "a reference is an index, and turning it into an address is arithmetic"
    );
}

#[test]
fn a_field_write_lands_where_the_layout_says() {
    let mut types = TypeRegistry::new();
    let cell = types.declare(&[Repr::I64]);
    let bases = heap_for(cell, &types, 4);

    let mut funcs = FuncRegistry::new();
    let shape = funcs.declare_signature(Signature {
        params: vec![Repr::Ref(RefKind::Aggregate(cell)), Repr::I64],
        returns: vec![],
        ..Signature::default()
    });
    let id = funcs.declare_function(shape);

    let mut func = function(&[Repr::Ref(RefKind::Aggregate(cell)), Repr::I64], &[]);
    let (object, value) = (param(&func, 0), param(&func, 1));
    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    b.field_store(object, cell, 0, value)
        .expect("field 0 exists");
    b.ret(&[]);

    let base = match bases.addressing() {
        rts_cranelift::mem::Addressing::Single {
            base: RegionBase::Immediate(address),
            ..
        } => *address,
        other => panic!("expected one region, got {other:?}"),
    };

    let address = compile_with_heap("write", id, &func, &funcs, &types, &bases);
    let write: extern "C" fn(i64, i64) = unsafe { std::mem::transmute(address) };
    write(3, 99);

    let layout = ObjectLayout::of(cell, &types);
    let written = unsafe {
        let field =
            object_at(&bases, base, 3).offset(layout.field_offsets[0] as isize) as *const i64;
        field.read()
    };
    assert_eq!(written, 99);
}

#[test]
fn reading_a_field_a_program_just_wrote_agrees_with_itself() {
    let mut types = TypeRegistry::new();
    let cell = types.declare(&[Repr::I64]);
    let bases = heap_for(cell, &types, 2);

    let mut funcs = FuncRegistry::new();
    let shape = funcs.declare_signature(Signature {
        params: vec![Repr::Ref(RefKind::Aggregate(cell)), Repr::I64],
        returns: vec![Repr::I64],
        ..Signature::default()
    });
    let id = funcs.declare_function(shape);

    let mut func = function(
        &[Repr::Ref(RefKind::Aggregate(cell)), Repr::I64],
        &[Repr::I64],
    );
    let (object, value) = (param(&func, 0), param(&func, 1));
    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    b.field_store(object, cell, 0, value).expect("field exists");
    let read = b.field_load(object, cell, 0).expect("field exists");
    let doubled = b.arith(NumOp::Add, read, read).expect("proven");
    b.ret(&[doubled]);

    let address = compile_with_heap("roundtrip", id, &func, &funcs, &types, &bases);
    let roundtrip: extern "C" fn(i64, i64) -> i64 = unsafe { std::mem::transmute(address) };
    assert_eq!(roundtrip(1, 21), 42);
}

#[test]
fn a_field_access_without_a_heap_is_refused() {
    let mut types = TypeRegistry::new();
    let cell = types.declare(&[Repr::I64]);

    let mut func = function(&[Repr::Ref(RefKind::Aggregate(cell))], &[Repr::I64]);
    let object = param(&func, 0);
    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let read = b.field_load(object, cell, 0).expect("field exists");
    b.ret(&[read]);

    assert!(
        matches!(
            lower_in(&func, CallConv::SystemV, Environment::default()),
            Err(LowerError::NotYetLowered {
                needs: Capability::Memory,
                ..
            })
        ),
        "the arithmetic needs a heap to be arithmetic about"
    );
}

#[test]
fn allocation_is_still_refused_because_it_is_not_arithmetic() {
    let mut types = TypeRegistry::new();
    let cell = types.declare(&[Repr::I64]);
    let bases = heap_for(cell, &types, 1);

    let mut func = function(&[], &[]);
    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    b.alloc(cell, Region::Local);
    b.ret(&[]);

    assert!(
        matches!(
            lower_in(
                &func,
                CallConv::SystemV,
                Environment {
                    outside: None,
                    heap: Some(Heap {
                        bases: &bases,
                        types: &types,
                        base: None,
                    }),
                },
            ),
            Err(LowerError::NotYetLowered {
                needs: Capability::Memory,
                ..
            })
        ),
        "asking a heap for space is a runtime entry point, not an address computation"
    );
}

#[test]
fn the_header_sits_before_every_field() {
    let mut types = TypeRegistry::new();
    let ty = types.declare(&[Repr::I64]);
    let layout = ObjectLayout::of(ty, &types);

    assert_eq!(layout.field_offsets[0], HeaderLayout::BYTES as i32);
    assert_eq!(
        layout.size,
        HeaderLayout::BYTES + 8,
        "every object pays the header, so it is one word and not more"
    );
}

#[test]
fn a_sharded_heap_reaches_the_right_object_in_the_right_region() {
    let mut types = TypeRegistry::new();
    let cell = types.declare(&[Repr::I64]);
    let layout = ObjectLayout::of(cell, &types);

    // Four regions, each holding two objects. A reference's low bits select the
    // region and the rest select the slot inside it, which is the arithmetic
    // this test exists to check actually reaches the object it names.
    const REGIONS: usize = 4;
    const SLOTS: usize = 2;
    let stride = layout.size;

    let mut bases = Vec::with_capacity(REGIONS);
    for region in 0..REGIONS {
        let bytes = vec![0u8; stride as usize * SLOTS];
        let leaked = Box::leak(bytes.into_boxed_slice());
        let base = leaked.as_ptr() as u64;

        // Fill every slot with a number that says where it is, so that reading
        // the wrong one is visible rather than plausible.
        for slot in 0..SLOTS {
            unsafe {
                let object = (base as usize + slot * stride as usize) as *mut u8;
                let offset = layout.field_offset(0).expect("field exists");
                (object.offset(offset as isize) as *mut i64).write((region * 10 + slot) as i64);
            }
        }
        bases.push(base);
    }

    // The table of region bases, which the emitted code loads from.
    let table = Box::leak(bases.into_boxed_slice());
    let region_bases = RegionBases::sharded(
        RegionBase::Immediate(table.as_ptr() as u64),
        REGIONS as u32,
        stride,
    )
    .expect("a power of two");

    let mut funcs = FuncRegistry::new();
    let shape = funcs.declare_signature(Signature {
        params: vec![Repr::Ref(RefKind::Aggregate(cell))],
        returns: vec![Repr::I64],
        ..Signature::default()
    });
    let id = funcs.declare_function(shape);

    let mut func = function(&[Repr::Ref(RefKind::Aggregate(cell))], &[Repr::I64]);
    let object = param(&func, 0);
    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let value = b.field_load(object, cell, 0).expect("field exists");
    b.ret(&[value]);

    let address = compile_with_heap("sharded", id, &func, &funcs, &types, &region_bases);
    let read: extern "C" fn(i64) -> i64 = unsafe { std::mem::transmute(address) };

    for region in 0..REGIONS {
        for slot in 0..SLOTS {
            // A reference is the slot shifted past the region selector, plus the
            // region — which is what the emitted code takes apart again.
            let reference = ((slot << REGIONS.trailing_zeros()) | region) as i64;
            assert_eq!(
                read(reference),
                (region * 10 + slot) as i64,
                "region {region} slot {slot} was reached as something else"
            );
        }
    }
}
