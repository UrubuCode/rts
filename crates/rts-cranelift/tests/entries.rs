//! Runtime entry points, resolved and called.
//!
//! Allocation and the write barrier are the two operations this layer cannot
//! emit as instructions. These tests provide a runtime for them — a bump
//! allocator and a barrier that counts — compile a program that uses both, run
//! it, and check that the runtime was actually reached.
//!
//! That last part is the claim worth testing. A barrier that is documented as
//! unforgettable and never emitted is exactly the failure this file exists to
//! catch, and it was a real one until this commit.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};

use cranelift_jit::JITBuilder;
use cranelift_module::Linkage;
use rts_cranelift::ir::{FuncBuilder, FuncRegistry, Function, Region, Signature, ValueId};
use rts_cranelift::mem::{ObjectLayout, RegionBase, RegionBases};
use rts_cranelift::repr::{RefKind, Repr};
use rts_cranelift::symbols::RtEntry;
use rts_cranelift::target::{MachineModule, host_isa};
use rts_cranelift::types::TypeRegistry;

/// Where the test heap starts, and how far it has been handed out.
static HEAP_BASE: AtomicU64 = AtomicU64::new(0);
static NEXT_SLOT: AtomicU64 = AtomicU64::new(0);
static BARRIERS: AtomicU64 = AtomicU64::new(0);

/// How far apart objects sit in the test heap.
const STRIDE: u64 = 64;

/// How many objects the test heap holds.
const CAPACITY: u64 = 16;

/// A bump allocator, standing in for a real one.
///
/// Returns an index rather than an address, which is what a reference is here.
/// Ignores the size it is given, because every slot in this heap is the same
/// width — a real allocator would not, and this one records the type so that the
/// header is written the way the layout says.
extern "C" fn test_alloc(_size: i64, type_id: i64) -> i64 {
    let slot = NEXT_SLOT.fetch_add(1, Ordering::SeqCst);
    assert!(
        slot < CAPACITY,
        "the test heap is not meant to be exhausted"
    );

    let base = HEAP_BASE.load(Ordering::SeqCst);
    let address = (base + slot * STRIDE) as *mut i64;
    unsafe { address.write(type_id) };
    slot as i64
}

/// A barrier that only records that it was reached.
extern "C" fn test_barrier(_object: i64, _value: i64) {
    BARRIERS.fetch_add(1, Ordering::SeqCst);
}

/// Serializes the tests that share the heap below.
///
/// The runtime a compiled program calls is reached by address, so it cannot take
/// a parameter saying which test it belongs to — which means these tests share
/// one allocator and one barrier counter. Running them at once made two of them
/// fail on counts that were right for the process and wrong for the test. Taking
/// a lock is the honest fix; making the assertions vaguer would have hidden it.
static SERIAL: Mutex<()> = Mutex::new(());

/// A heap the test owns, and the bases that describe it.
fn prepare_heap() -> (RegionBases, MutexGuard<'static, ()>) {
    let guard = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let bytes = vec![0u8; (STRIDE * CAPACITY) as usize];
    let leaked = Box::leak(bytes.into_boxed_slice());
    let base = leaked.as_ptr() as u64;

    HEAP_BASE.store(base, Ordering::SeqCst);
    NEXT_SLOT.store(0, Ordering::SeqCst);
    BARRIERS.store(0, Ordering::SeqCst);

    (
        RegionBases::single(RegionBase::Immediate(base), STRIDE as u32),
        guard,
    )
}

/// Executable memory that knows where the runtime's entry points live.
///
/// This is the whole difference between the two destinations: here an address is
/// registered before anything compiles, because there is no linker to ask. An
/// object file registers nothing and leaves the name undefined.
fn jit_with_runtime() -> cranelift_jit::JITModule {
    let isa = host_isa().expect("host");
    let mut builder = JITBuilder::with_isa(isa, cranelift_module::default_libcall_names());
    builder.symbol(RtEntry::Alloc.symbol(), test_alloc as *const u8);
    builder.symbol(RtEntry::WriteBarrier.symbol(), test_barrier as *const u8);
    cranelift_jit::JITModule::new(builder)
}

fn param(func: &Function, index: usize) -> ValueId {
    func.block(func.entry).expect("entry exists").params[index]
}

#[test]
fn allocation_reaches_the_runtime_and_returns_a_usable_reference() {
    let (bases, _serial) = prepare_heap();
    let mut types = TypeRegistry::new();
    let cell = types.declare(&[Repr::I64]);

    let mut funcs = FuncRegistry::new();
    let shape = funcs.declare_signature(Signature {
        params: vec![Repr::I64],
        returns: vec![Repr::I64],
        ..Signature::default()
    });
    let id = funcs.declare_function(shape);

    // Allocate, write the argument into the object, read it back out.
    let mut func = Function::new(Signature {
        params: vec![Repr::I64],
        returns: vec![Repr::I64],
        ..Signature::default()
    });
    let value = param(&func, 0);
    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let object = b.alloc(cell, Region::Local);
    b.field_store(object, cell, 0, value).expect("field exists");
    let read = b.field_load(object, cell, 0).expect("field exists");
    b.ret(&[read]);

    let mut jit = jit_with_runtime();
    let machine_id = {
        let mut module = MachineModule::new(&mut jit).with_heap(bases);
        module
            .declare(id, "roundtrip", Linkage::Export, &funcs)
            .expect("declared");
        module
            .define(id, &func, &funcs, &types)
            .expect("allocation is a call, and the call is declared");
        module.declarations().machine_id(id).expect("declared")
    };
    jit.finalize_definitions().expect("finalized");
    let address = jit.get_finalized_function(machine_id);
    std::mem::forget(jit);

    let roundtrip: extern "C" fn(i64) -> i64 = unsafe { std::mem::transmute(address) };
    assert_eq!(roundtrip(1234), 1234);
    assert_eq!(
        NEXT_SLOT.load(Ordering::SeqCst),
        1,
        "the program asked the runtime for space exactly once"
    );
}

