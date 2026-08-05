//! Promises, awaiting and throwing, compiled and run.
//!
//! The runtime here is a stand-in — promises are slots in an array, awaiting
//! reads one, throwing records what was thrown — but the question these tests
//! answer is not what a real runtime would do. It is whether a compiled program
//! reaches it, with the right arguments, at the right times.

use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};

use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::Linkage;
use rts_cranelift::ir::{
    ConstDecl, FuncBuilder, FuncRegistry, Function, ScalarBits, Signature, ValueId,
};
use rts_cranelift::lower::{Capability, LowerError};
use rts_cranelift::repr::{RefKind, Repr};
use rts_cranelift::symbols::RtEntry;
use rts_cranelift::target::{MachineModule, TargetError, host_isa};
use rts_cranelift::types::TypeRegistry;
use rts_cranelift::unwind::{Handler, Tag};

static PROMISES: AtomicU64 = AtomicU64::new(0);
static SETTLED_WITH: AtomicI64 = AtomicI64::new(0);
static SETTLED_REJECTED: AtomicI64 = AtomicI64::new(0);
static AWAITED: AtomicI64 = AtomicI64::new(0);

/// Serializes the tests, which share one stand-in runtime.
static SERIAL: Mutex<()> = Mutex::new(());

fn reset() -> MutexGuard<'static, ()> {
    let guard = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    PROMISES.store(0, Ordering::SeqCst);
    SETTLED_WITH.store(0, Ordering::SeqCst);
    SETTLED_REJECTED.store(0, Ordering::SeqCst);
    AWAITED.store(0, Ordering::SeqCst);
    guard
}

extern "C" fn promise_new() -> i64 {
    PROMISES.fetch_add(1, Ordering::SeqCst) as i64
}

extern "C" fn promise_settle(promise: i64, value: i64, rejected: i64) {
    SETTLED_WITH.store(value, Ordering::SeqCst);
    SETTLED_REJECTED.store(rejected, Ordering::SeqCst);
    let _ = promise;
}

/// Stands in for parking: a real one would not return until the promise settled.
extern "C" fn promise_await(promise: i64) -> i64 {
    AWAITED.fetch_add(1, Ordering::SeqCst);
    promise
}

/// A real throw does not return, and lowering emits a trap after it saying so.
/// A stand-in that returned would hit that trap, which is the emitted code being
/// right rather than the test being wrong — so these tests check that the entry
/// point was reached for rather than watching it run.
extern "C" fn throw(_tag: i64, _value: i64) {
    unreachable!("a throw does not return")
}

extern "C" fn unused_alloc(_size: i64, _ty: i64) -> i64 {
    unreachable!("these programs do not allocate")
}

extern "C" fn unused_barrier(_object: i64, _value: i64) {
    unreachable!("these programs store no references")
}

fn jit_with_runtime() -> JITModule {
    let isa = host_isa().expect("host");
    let mut builder = JITBuilder::with_isa(isa, cranelift_module::default_libcall_names());
    builder.symbol(RtEntry::PromiseNew.symbol(), promise_new as *const u8);
    builder.symbol(RtEntry::PromiseSettle.symbol(), promise_settle as *const u8);
    builder.symbol(RtEntry::PromiseAwait.symbol(), promise_await as *const u8);
    builder.symbol(RtEntry::Throw.symbol(), throw as *const u8);
    builder.symbol(RtEntry::Alloc.symbol(), unused_alloc as *const u8);
    builder.symbol(RtEntry::WriteBarrier.symbol(), unused_barrier as *const u8);
    JITModule::new(builder)
}

fn param(func: &Function, index: usize) -> ValueId {
    func.block(func.entry).expect("entry exists").params[index]
}

