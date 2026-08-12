//! Cached property reads, run against real objects.
//!
//! A cache is only worth having if it is right when it hits, right when it
//! misses, and right when the thing it remembered stops being true. These tests
//! feed one site objects of one layout, then of two, then of a layout that does
//! not have the property at all — and count how often the resolver was asked,
//! because "it learned" is a claim about how many times it had to ask.

use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Mutex, MutexGuard};

use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::Linkage;
use rts_cranelift::ir::{FuncBuilder, FuncRegistry, Function, Signature};
use rts_cranelift::mem::{HeaderLayout, ObjectLayout, RegionBase, RegionBases};
use rts_cranelift::repr::{RefKind, Repr};
use rts_cranelift::shape::{Key, KeyRegistry, ShapeTree};
use rts_cranelift::symbols::RtEntry;
use rts_cranelift::target::{MachineModule, host_isa};
use rts_cranelift::types::{TypeId, TypeRegistry};
use rts_cranelift::verify::verify;

/// How many times a site had to ask where a property was.
static ASKS: AtomicI64 = AtomicI64::new(0);
static SERIAL: Mutex<()> = Mutex::new(());

/// What the resolver knows: layout, property, and where it sits.
static KNOWN: Mutex<Vec<(i64, u32, i64)>> = Mutex::new(Vec::new());

fn reset() -> MutexGuard<'static, ()> {
    let guard = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
    ASKS.store(0, Ordering::SeqCst);
    KNOWN.lock().unwrap_or_else(|p| p.into_inner()).clear();
    guard
}

/// Stands in for the runtime's answer, and fills the cell like a real one would.
///
/// The filling is the part that matters: a resolver that only answered would
/// leave the site as slow on the second read as on the first.
extern "C" fn resolve(object: i64, key: i64, cell: i64) -> i64 {
    ASKS.fetch_add(1, Ordering::SeqCst);

    let layout = unsafe { object_header(object) };
    let known = KNOWN.lock().unwrap_or_else(|p| p.into_inner());
    let found = known
        .iter()
        .find(|(shape, property, _)| *shape == layout && *property == key as u32)
        .map(|(_, _, offset)| *offset);

    match found {
        Some(offset) => {
            unsafe {
                let cell = cell as *mut i64;
                cell.write(layout);
                cell.offset(1).write(offset);
            }
            offset
        }
        // Left cold on purpose: a site that remembered a layout it could not read
        // would take the slow path and then read at whatever offset was there.
        None => -1,
    }
}

/// Stands in for a resolver that has decided it cannot answer for this layout,
/// and REMEMBERS having decided.
///
/// The negative offset is the whole of the mechanism: a cell whose first word
/// the site recognises but whose second is below zero must send the site to its
/// miss path, and must never be added to an address and loaded from. This stub
/// exists so that the machine's half of that is pinned without a runtime.
extern "C" fn refuse(object: i64, _key: i64, cell: i64) -> i64 {
    ASKS.fetch_add(1, Ordering::SeqCst);
    let layout = unsafe { object_header(object) };
    unsafe {
        let cell = cell as *mut i64;
        cell.write(layout);
        cell.offset(1).write(-1);
        cell.offset(2).write(0);
        cell.offset(3).write(-1);
    }
    -1
}

extern "C" fn never(_a: i64, _b: i64) -> i64 {
    unreachable!("these programs do not allocate")
}

extern "C" fn never_barrier(_a: i64, _b: i64) {
    unreachable!("these programs store no references")
}

/// Where the test heap is, so the resolver can read an object's header.
static HEAP_BASE: AtomicI64 = AtomicI64::new(0);
static HEAP_STRIDE: AtomicI64 = AtomicI64::new(0);

unsafe fn object_header(reference: i64) -> i64 {
    unsafe {
        let base = HEAP_BASE.load(Ordering::SeqCst);
        let stride = HEAP_STRIDE.load(Ordering::SeqCst);
        ((base + reference * stride) as *const i64).read()
    }
}

/// A heap of `count` slots, wide enough for anything these tests build.
fn heap(stride: u32, count: usize) -> RegionBases {
    let bytes = vec![0u8; stride as usize * count];
    let leaked = Box::leak(bytes.into_boxed_slice());
    let base = leaked.as_ptr() as u64;

    HEAP_BASE.store(base as i64, Ordering::SeqCst);
    HEAP_STRIDE.store(i64::from(stride), Ordering::SeqCst);
    RegionBases::single(RegionBase::Immediate(base), stride)
}

