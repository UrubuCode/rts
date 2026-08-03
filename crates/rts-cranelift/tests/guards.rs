//! Type guards, run.
//!
//! The value encoding says a word is a reference and stops there. Which kind it
//! refers to is in the object, so answering that means reading it — and these
//! tests check the two guards compose into a complete narrowing: from a generic
//! value, to a reference, to an object of a known type whose fields can be read.

use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::Linkage;
use rts_cranelift::ir::{FuncBuilder, FuncRegistry, Function, Signature, ValueId};
use rts_cranelift::mem::{HeaderLayout, ObjectLayout, RegionBase, RegionBases};
use rts_cranelift::repr::{RefKind, Repr};
use rts_cranelift::symbols::RtEntry;
use rts_cranelift::target::{MachineModule, host_isa};
use rts_cranelift::types::{TypeId, TypeRegistry};
use rts_cranelift::verify::{VerifyError, verify};

extern "C" fn never(_a: i64, _b: i64) -> i64 {
    unreachable!("these programs do not allocate")
}

extern "C" fn never_barrier(_a: i64, _b: i64) {
    unreachable!("these programs store no references")
}

fn param(func: &Function, index: usize) -> ValueId {
    func.block(func.entry).expect("entry exists").params[index]
}

/// A heap of `count` slots wide enough for either type, and its bases.
fn heap(stride: u32, count: usize) -> (RegionBases, u64) {
    let bytes = vec![0u8; stride as usize * count];
    let leaked = Box::leak(bytes.into_boxed_slice());
    let base = leaked.as_ptr() as u64;
    (
        RegionBases::single(RegionBase::Immediate(base), stride),
        base,
    )
}

/// Writes an object's header, the way a real allocator would.
unsafe fn write_header(base: u64, stride: u32, slot: usize, ty: TypeId) {
    unsafe {
        let address = (base as usize + slot * stride as usize) as *mut i64;
        address
            .byte_offset(HeaderLayout::TYPE_OFFSET as isize)
            .write(ty.index() as i64);
    }
}

/// Writes a field of an object, the way the layout says.
unsafe fn write_field(
    base: u64,
    stride: u32,
    slot: usize,
    layout: &ObjectLayout,
    field: u32,
    value: i64,
) {
    unsafe {
        let object = (base as usize + slot * stride as usize) as *mut u8;
        let offset = layout.field_offset(field).expect("field exists");
        (object.offset(offset as isize) as *mut i64).write(value);
    }
}

fn compile(
    name: &str,
    params: &[Repr],
    returns: &[Repr],
    func: &Function,
    bases: RegionBases,
    types: &TypeRegistry,
) -> *const u8 {
    let mut funcs = FuncRegistry::new();
    let shape = funcs.declare_signature(Signature {
        params: params.to_vec(),
        returns: returns.to_vec(),
        ..Signature::default()
    });
    let id = funcs.declare_function(shape);

    let isa = host_isa().expect("host");
    let mut builder = JITBuilder::with_isa(isa, cranelift_module::default_libcall_names());
    builder.symbol(RtEntry::Alloc.symbol(), never as *const u8);
    builder.symbol(RtEntry::WriteBarrier.symbol(), never_barrier as *const u8);
    let mut jit = JITModule::new(builder);

    let machine_id = {
        let mut module = MachineModule::new(&mut jit).with_heap(bases);
        module
            .declare(id, name, Linkage::Export, &funcs)
            .expect("declared");
        module.define(id, func, &funcs, types).expect("defined");
        module.declarations().machine_id(id).expect("declared")
    };
    jit.finalize_definitions().expect("finalized");
    let address = jit.get_finalized_function(machine_id);
    std::mem::forget(jit);
    address
}

#[test]
fn a_type_guard_reads_the_object_and_takes_the_path_that_matches() {
    let mut types = TypeRegistry::new();
    let point = types.declare(&[Repr::I64]);
    let other = types.declare(&[Repr::I64]);
    let layout = ObjectLayout::of(point, &types);
    let (bases, base) = heap(layout.size, 4);

    let mut func = Function::new(Signature {
        params: vec![Repr::Ref(RefKind::Opaque)],
        returns: vec![Repr::I64],
        ..Signature::default()
    });
    let object = param(&func, 0);
    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);

    let matched = b.create_block();
    let mismatched = b.create_block();
    b.add_block_param(matched, Repr::Ref(RefKind::Aggregate(point)));
    b.guard_type(object, point, (matched, &[]), (mismatched, &[]))
        .expect("well formed");

    // The success path can now read a field, which needs a known type.
    let narrowed = func.block(matched).expect("exists").params[0];
    let mut b = FuncBuilder::new(&mut func, &types, matched);
    let value = b.field_load(narrowed, point, 0).expect("field exists");
    b.ret(&[value]);

    let mut b = FuncBuilder::new(&mut func, &types, mismatched);
    let missing = b.declare_const(rts_cranelift::ir::ConstDecl::Scalar {
        repr: Repr::I64,
        bits: rts_cranelift::ir::ScalarBits(-1i64 as u64),
    });
    let missing = b.use_const(missing);
    b.ret(&[missing]);

    assert_eq!(verify(&func, &types, &FuncRegistry::new()), vec![]);

    unsafe {
        write_header(base, layout.size, 0, point);
        write_field(base, layout.size, 0, &layout, 0, 500);
        write_header(base, layout.size, 1, other);
        write_field(base, layout.size, 1, &layout, 0, 900);
    }

    let address = compile(
        "read",
        &[Repr::Ref(RefKind::Opaque)],
        &[Repr::I64],
        &func,
        bases,
        &types,
    );
    let read: extern "C" fn(i64) -> i64 = unsafe { std::mem::transmute(address) };

    assert_eq!(
        read(0),
        500,
        "the object is what it claimed, so the field is read"
    );
    assert_eq!(
        read(1),
        -1,
        "and one that is not takes the path that says so, rather than reading anyway"
    );
}

