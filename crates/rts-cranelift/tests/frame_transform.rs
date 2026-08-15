//! The frame transformation, checked and then run.
//!
//! A rewritten function contains no suspension, so it goes through the ordinary
//! pipeline: our verifier accepts it, lowering emits it, and it can be compiled
//! and called. These tests do all three, because a rewrite that produces
//! something only the rewriter understands has not moved the problem anywhere.

use std::sync::atomic::Ordering;
use std::sync::{Mutex, MutexGuard};

use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::Linkage;
use rts_cranelift::frame::{ResumeMode, TransformError, resumable_form};
use rts_cranelift::ir::{
    ConstDecl, FuncBuilder, FuncRegistry, Function, NumOp, ScalarBits, Signature, ValueId,
};
use rts_cranelift::mem::{ObjectLayout, RegionBase, RegionBases};
use rts_cranelift::repr::Repr;
use rts_cranelift::symbols::RtEntry;
use rts_cranelift::target::{MachineModule, host_isa};
use rts_cranelift::types::TypeRegistry;
use rts_cranelift::unwind::{Handler, Tag};
use rts_cranelift::verify::verify;

/// What a resumption of [`ResumeMode::Unwind`] leaves with here.
///
/// A constant of this test rather than of the crate, because the tag is the
/// client's: this layer compares tags for equality and does not interpret them,
/// so it cannot choose one. That is why the rewrite takes it.
const ABRUPT: Tag = Tag(1);

/// What the cleanups did, in the order they did it.
static TRACE: Mutex<Vec<i64>> = Mutex::new(Vec::new());
static SERIAL: Mutex<()> = Mutex::new(());

fn reset() -> MutexGuard<'static, ()> {
    let guard = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
    TRACE.lock().unwrap_or_else(|p| p.into_inner()).clear();
    guard
}

fn trace() -> Vec<i64> {
    TRACE.lock().unwrap_or_else(|p| p.into_inner()).clone()
}

/// Stands in for whatever a cleanup releases: it records that it ran.
extern "C" fn record(_promise: i64, value: i64, _rejected: i64) {
    TRACE.lock().unwrap_or_else(|p| p.into_inner()).push(value);
}

extern "C" fn promise_new() -> i64 {
    0
}

/// A throw that reaches here left the function, and none of these do.
extern "C" fn escaping_throw(_tag: i64, _value: i64) {
    unreachable!("every throw in these programs has a handler in the same function")
}

/// Fills a block with a cleanup that records `marker` and ends properly.
fn write_cleanup(
    func: &mut Function,
    types: &TypeRegistry,
    block: rts_cranelift::ir::BlockId,
    marker: u64,
) {
    let mut b = FuncBuilder::new(func, types, block);
    let promise = b.promise_new();
    let value = b.declare_const(ConstDecl::Scalar {
        repr: Repr::Tagged,
        bits: ScalarBits(marker),
    });
    let value = b.use_const(value);
    b.promise_settle(promise, value, false);
    b.cleanup_done();
}

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
    builder.symbol(RtEntry::PromiseNew.symbol(), promise_new as *const u8);
    builder.symbol(RtEntry::PromiseSettle.symbol(), record as *const u8);
    builder.symbol(RtEntry::Throw.symbol(), escaping_throw as *const u8);
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
        resumable_form(&func, &mut types, ABRUPT).err(),
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

    let resumable = resumable_form(&func, &mut types, ABRUPT).expect("rewritten");

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

    let resumable = resumable_form(&func, &mut types, ABRUPT).expect("rewritten");
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

    let resumable = resumable_form(&func, &mut types, ABRUPT).expect("rewritten");
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

    let resumable = resumable_form(&func, &mut types, ABRUPT).expect("rewritten");
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

    let resumable = resumable_form(&func, &mut types, ABRUPT).expect("rewritten");
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

    let resumable = resumable_form(&func, &mut types, ABRUPT).expect("rewritten");
    assert_eq!(resumable.layout.param_fields.len(), 2);
    assert_eq!(resumable.layout.return_fields.len(), 1);
    assert!(
        resumable.layout.survives(x),
        "a value read after a suspension has to be written down before it"
    );
}