/// Compiles one function and reports which runtime entry points it reached for.
///
/// Entry points are declared on first use, so what a compilation declared is
/// exactly what its code calls. That is a structural check, and cheaper than
/// arranging to observe a side effect — and for a throw it is the only safe one,
/// because a real throw does not come back.
fn entries_reached(
    name: &str,
    params: &[Repr],
    returns: &[Repr],
    build: impl FnOnce(&mut Function, &TypeRegistry),
) -> Vec<RtEntry> {
    let types = TypeRegistry::new();
    let mut funcs = FuncRegistry::new();
    let shape = funcs.declare_signature(Signature {
        params: params.to_vec(),
        returns: returns.to_vec(),
        ..Signature::default()
    });
    let id = funcs.declare_function(shape);

    let mut func = Function::new(Signature {
        params: params.to_vec(),
        returns: returns.to_vec(),
        ..Signature::default()
    });
    build(&mut func, &types);

    let mut jit = jit_with_runtime();
    let mut module = MachineModule::new(&mut jit);
    module
        .declare(id, name, Linkage::Export, &funcs)
        .expect("declared");
    module.define(id, &func, &funcs, &types).expect("defined");

    RtEntry::ALL
        .iter()
        .copied()
        .filter(|entry| module.entries().is_declared(*entry))
        .collect()
}

/// Compiles one function into memory and returns its address.
fn compile(
    name: &str,
    params: &[Repr],
    returns: &[Repr],
    build: impl FnOnce(&mut Function, &TypeRegistry),
) -> *const u8 {
    let types = TypeRegistry::new();
    let mut funcs = FuncRegistry::new();
    let shape = funcs.declare_signature(Signature {
        params: params.to_vec(),
        returns: returns.to_vec(),
        ..Signature::default()
    });
    let id = funcs.declare_function(shape);

    let mut func = Function::new(Signature {
        params: params.to_vec(),
        returns: returns.to_vec(),
        ..Signature::default()
    });
    build(&mut func, &types);

    let mut jit = jit_with_runtime();
    let machine_id = {
        let mut module = MachineModule::new(&mut jit);
        module
            .declare(id, name, Linkage::Export, &funcs)
            .expect("declared");
        module.define(id, &func, &funcs, &types).expect("defined");
        module.declarations().machine_id(id).expect("declared")
    };
    jit.finalize_definitions().expect("finalized");
    let address = jit.get_finalized_function(machine_id);
    std::mem::forget(jit);
    address
}

#[test]
fn creating_a_promise_reaches_the_scheduler() {
    let _serial = reset();

    let address = compile("make", &[], &[Repr::Ref(RefKind::Opaque)], |func, types| {
        let entry = func.entry;
        let mut b = FuncBuilder::new(func, types, entry);
        let promise = b.promise_new();
        b.ret(&[promise]);
    });

    let make: extern "C" fn() -> i64 = unsafe { std::mem::transmute(address) };
    assert_eq!(
        make(),
        0,
        "the runtime decides what a promise is, not the program"
    );
    assert_eq!(PROMISES.load(Ordering::SeqCst), 1);
}

#[test]
fn settling_carries_the_value_and_whether_it_failed() {
    let _serial = reset();

    let address = compile("settle", &[Repr::Tagged], &[], |func, types| {
        let value = param(func, 0);
        let entry = func.entry;
        let mut b = FuncBuilder::new(func, types, entry);
        let promise = b.promise_new();
        b.promise_settle(promise, value, true);
        b.ret(&[]);
    });

    let settle: extern "C" fn(i64) = unsafe { std::mem::transmute(address) };
    settle(77);

    assert_eq!(SETTLED_WITH.load(Ordering::SeqCst), 77);
    assert_eq!(
        SETTLED_REJECTED.load(Ordering::SeqCst),
        1,
        "one settlement with two outcomes, not two mechanisms"
    );
}

#[test]
fn awaiting_is_one_node_and_yields_what_the_promise_carried() {
    let _serial = reset();

    let address = compile("wait", &[], &[Repr::Tagged], |func, types| {
        let entry = func.entry;
        let mut b = FuncBuilder::new(func, types, entry);
        let promise = b.promise_new();
        let delivered = b.await_(promise);
        b.ret(&[delivered]);
    });

    // A suspending function is what awaits, so the shape has to say so.
    let wait: extern "C" fn() -> i64 = unsafe { std::mem::transmute(address) };
    let _ = wait();
    assert_eq!(AWAITED.load(Ordering::SeqCst), 1);
}