/// Writes an object of a layout, with its fields.
unsafe fn write_object(slot: usize, ty: TypeId, layout: &ObjectLayout, fields: &[i64]) {
    unsafe {
        let base = HEAP_BASE.load(Ordering::SeqCst);
        let stride = HEAP_STRIDE.load(Ordering::SeqCst);
        let object = (base + slot as i64 * stride) as *mut u8;

        (object.offset(HeaderLayout::TYPE_OFFSET as isize) as *mut i64).write(ty.index() as i64);
        for (index, value) in fields.iter().enumerate() {
            let offset = layout.field_offset(index as u32).expect("field exists");
            (object.offset(offset as isize) as *mut i64).write(*value);
        }
    }
}

/// Tells the resolver where a property sits in a layout.
fn teach(ty: TypeId, key: Key, layout: &ObjectLayout, slot: u32) {
    KNOWN.lock().unwrap_or_else(|p| p.into_inner()).push((
        ty.index() as i64,
        key.index() as u32,
        i64::from(layout.field_offset(slot).expect("field exists")),
    ));
}

/// Builds a function that reads one property of an unknown object.
fn reader(key: Key, types: &TypeRegistry) -> Function {
    let mut func = Function::new(Signature {
        params: vec![Repr::Ref(RefKind::Opaque)],
        returns: vec![Repr::Tagged],
        ..Signature::default()
    });
    let object = func.block(func.entry).expect("entry").params[0];
    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, types, entry);

    let hit = b.create_block();
    let miss = b.create_block();
    b.add_block_param(hit, Repr::Tagged);
    let cache = b.declare_cache();
    b.cached_get(object, key, cache, (hit, &[]), (miss, &[]))
        .expect("well formed");

    let value = func.block(hit).expect("exists").params[0];
    let mut b = FuncBuilder::new(&mut func, types, hit);
    b.ret(&[value]);

    // Absent means whatever the client says. Here: a number nothing else returns.
    let mut b = FuncBuilder::new(&mut func, types, miss);
    let absent = b.declare_const(rts_cranelift::ir::ConstDecl::Scalar {
        repr: Repr::Tagged,
        bits: rts_cranelift::ir::ScalarBits(-1i64 as u64),
    });
    let absent = b.use_const(absent);
    b.ret(&[absent]);

    func
}