/// An `async function*` still parks after being made resumable, and says so.
///
/// The rewrite removes `Suspend` and keeps `Await`, so the two questions — "did
/// the source suspend" and "does the rewritten form" — have different answers.
/// The signature was built with `Signature::default()`, which answers `false` to
/// the second, and that was invisible while every function rewritten here was a
/// plain `function*` with no `Await` to leave behind. The first `async
/// function*` written refused the whole program with `UndeclaredSuspension`.
#[test]
fn a_rewritten_body_that_still_awaits_declares_that_it_may_park() {
    let mut types = TypeRegistry::new();
    let mut func = suspending(&[Repr::Tagged], &[Repr::Tagged]);
    let promise = param(&func, 0);
    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let waited = b.await_(promise);
    b.suspend();
    b.ret(&[waited]);

    let resumable = resumable_form(&func, &mut types, ABRUPT).expect("it may suspend");
    assert!(
        resumable.func.signature.may_suspend,
        "a body that can still park declaring it cannot is what the verifier refuses"
    );
    assert!(
        verify(&resumable.func, &types, &FuncRegistry::new()).is_empty(),
        "and the refusal is the observable half of it"
    );
}

/// A resumption that unwinds does so AT the suspension, inside its regions.
///
/// This is the whole reason the mode is a field of the frame rather than
/// something the resumer does on its own: from outside, the regions the
/// suspension was written in are no longer entered, so a throw raised there
/// could never reach a handler the parked frame sits inside.
#[test]
fn a_resumption_that_unwinds_lands_in_the_handler_the_suspension_was_inside() {
    let _serial = reset();
    let mut types = TypeRegistry::new();
    let mut func = suspending(&[], &[Repr::Tagged]);
    let entry = func.entry;
    let empty = TypeRegistry::new();

    let mut b = FuncBuilder::new(&mut func, &empty, entry);
    let handler = b.create_block();
    b.add_block_param(handler, Repr::Tagged);
    b.open_region(
        vec![Handler {
            tag: ABRUPT,
            block: handler,
        }],
        None,
    );
    let delivered = b.suspend();
    b.ret(&[delivered]);
    b.close_region();

    // The handler answers something the ordinary path cannot, so that the two
    // are told apart by the value rather than by whether it crashed.
    let mut b = FuncBuilder::new(&mut func, &empty, handler);
    let caught = b.declare_const(ConstDecl::Scalar {
        repr: Repr::Tagged,
        bits: ScalarBits(777),
    });
    let caught = b.use_const(caught);
    b.ret(&[caught]);

    let resumable = resumable_form(&func, &mut types, ABRUPT).expect("rewritten");
    assert_eq!(
        verify(&resumable.func, &types, &FuncRegistry::new()),
        vec![],
        "the dispatch the rewrite writes is ordinary IR or it is not shippable"
    );
    let layout = ObjectLayout::of(resumable.layout.ty, &types);
    let (address, frame) = compile_resumable("unwind_into_handler", &resumable, &types);
    let run: extern "C" fn(i64) -> i64 = unsafe { std::mem::transmute(address) };

    assert_eq!(run(0), 0, "parked inside the protected region");

    unsafe {
        write_field(frame, &layout, resumable.layout.resumed_field, 4242);
        write_field(
            frame,
            &layout,
            resumable.layout.mode_field,
            ResumeMode::Unwind.number() as i64,
        );
    }
    assert_eq!(run(0), 1, "it left the function rather than carrying on");
    assert_eq!(
        unsafe { read_field(frame, &layout, resumable.layout.return_fields[0]) },
        777,
        "the delivered value became a throw at the suspension, so the handler \
         wrapped around it ran — carrying on would have answered 4242"
    );
}

