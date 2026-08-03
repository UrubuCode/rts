//! The frame transformation, checked and then run.
//!
//! A rewritten function contains no suspension, so it goes through the ordinary
//! pipeline: our verifier accepts it, lowering emits it, and it can be compiled
//! and called. These tests do all three, because a rewrite that produces
//! something only the rewriter understands has not moved the problem anywhere.

use std::sync::atomic::Ordering;

use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::Linkage;
use rts_cranelift::frame::{TransformError, resumable_form};
use rts_cranelift::ir::{FuncBuilder, FuncRegistry, Function, NumOp, Signature, ValueId};
use rts_cranelift::mem::{ObjectLayout, RegionBase, RegionBases};
use rts_cranelift::repr::Repr;
use rts_cranelift::symbols::RtEntry;
use rts_cranelift::target::{MachineModule, host_isa};
use rts_cranelift::types::TypeRegistry;
use rts_cranelift::verify::verify;

/// A function that is allowed to park its frame.
fn suspending(params: &[Repr], returns: &[Repr]) -> Function {
    Function::new(Signature {
        params: params.to_vec(),
        returns: returns.to_vec(),
        may_suspend: true,
        ..Signature::default()
    })
}

fn param(func: &Function, index: usize) -> ValueId {
    func.block(func.entry).expect("entry exists").params[index]
}

extern "C" fn unreachable_entry(_a: i64, _b: i64) -> i64 {
    unreachable!("a rewritten function reaches nothing but its own frame")
}

/// A frame has a slot for what a resumption delivers, and that slot is generic —
/// so writing it is a reference store, and a reference store owes a barrier.
/// A stand-in that refused to be called asserted the opposite, and was wrong.
extern "C" fn counting_barrier(_object: i64, _value: i64) {
    BARRIERS.fetch_add(1, Ordering::SeqCst);
}

static BARRIERS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Compiles a rewritten function and returns its address, plus the frame it uses.
///
/// The frame is one object in a heap the test owns. Leaked, because compiled code
/// holds the heap's address as a constant and has to outlive the test.
fn compile_resumable(
    name: &str,
    resumable: &rts_cranelift::frame::Resumable,
    types: &TypeRegistry,
) -> (*const u8, *mut u8) {
    let layout = ObjectLayout::of(resumable.layout.ty, types);
    let bytes = vec![0u8; layout.size as usize];
    let leaked = Box::leak(bytes.into_boxed_slice());
    let base = leaked.as_ptr() as u64;
    let bases = RegionBases::single(RegionBase::Immediate(base), layout.size);

    let mut funcs = FuncRegistry::new();
    let shape = funcs.declare_signature(resumable.func.signature.clone());
    let id = funcs.declare_function(shape);

    let isa = host_isa().expect("host");
    let mut builder = JITBuilder::with_isa(isa, cranelift_module::default_libcall_names());
    builder.symbol(RtEntry::Alloc.symbol(), unreachable_entry as *const u8);
    builder.symbol(
        RtEntry::WriteBarrier.symbol(),
        counting_barrier as *const u8,
    );
    let mut jit = JITModule::new(builder);

    let machine_id = {
        let mut module = MachineModule::new(&mut jit).with_heap(bases);
        module
            .declare(id, name, Linkage::Export, &funcs)
            .expect("declared");
        module
            .define(id, &resumable.func, &funcs, types)
            .expect("a rewritten function has nothing left that cannot be emitted");
        module.declarations().machine_id(id).expect("declared")
    };
    jit.finalize_definitions().expect("finalized");
    let address = jit.get_finalized_function(machine_id);
    std::mem::forget(jit);

    (address, base as *mut u8)
}

/// Reads a field of the frame, the way the layout says.
unsafe fn read_field(frame: *mut u8, layout: &ObjectLayout, field: u32) -> i64 {
    unsafe {
        let offset = layout.field_offset(field).expect("field exists");
        (frame.offset(offset as isize) as *const i64).read()
    }
}

/// Writes a field of the frame, the way the layout says.
unsafe fn write_field(frame: *mut u8, layout: &ObjectLayout, field: u32, value: i64) {
    unsafe {
        let offset = layout.field_offset(field).expect("field exists");
        (frame.offset(offset as isize) as *mut i64).write(value);
    }
}

#[test]
fn a_function_that_does_not_declare_it_may_park_is_not_rewritten() {
    let mut types = TypeRegistry::new();
    let func = Function::new(Signature::default());

    assert_eq!(
        resumable_form(&func, &mut types).err(),
        Some(TransformError::NotSuspending),
        "rewriting one that did not say it parks would change what it is"
    );
}

#[test]
fn the_rewritten_function_contains_no_suspension_and_verifies() {
    let mut types = TypeRegistry::new();
    let mut func = suspending(&[], &[Repr::Tagged]);
    let entry = func.entry;
    let empty = TypeRegistry::new();
    let mut b = FuncBuilder::new(&mut func, &empty, entry);
    let resumed = b.suspend();
    b.ret(&[resumed]);

    let resumable = resumable_form(&func, &mut types).expect("rewritten");

    let has_suspension = resumable.func.blocks().any(|(_, block)| {
        block
            .insts
            .iter()
            .any(|&i| resumable.func.inst(i).is_some_and(|d| d.inst.is_suspend()))
    });
    assert!(
        !has_suspension,
        "the whole point is that nothing is left to suspend"
    );
    assert_eq!(
        verify(&resumable.func, &types, &FuncRegistry::new()),
        vec![],
        "a rewrite that produced something only the rewriter understands moved nothing"
    );
}