#[test]
fn allocation_writes_the_type_into_the_header() {
    let (bases, _serial) = prepare_heap();
    let mut types = TypeRegistry::new();
    let first = types.declare(&[Repr::I64]);
    let second = types.declare(&[Repr::I64, Repr::I64]);

    let mut funcs = FuncRegistry::new();
    let shape = funcs.declare_signature(Signature {
        returns: vec![Repr::I64],
        ..Signature::default()
    });
    let id = funcs.declare_function(shape);

    let mut func = Function::new(Signature {
        returns: vec![Repr::I64],
        ..Signature::default()
    });
    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    b.alloc(first, Region::Local);
    let object = b.alloc(second, Region::Local);
    b.ret(&[object]);

    let mut jit = jit_with_runtime();
    let machine_id = {
        let mut module = MachineModule::new(&mut jit).with_heap(bases);
        module
            .declare(id, "two", Linkage::Export, &funcs)
            .expect("declared");
        module.define(id, &func, &funcs, &types).expect("defined");
        module.declarations().machine_id(id).expect("declared")
    };
    jit.finalize_definitions().expect("finalized");
    let address = jit.get_finalized_function(machine_id);
    std::mem::forget(jit);

    let two: extern "C" fn() -> i64 = unsafe { std::mem::transmute(address) };
    let reference = two();
    assert_eq!(reference, 1, "the second allocation got the second slot");

    let header = unsafe {
        let base = HEAP_BASE.load(Ordering::SeqCst);
        ((base + reference as u64 * STRIDE) as *const i64).read()
    };
    assert_eq!(
        header,
        second.index() as i64,
        "the header records which type an object is, which is what a collector reads"
    );
}

#[test]
fn storing_a_reference_reaches_the_barrier() {
    let (bases, _serial) = prepare_heap();
    let mut types = TypeRegistry::new();
    // A field the collector must trace, so a store into it owes a barrier.
    let holder = types.declare(&[Repr::Ref(RefKind::Opaque)]);

    let mut funcs = FuncRegistry::new();
    let shape = funcs.declare_signature(Signature {
        params: vec![Repr::Ref(RefKind::Opaque)],
        returns: vec![],
        ..Signature::default()
    });
    let id = funcs.declare_function(shape);

    let mut func = Function::new(Signature {
        params: vec![Repr::Ref(RefKind::Opaque)],
        returns: vec![],
        ..Signature::default()
    });
    let value = param(&func, 0);
    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let object = b.alloc(holder, Region::Local);
    b.field_store(object, holder, 0, value)
        .expect("field exists");
    b.ret(&[]);

    let mut jit = jit_with_runtime();
    let machine_id = {
        let mut module = MachineModule::new(&mut jit).with_heap(bases);
        module
            .declare(id, "hold", Linkage::Export, &funcs)
            .expect("declared");
        module.define(id, &func, &funcs, &types).expect("defined");
        module.declarations().machine_id(id).expect("declared")
    };
    jit.finalize_definitions().expect("finalized");
    let address = jit.get_finalized_function(machine_id);
    std::mem::forget(jit);

    let hold: extern "C" fn(i64) = unsafe { std::mem::transmute(address) };
    hold(0);

    assert_eq!(
        BARRIERS.load(Ordering::SeqCst),
        1,
        "a barrier documented as unforgettable has to actually be emitted"
    );
}

#[test]
fn storing_a_number_reaches_no_barrier() {
    let (bases, _serial) = prepare_heap();
    let mut types = TypeRegistry::new();
    // A field of numbers: a store into it can never create a reference.
    let counter = types.declare(&[Repr::I64]);

    let mut funcs = FuncRegistry::new();
    let shape = funcs.declare_signature(Signature {
        params: vec![Repr::I64],
        returns: vec![],
        ..Signature::default()
    });
    let id = funcs.declare_function(shape);

    let mut func = Function::new(Signature {
        params: vec![Repr::I64],
        returns: vec![],
        ..Signature::default()
    });
    let value = param(&func, 0);
    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let object = b.alloc(counter, Region::Local);
    b.field_store(object, counter, 0, value)
        .expect("field exists");
    b.ret(&[]);

    let mut jit = jit_with_runtime();
    let machine_id = {
        let mut module = MachineModule::new(&mut jit).with_heap(bases);
        module
            .declare(id, "count", Linkage::Export, &funcs)
            .expect("declared");
        module.define(id, &func, &funcs, &types).expect("defined");
        module.declarations().machine_id(id).expect("declared")
    };
    jit.finalize_definitions().expect("finalized");
    let address = jit.get_finalized_function(machine_id);
    std::mem::forget(jit);

    let count: extern "C" fn(i64) = unsafe { std::mem::transmute(address) };
    count(7);

    assert_eq!(
        BARRIERS.load(Ordering::SeqCst),
        0,
        "the barrier follows from what the field is, so a number needs none"
    );
}

#[test]
fn the_object_layout_and_the_test_heap_agree() {
    let mut types = TypeRegistry::new();
    let ty = types.declare(&[Repr::I64, Repr::I64]);
    let layout = ObjectLayout::of(ty, &types);

    assert!(
        (layout.size as u64) <= STRIDE,
        "a test whose objects overlap would pass for the wrong reason"
    );
}