/// A resumption that returns runs what leaving the regions owes.
#[test]
fn a_resumption_that_returns_runs_the_cleanup_it_parked_inside() {
    let _serial = reset();
    let mut types = TypeRegistry::new();
    let mut func = suspending(&[], &[Repr::Tagged]);
    let entry = func.entry;
    let empty = TypeRegistry::new();

    let mut b = FuncBuilder::new(&mut func, &empty, entry);
    let cleanup = b.create_block();
    b.open_region(vec![], Some(cleanup));
    let delivered = b.suspend();
    b.ret(&[delivered]);
    b.close_region();
    write_cleanup(&mut func, &empty, cleanup, 7);

    let resumable = resumable_form(&func, &mut types, ABRUPT).expect("rewritten");
    assert_eq!(verify(&resumable.func, &types, &FuncRegistry::new()), vec![]);
    let layout = ObjectLayout::of(resumable.layout.ty, &types);
    let (address, frame) = compile_resumable("return_through_cleanup", &resumable, &types);
    let run: extern "C" fn(i64) -> i64 = unsafe { std::mem::transmute(address) };

    assert_eq!(run(0), 0, "parked inside the protected region");
    assert_eq!(trace(), Vec::<i64>::new(), "parking is not leaving");

    unsafe {
        write_field(frame, &layout, resumable.layout.resumed_field, 4242);
        write_field(
            frame,
            &layout,
            resumable.layout.mode_field,
            ResumeMode::Return.number() as i64,
        );
    }
    assert_eq!(run(0), 1, "it finished");
    assert_eq!(
        trace(),
        vec![7],
        "and running what leaving the region owes is the entire point of \
         returning AT the suspension rather than dropping the frame"
    );
    assert_eq!(
        unsafe { read_field(frame, &layout, resumable.layout.return_fields[0]) },
        0,
        "the delivered value is NOT written where the function's answer goes: \
         the resumer named it and still holds it, and the slot need not even be \
         the representation it is"
    );
}

/// An ordinary resumption is the one a zeroed record already asks for.
///
/// The pair to the two above, and it is the one that would break silently: a
/// mode field nobody wrote must mean "carry on", or every generator ever made
/// would resume abruptly the first time it was advanced.
#[test]
fn a_resumption_with_nothing_written_in_the_mode_field_carries_on() {
    let mut types = TypeRegistry::new();
    let mut func = suspending(&[], &[Repr::Tagged]);
    let entry = func.entry;
    let empty = TypeRegistry::new();
    let mut b = FuncBuilder::new(&mut func, &empty, entry);
    let delivered = b.suspend();
    b.ret(&[delivered]);

    assert_eq!(
        ResumeMode::default(),
        ResumeMode::Deliver,
        "zero has to be the ordinary way, because a fresh record is zeroed"
    );
    assert_eq!(ResumeMode::Deliver.number(), 0);

    let resumable = resumable_form(&func, &mut types, ABRUPT).expect("rewritten");
    let layout = ObjectLayout::of(resumable.layout.ty, &types);
    let (address, frame) = compile_resumable("deliver_by_default", &resumable, &types);
    let run: extern "C" fn(i64) -> i64 = unsafe { std::mem::transmute(address) };

    assert_eq!(run(0), 0, "parked");
    unsafe {
        write_field(frame, &layout, resumable.layout.resumed_field, 4242);
    }
    assert_eq!(run(0), 1, "finished");
    assert_eq!(
        unsafe { read_field(frame, &layout, resumable.layout.return_fields[0]) },
        4242,
        "nothing wrote the mode, so the delivered value is the suspension's result"
    );
}

/// A plain `function*` no longer parks once its yields are resume labels.
#[test]
fn a_rewritten_body_with_nothing_left_to_await_declares_that_it_cannot() {
    let mut types = TypeRegistry::new();
    let mut func = suspending(&[Repr::Tagged], &[Repr::Tagged]);
    let held = param(&func, 0);
    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    b.suspend();
    b.ret(&[held]);

    let resumable = resumable_form(&func, &mut types, ABRUPT).expect("it may suspend");
    assert!(
        !resumable.func.signature.may_suspend,
        "the rewrite is what removes the suspension, so the permission goes with it"
    );
}