#[test]
fn a_throw_nothing_catches_reaches_the_runtime() {
    let _serial = reset();

    let reached = entries_reached("boom", &[Repr::Tagged], &[], |func, types| {
        let value = param(func, 0);
        let entry = func.entry;
        let mut b = FuncBuilder::new(func, types, entry);
        b.throw(Tag(9), value);
    });

    assert!(
        reached.contains(&RtEntry::Throw),
        "nothing here catches it, so finding out whose problem it is falls to the runtime"
    );
}

#[test]
fn a_throw_a_handler_catches_never_reaches_the_runtime() {
    let _serial = reset();

    let reached = entries_reached("caught", &[Repr::Tagged], &[Repr::Tagged], |func, types| {
        let value = param(func, 0);
        let entry = func.entry;
        let mut b = FuncBuilder::new(func, types, entry);

        let handler = b.create_block();
        b.add_block_param(handler, Repr::Tagged);
        b.open_region(
            vec![Handler {
                tag: Tag(1),
                block: handler,
            }],
            None,
        );
        b.throw(Tag(1), value);

        let caught = func.block(handler).expect("exists").params[0];
        let mut b = FuncBuilder::new(func, types, handler);
        b.ret(&[caught]);
    });

    assert!(
        !reached.contains(&RtEntry::Throw),
        "the destination was known while compiling, so the runtime is never asked"
    );
}

#[test]
fn a_throw_runs_the_cleanup_it_owes_on_the_way_out() {
    let _serial = reset();

    let reached = entries_reached(
        "careful",
        &[Repr::Tagged],
        &[Repr::Tagged],
        |func, types| {
            let value = param(func, 0);
            let entry = func.entry;
            let mut b = FuncBuilder::new(func, types, entry);

            let handler = b.create_block();
            b.add_block_param(handler, Repr::Tagged);
            let cleanup = b.create_block();

            b.open_region(
                vec![Handler {
                    tag: Tag(1),
                    block: handler,
                }],
                Some(cleanup),
            );
            b.throw(Tag(1), value);

            // The cleanup settles a promise, which is only a way of being seen. It
            // reads nothing from around it, which is what lets it be copied.
            let mut b = FuncBuilder::new(func, types, cleanup);
            let promise = b.promise_new();
            let marker = b.declare_const(ConstDecl::Scalar {
                repr: Repr::Tagged,
                bits: ScalarBits(1234),
            });
            let marker = b.use_const(marker);
            b.promise_settle(promise, marker, false);
            b.cleanup_done();

            let caught = func.block(handler).expect("exists").params[0];
            let mut b = FuncBuilder::new(func, types, handler);
            b.ret(&[caught]);
        },
    );

    assert_eq!(
        SETTLED_WITH.load(Ordering::SeqCst),
        0,
        "compiling runs nothing; this only says the program was built"
    );
    assert!(
        reached.contains(&RtEntry::PromiseSettle),
        "the cleanup was copied into the path that throws, so what it does is emitted"
    );
    assert!(
        !reached.contains(&RtEntry::Throw),
        "and the handler is still reached without asking the runtime"
    );
}

#[test]
fn a_bare_suspension_still_needs_the_frame_transformation() {
    let types = TypeRegistry::new();
    let mut registry = FuncRegistry::new();
    let shape = registry.declare_signature(Signature {
        may_suspend: true,
        ..Signature::default()
    });
    let id = registry.declare_function(shape);

    let mut func = Function::new(Signature {
        may_suspend: true,
        ..Signature::default()
    });
    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    b.suspend();
    b.ret(&[]);

    let mut jit = jit_with_runtime();
    let mut module = MachineModule::new(&mut jit);
    module
        .declare(id, "yields", Linkage::Export, &registry)
        .expect("declared");

    let error = module
        .define(id, &func, &registry, &types)
        .expect_err("nothing decides when a bare suspension resumes");
    assert!(matches!(
        error,
        TargetError::Lower(LowerError::NotYetLowered {
            needs: Capability::Suspension,
            ..
        })
    ));
}