fn compile(func: &Function, bases: RegionBases, types: &TypeRegistry) -> *const u8 {
    let mut funcs = FuncRegistry::new();
    let shape = funcs.declare_signature(Signature {
        params: vec![Repr::Ref(RefKind::Opaque)],
        returns: vec![Repr::Tagged],
        ..Signature::default()
    });
    let id = funcs.declare_function(shape);

    let isa = host_isa().expect("host");
    let mut builder = JITBuilder::with_isa(isa, cranelift_module::default_libcall_names());
    builder.symbol(RtEntry::CacheResolve.symbol(), resolve as *const u8);
    builder.symbol(RtEntry::CacheResolveIndirect.symbol(), refuse as *const u8);
    builder.symbol(RtEntry::Alloc.symbol(), never as *const u8);
    builder.symbol(RtEntry::WriteBarrier.symbol(), never_barrier as *const u8);
    let mut jit = JITModule::new(builder);

    let machine_id = {
        let mut module = MachineModule::new(&mut jit).with_heap(bases);
        module
            .declare(id, "read", Linkage::Export, &funcs)
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
fn a_site_asks_once_and_then_stops_asking() {
    let _serial = reset();
    let mut keys = KeyRegistry::new();
    let mut shapes = ShapeTree::new();
    let mut types = TypeRegistry::new();

    let x = keys.declare_one();
    let shape = shapes
        .transition(shapes.root(), x, Repr::Tagged)
        .expect("added");
    let ty = shapes.layout(shape, &mut types);
    let layout = ObjectLayout::of(ty, &types);

    let bases = heap(layout.size.max(32), 4);
    unsafe { write_object(0, ty, &layout, &[1234]) };
    teach(ty, x, &layout, shapes.slot_of(shape, x).expect("there"));

    let func = reader(x, &types);
    assert_eq!(verify(&func, &types, &FuncRegistry::new()), vec![]);

    let address = compile(&func, bases, &types);
    let read: extern "C" fn(i64) -> i64 = unsafe { std::mem::transmute(address) };

    for _ in 0..10 {
        assert_eq!(read(0), 1234);
    }
    assert_eq!(
        ASKS.load(Ordering::SeqCst),
        1,
        "ten reads of one layout is one question, or the site did not remember"
    );
}

#[test]
fn a_site_that_sees_two_layouts_asks_again_and_stays_right() {
    let _serial = reset();
    let mut keys = KeyRegistry::new();
    let mut shapes = ShapeTree::new();
    let mut types = TypeRegistry::new();

    let x = keys.declare_one();
    let other = keys.declare_one();

    // The same property, at different positions in two layouts.
    let first = shapes
        .transition(shapes.root(), x, Repr::Tagged)
        .expect("added");
    let second = shapes
        .transition(shapes.root(), other, Repr::Tagged)
        .expect("added");
    let second = shapes.transition(second, x, Repr::Tagged).expect("added");

    let first_ty = shapes.layout(first, &mut types);
    let second_ty = shapes.layout(second, &mut types);
    let first_layout = ObjectLayout::of(first_ty, &types);
    let second_layout = ObjectLayout::of(second_ty, &types);

    let bases = heap(second_layout.size.max(32), 4);
    unsafe {
        write_object(0, first_ty, &first_layout, &[11]);
        write_object(1, second_ty, &second_layout, &[99, 22]);
    }
    teach(
        first_ty,
        x,
        &first_layout,
        shapes.slot_of(first, x).expect("there"),
    );
    teach(
        second_ty,
        x,
        &second_layout,
        shapes.slot_of(second, x).expect("there"),
    );

    let func = reader(x, &types);
    let address = compile(&func, bases, &types);
    let read: extern "C" fn(i64) -> i64 = unsafe { std::mem::transmute(address) };

    assert_eq!(read(0), 11, "the first layout, at its own position");
    assert_eq!(read(1), 22, "the second, at a different position");
    assert_eq!(read(0), 11, "and back, still right");

    assert_eq!(
        ASKS.load(Ordering::SeqCst),
        3,
        "each change of layout is one question, and nothing in between is"
    );
}

#[test]
fn a_layout_without_the_property_takes_the_miss_path_and_leaves_the_site_cold() {
    let _serial = reset();
    let mut keys = KeyRegistry::new();
    let mut shapes = ShapeTree::new();
    let mut types = TypeRegistry::new();

    let x = keys.declare_one();
    let unrelated = keys.declare_one();

    let has = shapes
        .transition(shapes.root(), x, Repr::Tagged)
        .expect("added");
    let lacks = shapes
        .transition(shapes.root(), unrelated, Repr::Tagged)
        .expect("added");

    let has_ty = shapes.layout(has, &mut types);
    let lacks_ty = shapes.layout(lacks, &mut types);
    let has_layout = ObjectLayout::of(has_ty, &types);
    let lacks_layout = ObjectLayout::of(lacks_ty, &types);

    let bases = heap(has_layout.size.max(32), 4);
    unsafe {
        write_object(0, has_ty, &has_layout, &[5]);
        write_object(1, lacks_ty, &lacks_layout, &[7]);
    }
    teach(
        has_ty,
        x,
        &has_layout,
        shapes.slot_of(has, x).expect("there"),
    );

    let func = reader(x, &types);
    let address = compile(&func, bases, &types);
    let read: extern "C" fn(i64) -> i64 = unsafe { std::mem::transmute(address) };

    assert_eq!(
        read(1),
        -1,
        "it does not have the property, so the miss path"
    );
    assert_eq!(
        read(1),
        -1,
        "and asking again is the honest cost of not being able to remember it"
    );
    assert_eq!(ASKS.load(Ordering::SeqCst), 2);

    assert_eq!(
        read(0),
        5,
        "a layout that does have it is still read correctly afterwards"
    );
}

/// A refusal the resolver remembers must cost the site one call and no more,
/// and must never become a load at a negative offset.
///
/// This is the machine's half of the fix for `derived.bp()`, where the property
/// is two links away — further than the resolver may walk. Before it, the site
/// recognised nothing, called on every pass, and paid the whole attempt on top
/// of the miss path it was already paying.
#[test]
fn a_remembered_refusal_takes_the_miss_path_without_asking_again() {
    let _serial = reset();
    let mut keys = KeyRegistry::new();
    let mut shapes = ShapeTree::new();
    let mut types = TypeRegistry::new();

    let x = keys.declare_one();
    let unrelated = keys.declare_one();
    let lacks = shapes
        .transition(shapes.root(), unrelated, Repr::Tagged)
        .expect("added");
    let lacks_ty = shapes.layout(lacks, &mut types);
    let lacks_layout = ObjectLayout::of(lacks_ty, &types);

    let bases = heap(lacks_layout.size.max(32), 4);
    unsafe {
        write_object(0, lacks_ty, &lacks_layout, &[7]);
    }

    let func = reader_indirect(x, &types);
    let address = compile(&func, bases, &types);
    let read: extern "C" fn(i64) -> i64 = unsafe { std::mem::transmute(address) };

    for _ in 0..8 {
        assert_eq!(
            read(0),
            -1,
            "a remembered refusal answers the miss path, not a value read at a \
             negative offset"
        );
    }
    assert_eq!(
        ASKS.load(Ordering::SeqCst),
        1,
        "asked once — the whole point of remembering the refusal"
    );
}

/// The same as [`reader`], through the terminator that may answer out of an
/// inherited cell.
fn reader_indirect(key: Key, types: &TypeRegistry) -> Function {
    let mut func = Function::new(Signature {
        params: vec![Repr::Ref(RefKind::Opaque)],
        returns: vec![Repr::Tagged],
        ..Signature::default()
    });
    let object = func.block(func.entry).expect("entry").params[0];
    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, types, entry);

    let hit = b.create_block();
    let miss = b.create_block();
    b.add_block_param(hit, Repr::Tagged);
    let cache = b.declare_cache();
    b.cached_get_indirect(object, key, cache, (hit, &[]), (miss, &[]))
        .expect("well formed");

    let value = func.block(hit).expect("exists").params[0];
    let mut b = FuncBuilder::new(&mut func, types, hit);
    b.ret(&[value]);

    let mut b = FuncBuilder::new(&mut func, types, miss);
    let absent = b.declare_const(rts_cranelift::ir::ConstDecl::Scalar {
        repr: Repr::Tagged,
        bits: rts_cranelift::ir::ScalarBits(-1i64 as u64),
    });
    let absent = b.use_const(absent);
    b.ret(&[absent]);

    func
}

#[test]
fn a_site_that_has_never_run_recognizes_nothing() {
    let _serial = reset();
    let mut keys = KeyRegistry::new();
    let mut shapes = ShapeTree::new();
    let mut types = TypeRegistry::new();

    // The first layout ever declared, whose identity is zero — which is what a
    // cell initialized to zero would claim to recognize.
    let x = keys.declare_one();
    let shape = shapes
        .transition(shapes.root(), x, Repr::Tagged)
        .expect("added");
    let ty = shapes.layout(shape, &mut types);
    assert_eq!(
        ty.index(),
        0,
        "test premise: this is the layout numbered zero"
    );

    let layout = ObjectLayout::of(ty, &types);
    let bases = heap(layout.size.max(32), 2);
    unsafe { write_object(0, ty, &layout, &[42]) };
    teach(ty, x, &layout, shapes.slot_of(shape, x).expect("there"));

    let func = reader(x, &types);
    let address = compile(&func, bases, &types);
    let read: extern "C" fn(i64) -> i64 = unsafe { std::mem::transmute(address) };

    assert_eq!(read(0), 42);
    assert_eq!(
        ASKS.load(Ordering::SeqCst),
        1,
        "a cell starting at zero would have recognized layout zero and read at \
         whatever offset happened to be beside it, without asking anything"
    );
}

#[test]
fn two_sites_remember_separately() {
    let _serial = reset();
    let mut keys = KeyRegistry::new();
    let mut shapes = ShapeTree::new();
    let mut types = TypeRegistry::new();

    let x = keys.declare_one();
    let first = shapes
        .transition(shapes.root(), x, Repr::Tagged)
        .expect("added");
    let second_key = keys.declare_one();
    let second = shapes
        .transition(shapes.root(), second_key, Repr::Tagged)
        .expect("added");
    let second = shapes.transition(second, x, Repr::Tagged).expect("added");

    let first_ty = shapes.layout(first, &mut types);
    let second_ty = shapes.layout(second, &mut types);
    let first_layout = ObjectLayout::of(first_ty, &types);
    let second_layout = ObjectLayout::of(second_ty, &types);

    let bases = heap(second_layout.size.max(32), 4);
    unsafe {
        write_object(0, first_ty, &first_layout, &[1]);
        write_object(1, second_ty, &second_layout, &[0, 2]);
    }
    teach(
        first_ty,
        x,
        &first_layout,
        shapes.slot_of(first, x).expect("there"),
    );
    teach(
        second_ty,
        x,
        &second_layout,
        shapes.slot_of(second, x).expect("there"),
    );

    // One function, two sites, each reading the same property.
    let mut func = Function::new(Signature {
        params: vec![Repr::Ref(RefKind::Opaque), Repr::Ref(RefKind::Opaque)],
        returns: vec![Repr::Tagged],
        ..Signature::default()
    });
    let params = func.block(func.entry).expect("entry").params.clone();
    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);

    let after_first = b.create_block();
    let second_site = b.create_block();
    let after_second = b.create_block();
    let gave_up = b.create_block();
    b.add_block_param(after_first, Repr::Tagged);
    b.add_block_param(after_second, Repr::Tagged);

    let one = b.declare_cache();
    let two = b.declare_cache();
    b.cached_get(params[0], x, one, (after_first, &[]), (gave_up, &[]))
        .expect("well formed");

    let mut b = FuncBuilder::new(&mut func, &types, after_first);
    b.jump(second_site, &[]).expect("no parameters");

    let mut b = FuncBuilder::new(&mut func, &types, second_site);
    b.cached_get(params[1], x, two, (after_second, &[]), (gave_up, &[]))
        .expect("well formed");

    let value = func.block(after_second).expect("exists").params[0];
    let mut b = FuncBuilder::new(&mut func, &types, after_second);
    b.ret(&[value]);

    let mut b = FuncBuilder::new(&mut func, &types, gave_up);
    let absent = b.declare_const(rts_cranelift::ir::ConstDecl::Scalar {
        repr: Repr::Tagged,
        bits: rts_cranelift::ir::ScalarBits(-1i64 as u64),
    });
    let absent = b.use_const(absent);
    b.ret(&[absent]);

    assert_eq!(verify(&func, &types, &FuncRegistry::new()), vec![]);

    let mut funcs = FuncRegistry::new();
    let shape = funcs.declare_signature(Signature {
        params: vec![Repr::Ref(RefKind::Opaque), Repr::Ref(RefKind::Opaque)],
        returns: vec![Repr::Tagged],
        ..Signature::default()
    });
    let id = funcs.declare_function(shape);

    let isa = host_isa().expect("host");
    let mut builder = JITBuilder::with_isa(isa, cranelift_module::default_libcall_names());
    builder.symbol(RtEntry::CacheResolve.symbol(), resolve as *const u8);
    builder.symbol(RtEntry::Alloc.symbol(), never as *const u8);
    builder.symbol(RtEntry::WriteBarrier.symbol(), never_barrier as *const u8);
    let mut jit = JITModule::new(builder);
    let machine_id = {
        let mut module = MachineModule::new(&mut jit).with_heap(bases);
        module
            .declare(id, "two_sites", Linkage::Export, &funcs)
            .expect("declared");
        module.define(id, &func, &funcs, &types).expect("defined");
        module.declarations().machine_id(id).expect("declared")
    };
    jit.finalize_definitions().expect("finalized");
    let address = jit.get_finalized_function(machine_id);
    std::mem::forget(jit);

    let both: extern "C" fn(i64, i64) -> i64 = unsafe { std::mem::transmute(address) };
    for _ in 0..5 {
        assert_eq!(both(0, 1), 2);
    }

    assert_eq!(
        ASKS.load(Ordering::SeqCst),
        2,
        "each site learned its own layout once; one shared cell would have made \
         them take turns forgetting"
    );
}
