//! Shapes, and the property reads they make possible.
//!
//! The claim under test is that a shape adds one thing — a name to a position in
//! a layout that grows — and that everything else keeps working untouched. So
//! these tests build layouts by adding properties, then compile ordinary field
//! reads through the positions those layouts give, and run them.

use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::Linkage;
use rts_cranelift::ir::{FuncBuilder, FuncRegistry, Function, Signature, ValueId};
use rts_cranelift::mem::{HeaderLayout, ObjectLayout, RegionBase, RegionBases};
use rts_cranelift::repr::{RefKind, Repr};
use rts_cranelift::shape::{KeyRegistry, ShapeError, ShapeTree};
use rts_cranelift::symbols::RtEntry;
use rts_cranelift::target::{MachineModule, host_isa};
use rts_cranelift::types::{TypeId, TypeRegistry};
use rts_cranelift::verify::verify;

extern "C" fn never(_a: i64, _b: i64) -> i64 {
    unreachable!("these programs do not allocate")
}

extern "C" fn never_barrier(_a: i64, _b: i64) {
    unreachable!("these programs store no references")
}

fn param(func: &Function, index: usize) -> ValueId {
    func.block(func.entry).expect("entry exists").params[index]
}

/// A heap of one object, with its header and fields written by hand.
fn heap_with(ty: TypeId, types: &TypeRegistry, fields: &[i64]) -> RegionBases {
    let layout = ObjectLayout::of(ty, types);
    let bytes = vec![0u8; layout.size as usize];
    let leaked = Box::leak(bytes.into_boxed_slice());
    let base = leaked.as_ptr() as u64;

    unsafe {
        let object = base as *mut u8;
        (object.offset(HeaderLayout::TYPE_OFFSET as isize) as *mut i64).write(ty.index() as i64);
        for (index, value) in fields.iter().enumerate() {
            let offset = layout
                .field_offset(index as u32)
                .expect("the layout has this field");
            (object.offset(offset as isize) as *mut i64).write(*value);
        }
    }

    RegionBases::single(RegionBase::Immediate(base), layout.size)
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
fn two_objects_built_the_same_way_arrive_at_the_same_layout() {
    let mut keys = KeyRegistry::new();
    let mut shapes = ShapeTree::new();
    let (x, y) = (keys.declare_one(), keys.declare_one());

    let one = shapes
        .transition(shapes.root(), x, Repr::I64)
        .expect("added");
    let one = shapes.transition(one, y, Repr::I64).expect("added");

    let other = shapes
        .transition(shapes.root(), x, Repr::I64)
        .expect("added");
    let other = shapes.transition(other, y, Repr::I64).expect("added");

    assert_eq!(
        one, other,
        "comparing one number has to be a complete answer to whether two objects \
         are laid out alike"
    );
}

#[test]
fn the_same_properties_in_a_different_order_are_a_different_layout() {
    let mut keys = KeyRegistry::new();
    let mut shapes = ShapeTree::new();
    let (x, y) = (keys.declare_one(), keys.declare_one());

    let forwards = shapes
        .transition(shapes.root(), x, Repr::I64)
        .expect("added");
    let forwards = shapes.transition(forwards, y, Repr::I64).expect("added");

    let backwards = shapes
        .transition(shapes.root(), y, Repr::I64)
        .expect("added");
    let backwards = shapes.transition(backwards, x, Repr::I64).expect("added");

    assert_ne!(
        forwards, backwards,
        "they hold the same properties at different positions, and a position is \
         what compiled code was told"
    );
    assert_eq!(shapes.slot_of(forwards, x), Some(0));
    assert_eq!(shapes.slot_of(backwards, x), Some(1));
}

#[test]
fn a_position_handed_out_once_survives_the_layout_growing() {
    let mut keys = KeyRegistry::new();
    let mut shapes = ShapeTree::new();
    let (x, y, z) = (keys.declare_one(), keys.declare_one(), keys.declare_one());

    let small = shapes
        .transition(shapes.root(), x, Repr::I64)
        .expect("added");
    let early = shapes.slot_of(small, x).expect("it is there");

    let grown = shapes.transition(small, y, Repr::I64).expect("added");
    let grown = shapes.transition(grown, z, Repr::I64).expect("added");

    assert_eq!(
        shapes.slot_of(grown, x),
        Some(early),
        "properties are appended, so what compiled code was told stays true"
    );
}

#[test]
fn assigning_to_a_property_an_object_already_has_is_not_a_transition() {
    let mut keys = KeyRegistry::new();
    let mut shapes = ShapeTree::new();
    let x = keys.declare_one();

    let shape = shapes
        .transition(shapes.root(), x, Repr::I64)
        .expect("added");
    let again = shapes
        .transition(shape, x, Repr::I64)
        .expect("already there");

    assert_eq!(
        again, shape,
        "writing to a property does not change a layout"
    );
    assert_eq!(shapes.len(), 1, "and does not leave a layout behind either");
}

#[test]
fn adding_a_property_held_differently_is_refused_rather_than_silently_reshaped() {
    let mut keys = KeyRegistry::new();
    let mut shapes = ShapeTree::new();
    let x = keys.declare_one();

    let shape = shapes
        .transition(shapes.root(), x, Repr::I64)
        .expect("added");

    assert_eq!(
        shapes.transition(shape, x, Repr::F64),
        Err(ShapeError::AlreadyHeldDifferently {
            key: x,
            existing: Repr::I64,
            asked: Repr::F64,
        }),
        "the key list would be unchanged while what is in a slot changed under \
         code that had already been told"
    );

    // Reaching it deliberately produces a different layout, which guarded code
    // will not mistake for the old one.
    let changed = shapes.redefine(shape, x, Repr::F64);
    assert_ne!(changed, shape);
    assert_eq!(shapes.repr_of(changed, x), Some(Repr::F64));
}

#[test]
fn removing_a_property_leaves_the_layouts_that_still_use_it_alone() {
    let mut keys = KeyRegistry::new();
    let mut shapes = ShapeTree::new();
    let (x, y) = (keys.declare_one(), keys.declare_one());

    let full = shapes
        .transition(shapes.root(), x, Repr::I64)
        .expect("added");
    let full = shapes.transition(full, y, Repr::I64).expect("added");

    let without = shapes.remove(full, x);
    assert_ne!(without, full);
    assert_eq!(shapes.slot_of(without, x), None);
    assert_eq!(shapes.slot_of(without, y), Some(0));

    assert_eq!(
        shapes.slot_of(full, x),
        Some(0),
        "the layout other objects are using is untouched, because the tree only grows"
    );
}

#[test]
fn a_layout_reached_by_adding_properties_is_an_ordinary_aggregate() {
    let mut keys = KeyRegistry::new();
    let mut shapes = ShapeTree::new();
    let mut types = TypeRegistry::new();
    let (x, y) = (keys.declare_one(), keys.declare_one());

    let shape = shapes
        .transition(shapes.root(), x, Repr::I64)
        .expect("added");
    let shape = shapes.transition(shape, y, Repr::F64).expect("added");

    let ty = shapes.layout(shape, &mut types);
    let layout = ObjectLayout::of(ty, &types);

    assert_eq!(layout.field_offsets.len(), 2);
    assert_eq!(
        shapes.layout(shape, &mut types),
        ty,
        "asking twice gives the same aggregate, not a second one"
    );
}

#[test]
fn a_property_read_through_a_shape_is_an_ordinary_field_read_and_runs() {
    let mut keys = KeyRegistry::new();
    let mut shapes = ShapeTree::new();
    let mut types = TypeRegistry::new();
    let (x, y) = (keys.declare_one(), keys.declare_one());

    let shape = shapes
        .transition(shapes.root(), x, Repr::I64)
        .expect("added");
    let shape = shapes.transition(shape, y, Repr::I64).expect("added");
    let ty = shapes.layout(shape, &mut types);

    // The shape says which field; reading it needs nothing a shape introduced.
    let slot = shapes.slot_of(shape, y).expect("the layout has it");

    let mut func = Function::new(Signature {
        params: vec![Repr::Ref(RefKind::Aggregate(ty))],
        returns: vec![Repr::I64],
        ..Signature::default()
    });
    let object = param(&func, 0);
    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let value = b.field_load(object, ty, slot).expect("the field exists");
    b.ret(&[value]);

    assert_eq!(verify(&func, &types, &FuncRegistry::new()), vec![]);

    let bases = heap_with(ty, &types, &[100, 200]);
    let address = compile(
        "read_y",
        &[Repr::Ref(RefKind::Aggregate(ty))],
        &[Repr::I64],
        &func,
        bases,
        &types,
    );
    let read: extern "C" fn(i64) -> i64 = unsafe { std::mem::transmute(address) };
    assert_eq!(
        read(0),
        200,
        "the second property, at the position the shape gave"
    );
}

#[test]
fn an_unknown_object_is_narrowed_by_its_shape_and_then_read_normally() {
    let mut keys = KeyRegistry::new();
    let mut shapes = ShapeTree::new();
    let mut types = TypeRegistry::new();
    let x = keys.declare_one();

    let expected = shapes
        .transition(shapes.root(), x, Repr::I64)
        .expect("added");
    let expected_ty = shapes.layout(expected, &mut types);

    // A layout of the same width that is not the same layout.
    let other = keys.declare_one();
    let other_shape = shapes
        .transition(shapes.root(), other, Repr::I64)
        .expect("added");
    let other_ty = shapes.layout(other_shape, &mut types);
    assert_ne!(expected_ty, other_ty);

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
    b.add_block_param(matched, Repr::Ref(RefKind::Aggregate(expected_ty)));
    b.guard_type(object, expected_ty, (matched, &[]), (mismatched, &[]))
        .expect("well formed");

    let slot = shapes.slot_of(expected, x).expect("the layout has it");
    let narrowed = func.block(matched).expect("exists").params[0];
    let mut b = FuncBuilder::new(&mut func, &types, matched);
    let value = b.field_load(narrowed, expected_ty, slot).expect("exists");
    b.ret(&[value]);

    let mut b = FuncBuilder::new(&mut func, &types, mismatched);
    let missing = b.declare_const(rts_cranelift::ir::ConstDecl::Scalar {
        repr: Repr::I64,
        bits: rts_cranelift::ir::ScalarBits(-1i64 as u64),
    });
    let missing = b.use_const(missing);
    b.ret(&[missing]);

    let bases = heap_with(expected_ty, &types, &[7]);
    let address = compile(
        "read_guarded",
        &[Repr::Ref(RefKind::Opaque)],
        &[Repr::I64],
        &func,
        bases,
        &types,
    );
    let read: extern "C" fn(i64) -> i64 = unsafe { std::mem::transmute(address) };
    assert_eq!(
        read(0),
        7,
        "the guard established the layout, and the read needed nothing further"
    );
}

#[test]
fn a_chain_of_additions_costs_one_layout_per_addition() {
    let mut keys = KeyRegistry::new();
    let mut shapes = ShapeTree::new();

    let mut shape = shapes.root();
    for key in keys.declare(50) {
        shape = shapes.transition(shape, key, Repr::I64).expect("added");
    }

    assert_eq!(
        shapes.len(),
        50,
        "a layout stores one property and points at what it extends, so building \
         an object is linear rather than quadratic in its own length"
    );
    assert_eq!(shapes.width(shape), 50);
}