#[test]
fn the_two_guards_compose_from_a_generic_value_to_a_known_object() {
    let mut types = TypeRegistry::new();
    let cell = types.declare(&[Repr::I64]);
    let layout = ObjectLayout::of(cell, &types);
    let (bases, base) = heap(layout.size, 2);

    let mut func = Function::new(Signature {
        params: vec![Repr::Tagged],
        returns: vec![Repr::I64],
        ..Signature::default()
    });
    let unknown = param(&func, 0);
    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);

    let is_reference = b.create_block();
    let is_cell = b.create_block();
    let gave_up = b.create_block();
    b.add_block_param(is_reference, Repr::Ref(RefKind::Opaque));
    b.add_block_param(is_cell, Repr::Ref(RefKind::Aggregate(cell)));

    // First: is this a reference at all? That the encoding can answer.
    b.guard(
        unknown,
        Repr::Ref(RefKind::Opaque),
        (is_reference, &[]),
        (gave_up, &[]),
    )
    .expect("well formed");

    // Then: what does it refer to? That only the object can answer.
    let reference = func.block(is_reference).expect("exists").params[0];
    let mut b = FuncBuilder::new(&mut func, &types, is_reference);
    b.guard_type(reference, cell, (is_cell, &[]), (gave_up, &[]))
        .expect("well formed");

    let narrowed = func.block(is_cell).expect("exists").params[0];
    let mut b = FuncBuilder::new(&mut func, &types, is_cell);
    let value = b.field_load(narrowed, cell, 0).expect("field exists");
    b.ret(&[value]);

    let mut b = FuncBuilder::new(&mut func, &types, gave_up);
    let missing = b.declare_const(rts_cranelift::ir::ConstDecl::Scalar {
        repr: Repr::I64,
        bits: rts_cranelift::ir::ScalarBits(-1i64 as u64),
    });
    let missing = b.use_const(missing);
    b.ret(&[missing]);

    assert_eq!(verify(&func, &types, &FuncRegistry::new()), vec![]);

    unsafe {
        write_header(base, layout.size, 0, cell);
        write_field(base, layout.size, 0, &layout, 0, 12345);
    }

    let address = compile(
        "narrow",
        &[Repr::Tagged],
        &[Repr::I64],
        &func,
        bases,
        &types,
    );
    let narrow: extern "C" fn(i64) -> i64 = unsafe { std::mem::transmute(address) };

    let reference_word = rts_cranelift::tags::encode(rts_cranelift::tags::TAG_REFERENCE, 0);
    assert_eq!(
        narrow(reference_word as i64),
        12345,
        "generic, then a reference, then an object whose field can be read"
    );

    let not_a_reference = rts_cranelift::tags::encode_double(1.5);
    assert_eq!(
        narrow(not_a_reference as i64),
        -1,
        "and a value that is not a reference stops at the first guard"
    );
}

#[test]
fn a_type_guard_on_something_that_is_not_a_reference_is_refused() {
    let mut types = TypeRegistry::new();
    let cell = types.declare(&[Repr::I64]);

    let mut func = Function::new(Signature {
        params: vec![Repr::Tagged],
        ..Signature::default()
    });
    let unknown = param(&func, 0);
    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let ok = b.create_block();
    let fail = b.create_block();
    b.add_block_param(ok, Repr::Ref(RefKind::Aggregate(cell)));

    assert!(
        b.guard_type(unknown, cell, (ok, &[]), (fail, &[])).is_err(),
        "reading a type out of something that is not an object reads whatever is there"
    );
}

#[test]
fn a_type_guard_whose_success_block_ignores_the_object_is_refused() {
    let mut types = TypeRegistry::new();
    let cell = types.declare(&[Repr::I64]);

    let mut func = Function::new(Signature {
        params: vec![Repr::Ref(RefKind::Opaque)],
        ..Signature::default()
    });
    let object = param(&func, 0);
    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let ok = b.create_block();
    let fail = b.create_block();

    assert!(
        b.guard_type(object, cell, (ok, &[]), (fail, &[])).is_err(),
        "without the parameter the narrowed object would escape the path that proved it"
    );
}

#[test]
fn the_verifier_catches_a_type_guard_the_builder_did_not_make() {
    let mut types = TypeRegistry::new();
    let cell = types.declare(&[Repr::I64]);

    let mut func = Function::new(Signature {
        params: vec![Repr::I64],
        ..Signature::default()
    });
    let number = param(&func, 0);
    let entry = func.entry;
    let ok = func.push_block();
    let fail = func.push_block();
    func.push_block_param(ok, Repr::Ref(RefKind::Aggregate(cell)));

    func.set_terminator(
        entry,
        rts_cranelift::ir::Terminator::GuardType {
            object: number,
            expect: cell,
            ok: rts_cranelift::ir::BlockCall::to(ok),
            fail: rts_cranelift::ir::BlockCall::to(fail),
        },
    );
    func.set_terminator(ok, rts_cranelift::ir::Terminator::Return(vec![]));
    func.set_terminator(fail, rts_cranelift::ir::Terminator::Return(vec![]));

    assert!(
        verify(&func, &types, &FuncRegistry::new())
            .iter()
            .any(|e| matches!(e, VerifyError::GuardTypeOnNonReference { .. })),
        "the builder makes it awkward; the verifier makes it impossible"
    );
}