#[test]
fn a_frame_that_never_suspends_finishes_on_its_first_run() {
    let mut types = TypeRegistry::new();
    let mut func = suspending(&[Repr::I64], &[Repr::I64]);
    let x = param(&func, 0);
    let entry = func.entry;
    let empty = TypeRegistry::new();
    let mut b = FuncBuilder::new(&mut func, &empty, entry);
    let doubled = b.arith(NumOp::Add, x, x).expect("proven");
    b.ret(&[doubled]);

    let resumable = resumable_form(&func, &mut types).expect("rewritten");
    let layout = ObjectLayout::of(resumable.layout.ty, &types);
    let (address, frame) = compile_resumable("double", &resumable, &types);

    unsafe {
        write_field(frame, &layout, resumable.layout.param_fields[0], 21);
    }

    let run: extern "C" fn(i64) -> i64 = unsafe { std::mem::transmute(address) };
    assert_eq!(run(0), 1, "nothing suspended, so it finished");
    assert_eq!(
        unsafe { read_field(frame, &layout, resumable.layout.return_fields[0]) },
        42,
        "the result is left in the frame, because a resumed frame returns nowhere"
    );
}

#[test]
fn a_frame_that_suspends_stops_and_can_be_picked_back_up() {
    let mut types = TypeRegistry::new();
    let mut func = suspending(&[Repr::I64], &[Repr::I64]);
    let held = param(&func, 0);
    let entry = func.entry;
    let empty = TypeRegistry::new();
    let mut b = FuncBuilder::new(&mut func, &empty, entry);
    // Suspend, then return a value that was live across it.
    b.suspend();
    b.ret(&[held]);

    let resumable = resumable_form(&func, &mut types).expect("rewritten");
    let layout = ObjectLayout::of(resumable.layout.ty, &types);
    let (address, frame) = compile_resumable("park", &resumable, &types);

    unsafe {
        write_field(frame, &layout, resumable.layout.param_fields[0], 7);
        write_field(frame, &layout, resumable.layout.label_field, 0);
    }

    let run: extern "C" fn(i64) -> i64 = unsafe { std::mem::transmute(address) };

    assert_eq!(run(0), 0, "it parked rather than finishing");
    assert_eq!(
        unsafe { read_field(frame, &layout, resumable.layout.label_field) },
        1,
        "and wrote down where to come back to"
    );

    assert_eq!(run(0), 1, "entered again, it resumed and finished");
    assert_eq!(
        unsafe { read_field(frame, &layout, resumable.layout.return_fields[0]) },
        7,
        "a value live across the suspension survived it"
    );
}

#[test]
fn what_a_resumption_delivers_reaches_the_frame_that_asked() {
    let mut types = TypeRegistry::new();
    let mut func = suspending(&[], &[Repr::Tagged]);
    let entry = func.entry;
    let empty = TypeRegistry::new();
    let mut b = FuncBuilder::new(&mut func, &empty, entry);
    let resumed = b.suspend();
    b.ret(&[resumed]);

    let resumable = resumable_form(&func, &mut types).expect("rewritten");
    let layout = ObjectLayout::of(resumable.layout.ty, &types);
    let (address, frame) = compile_resumable("deliver", &resumable, &types);

    let run: extern "C" fn(i64) -> i64 = unsafe { std::mem::transmute(address) };
    assert_eq!(run(0), 0, "parked");

    unsafe {
        write_field(frame, &layout, resumable.layout.resumed_field, 4242);
    }
    assert_eq!(run(0), 1, "finished");
    assert_eq!(
        unsafe { read_field(frame, &layout, resumable.layout.return_fields[0]) },
        4242,
        "whoever resumes leaves the value in the frame, and the frame reads it"
    );
}

#[test]
fn two_suspensions_get_two_places_to_come_back_to() {
    let mut types = TypeRegistry::new();
    let mut func = suspending(&[], &[Repr::Tagged]);
    let entry = func.entry;
    let empty = TypeRegistry::new();
    let mut b = FuncBuilder::new(&mut func, &empty, entry);
    b.suspend();
    let second = b.suspend();
    b.ret(&[second]);

    let resumable = resumable_form(&func, &mut types).expect("rewritten");
    assert_eq!(resumable.resume_points, 2);

    let layout = ObjectLayout::of(resumable.layout.ty, &types);
    let (address, frame) = compile_resumable("twice", &resumable, &types);
    let run: extern "C" fn(i64) -> i64 = unsafe { std::mem::transmute(address) };

    assert_eq!(run(0), 0);
    assert_eq!(
        unsafe { read_field(frame, &layout, resumable.layout.label_field) },
        1
    );
    assert_eq!(run(0), 0, "the second suspension is a different place");
    assert_eq!(
        unsafe { read_field(frame, &layout, resumable.layout.label_field) },
        2
    );
    assert_eq!(run(0), 1, "and then it finished");
}

#[test]
fn the_frame_holds_a_slot_for_everything_it_needs() {
    let mut types = TypeRegistry::new();
    let mut func = suspending(&[Repr::I64, Repr::F64], &[Repr::I64]);
    let x = param(&func, 0);
    let entry = func.entry;
    let empty = TypeRegistry::new();
    let mut b = FuncBuilder::new(&mut func, &empty, entry);
    b.suspend();
    b.ret(&[x]);

    let resumable = resumable_form(&func, &mut types).expect("rewritten");
    assert_eq!(resumable.layout.param_fields.len(), 2);
    assert_eq!(resumable.layout.return_fields.len(), 1);
    assert!(
        resumable.layout.survives(x),
        "a value read after a suspension has to be written down before it"
    );
}
