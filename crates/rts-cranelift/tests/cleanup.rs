//! Cleanup chains, run.
//!
//! A cleanup is copied into every path that unwinds through it, rather than
//! jumped to. These tests check that the copies happen, in the right order, on
//! both the throwing path and the ordinary one — and that the rules which make
//! copying sound are enforced rather than assumed.

use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Mutex, MutexGuard};

use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::Linkage;
use rts_cranelift::ir::{
    ConstDecl, FuncBuilder, FuncRegistry, Function, ScalarBits, Signature, ValueId,
};
use rts_cranelift::repr::Repr;
use rts_cranelift::symbols::RtEntry;
use rts_cranelift::target::{MachineModule, host_isa};
use rts_cranelift::types::TypeRegistry;
use rts_cranelift::unwind::{Handler, Tag};
use rts_cranelift::verify::{VerifyError, verify};

/// What the cleanups did, in the order they did it.
static TRACE: Mutex<Vec<i64>> = Mutex::new(Vec::new());
static SERIAL: Mutex<()> = Mutex::new(());
static UNUSED: AtomicI64 = AtomicI64::new(0);

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
    UNUSED.fetch_add(1, Ordering::SeqCst)
}

extern "C" fn never(_a: i64, _b: i64) -> i64 {
    unreachable!("these programs do not allocate")
}

extern "C" fn never_barrier(_a: i64, _b: i64) {
    unreachable!("these programs store no references")
}

extern "C" fn never_throw(_tag: i64, _value: i64) {
    unreachable!("nothing here escapes")
}

fn jit() -> JITModule {
    let isa = host_isa().expect("host");
    let mut builder = JITBuilder::with_isa(isa, cranelift_module::default_libcall_names());
    builder.symbol(RtEntry::PromiseSettle.symbol(), record as *const u8);
    builder.symbol(RtEntry::PromiseNew.symbol(), promise_new as *const u8);
    builder.symbol(RtEntry::Alloc.symbol(), never as *const u8);
    builder.symbol(RtEntry::WriteBarrier.symbol(), never_barrier as *const u8);
    builder.symbol(RtEntry::Throw.symbol(), never_throw as *const u8);
    JITModule::new(builder)
}

fn param(func: &Function, index: usize) -> ValueId {
    func.block(func.entry).expect("entry exists").params[index]
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

/// Compiles one function into memory and returns its address.
fn compile(name: &str, params: &[Repr], returns: &[Repr], func: &Function) -> *const u8 {
    let types = TypeRegistry::new();
    let mut funcs = FuncRegistry::new();
    let shape = funcs.declare_signature(Signature {
        params: params.to_vec(),
        returns: returns.to_vec(),
        ..Signature::default()
    });
    let id = funcs.declare_function(shape);

    let mut module_owner = jit();
    let machine_id = {
        let mut module = MachineModule::new(&mut module_owner);
        module
            .declare(id, name, Linkage::Export, &funcs)
            .expect("declared");
        module.define(id, func, &funcs, &types).expect("defined");
        module.declarations().machine_id(id).expect("declared")
    };
    module_owner.finalize_definitions().expect("finalized");
    let address = module_owner.get_finalized_function(machine_id);
    std::mem::forget(module_owner);
    address
}

#[test]
fn a_throw_runs_its_cleanup_before_reaching_the_handler() {
    let _serial = reset();
    let types = TypeRegistry::new();

    let mut func = Function::new(Signature {
        params: vec![Repr::Tagged],
        returns: vec![Repr::Tagged],
        ..Signature::default()
    });
    let value = param(&func, 0);
    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
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

    write_cleanup(&mut func, &types, cleanup, 7);

    let caught = func.block(handler).expect("exists").params[0];
    let mut b = FuncBuilder::new(&mut func, &types, handler);
    b.ret(&[caught]);

    assert_eq!(verify(&func, &types, &FuncRegistry::new()), vec![]);

    let address = compile("careful", &[Repr::Tagged], &[Repr::Tagged], &func);
    let careful: extern "C" fn(i64) -> i64 = unsafe { std::mem::transmute(address) };
    assert_eq!(careful(99), 99, "the value still reaches the handler");
    assert_eq!(trace(), vec![7], "and the scope was undone on the way");
}

#[test]
fn nested_cleanups_run_innermost_first() {
    let _serial = reset();
    let types = TypeRegistry::new();

    let mut func = Function::new(Signature {
        params: vec![Repr::Tagged],
        returns: vec![Repr::Tagged],
        ..Signature::default()
    });
    let value = param(&func, 0);
    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);

    let handler = b.create_block();
    b.add_block_param(handler, Repr::Tagged);
    let outer_cleanup = b.create_block();
    let inner_cleanup = b.create_block();

    b.open_region(
        vec![Handler {
            tag: Tag(1),
            block: handler,
        }],
        Some(outer_cleanup),
    );
    // Nested by being opened inside the outer one -- the parent is derived,
    // so there is no second place to state which region encloses which.
    b.open_region(vec![], Some(inner_cleanup));
    b.throw(Tag(1), value);

    write_cleanup(&mut func, &types, inner_cleanup, 1);
    write_cleanup(&mut func, &types, outer_cleanup, 2);

    let caught = func.block(handler).expect("exists").params[0];
    let mut b = FuncBuilder::new(&mut func, &types, handler);
    b.ret(&[caught]);

    let address = compile("nested", &[Repr::Tagged], &[Repr::Tagged], &func);
    let nested: extern "C" fn(i64) -> i64 = unsafe { std::mem::transmute(address) };
    nested(0);

    assert_eq!(
        trace(),
        vec![1, 2],
        "undoing after an outer scope already ran is indistinguishable from not undoing"
    );
}

#[test]
fn leaving_a_region_normally_owes_the_same_cleanup() {
    let _serial = reset();
    let types = TypeRegistry::new();

    let mut func = Function::new(Signature {
        params: vec![Repr::Tagged],
        returns: vec![Repr::Tagged],
        ..Signature::default()
    });
    let value = param(&func, 0);
    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let cleanup = b.create_block();
    let _region = b.open_region(vec![], Some(cleanup));
    b.ret(&[value]);

    write_cleanup(&mut func, &types, cleanup, 5);

    let address = compile("normal", &[Repr::Tagged], &[Repr::Tagged], &func);
    let normal: extern "C" fn(i64) -> i64 = unsafe { std::mem::transmute(address) };
    assert_eq!(normal(3), 3);
    assert_eq!(
        trace(),
        vec![5],
        "a scope that only unwinds correctly when something goes wrong leaks the rest \
         of the time"
    );
}

#[test]
fn two_paths_through_one_cleanup_each_get_their_own_copy() {
    let _serial = reset();
    let types = TypeRegistry::new();

    let mut func = Function::new(Signature {
        params: vec![Repr::Bool, Repr::Tagged],
        returns: vec![Repr::Tagged],
        ..Signature::default()
    });
    let (cond, value) = (param(&func, 0), param(&func, 1));
    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);

    // The handler and the cleanup are made before the region opens, so they
    // are outside it. The two paths are made after, so they are inside — which
    // is the whole discipline, and it is now the order of the calls rather than
    // a list somebody has to keep right.
    let handler = b.create_block();
    b.add_block_param(handler, Repr::Tagged);
    let cleanup = b.create_block();

    let _region = b.open_region(
        vec![Handler {
            tag: Tag(1),
            block: handler,
        }],
        Some(cleanup),
    );
    let throwing = b.create_block();
    let returning = b.create_block();
    b.branch(cond, (throwing, &[]), (returning, &[]))
        .expect("proven boolean");

    let mut b = FuncBuilder::new(&mut func, &types, throwing);
    b.throw(Tag(1), value);

    let mut b = FuncBuilder::new(&mut func, &types, returning);
    b.ret(&[value]);

    write_cleanup(&mut func, &types, cleanup, 8);

    let caught = func.block(handler).expect("exists").params[0];
    let mut b = FuncBuilder::new(&mut func, &types, handler);
    b.ret(&[caught]);

    let address = compile("both", &[Repr::Bool, Repr::Tagged], &[Repr::Tagged], &func);
    let both: extern "C" fn(i64, i64) -> i64 = unsafe { std::mem::transmute(address) };

    both(1, 11);
    assert_eq!(trace(), vec![8], "the throwing path ran its copy");

    reset_trace();
    both(0, 22);
    assert_eq!(trace(), vec![8], "and so did the returning one");
}

fn reset_trace() {
    TRACE.lock().unwrap_or_else(|p| p.into_inner()).clear();
}

#[test]
fn a_cleanup_that_does_not_end_as_one_is_rejected() {
    let types = TypeRegistry::new();
    let mut func = Function::new(Signature::default());
    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let cleanup = b.create_block();
    let _region = b.open_region(vec![], Some(cleanup));
    b.ret(&[]);

    // Ends by returning, which would give the copy a second exit.
    let mut b = FuncBuilder::new(&mut func, &types, cleanup);
    b.ret(&[]);

    assert!(
        verify(&func, &types, &FuncRegistry::new())
            .iter()
            .any(|e| matches!(e, VerifyError::CleanupDoesNotEnd { .. })),
        "a copy with two exits is not a copy of one thing"
    );
}

#[test]
fn a_cleanup_that_reads_something_outside_itself_is_rejected() {
    let types = TypeRegistry::new();
    let mut func = Function::new(Signature {
        params: vec![Repr::Tagged],
        ..Signature::default()
    });
    // Computed somewhere that is neither the cleanup nor the function's entry.
    // A parameter would *not* be rejected, and should not be: the entry
    // dominates every block, so its values exist wherever a copy lands. This
    // one does not dominate anything, which is the case the rule is about.
    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let cleanup = b.create_block();
    let elsewhere = b.create_block();
    let _region = b.open_region(vec![], Some(cleanup));
    b.jump(elsewhere, &[]).expect("no parameters");

    let mut b = FuncBuilder::new(&mut func, &types, elsewhere);
    let outside = b.declare_const(ConstDecl::Scalar {
        repr: Repr::Tagged,
        bits: ScalarBits(1),
    });
    let outside = b.use_const(outside);
    b.ret(&[]);

    let mut b = FuncBuilder::new(&mut func, &types, cleanup);
    let promise = b.promise_new();
    b.promise_settle(promise, outside, false);
    b.cleanup_done();

    assert!(
        verify(&func, &types, &FuncRegistry::new())
            .iter()
            .any(|e| matches!(e, VerifyError::CleanupReadsOutsideItself { .. })),
        "copying it somewhere that value was never computed would read what is not there"
    );
}

#[test]
fn a_trap_inside_a_cleanup_is_not_an_exit_from_it() {
    // A trap has no successor, and the rule above rejects a terminator with
    // none — but for a reason that does not reach this one: what it refuses is
    // leaving the COPY through a path the unwind knows nothing about. A trap
    // leaves through no path at all, and nothing reaches it.
    //
    // It is how a guard's impossible branch is spelled, which makes it
    // ordinary rather than exotic: `s === "b"` guards its operand to `Bool`
    // and traps on the side that cannot happen. Refusing it refused every
    // strict comparison written inside a `finally`.
    let types = TypeRegistry::new();
    let mut func = Function::new(Signature {
        params: vec![Repr::Tagged],
        ..Signature::default()
    });
    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let cleanup = b.create_block();
    let _region = b.open_region(vec![], Some(cleanup));
    b.ret(&[]);

    let mut b = FuncBuilder::new(&mut func, &types, cleanup);
    let dead = b.create_block();
    let leaving = b.create_block();
    let flag = b.declare_const(ConstDecl::Scalar {
        repr: Repr::Bool,
        bits: ScalarBits(1),
    });
    let flag = b.use_const(flag);
    b.branch(flag, (leaving, &[]), (dead, &[]))
        .expect("no parameters");

    let mut b = FuncBuilder::new(&mut func, &types, dead);
    b.trap(rts_cranelift::ir::TrapCode::Unreachable);

    let mut b = FuncBuilder::new(&mut func, &types, leaving);
    b.cleanup_done();

    assert!(
        !verify(&func, &types, &FuncRegistry::new())
            .iter()
            .any(|e| matches!(e, VerifyError::CleanupDoesNotEnd { .. })),
        "a block nothing reaches is not a way out of the copy"
    );
}

#[test]
fn a_handler_in_the_region_does_not_hide_the_entry_from_the_cleanup() {
    // The case the dominance rule gets wrong if it is written naively.
    //
    // A handler block has NO predecessor in this graph: the unwinder enters it,
    // and the unwinder is not an edge. Running the usual `intersect over the
    // predecessors` step on one intersects over nothing and leaves it dominated
    // by itself alone — which then says the function's own ENTRY does not
    // dominate it, and every value defined at entry stops being readable from
    // the cleanup. Measured as every `try`/`catch` in the corpus reporting
    // `CleanupReadsOutsideItself` about the environment pointer.
    let types = TypeRegistry::new();
    let mut func = Function::new(Signature {
        params: vec![Repr::Tagged],
        ..Signature::default()
    });
    let held = param(&func, 0);
    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let cleanup = b.create_block();
    let _region = b.open_region(
        vec![Handler {
            tag: Tag(1),
            block: cleanup,
        }],
        Some(cleanup),
    );
    let handler = b.create_block();
    b.ret(&[]);

    let mut b = FuncBuilder::new(&mut func, &types, handler);
    b.ret(&[]);

    let mut b = FuncBuilder::new(&mut func, &types, cleanup);
    let promise = b.promise_new();
    b.promise_settle(promise, held, false);
    b.cleanup_done();

    assert!(
        !verify(&func, &types, &FuncRegistry::new())
            .iter()
            .any(|e| matches!(e, VerifyError::CleanupReadsOutsideItself { .. })),
        "a block the unwinder enters has no predecessor, and that is not the same \
         thing as the entry not dominating it"
    );
}

#[test]
fn a_cleanup_may_read_what_dominates_the_region_it_belongs_to() {
    // The complement of the test above, and the reason the rule needed a real
    // dominance analysis rather than "the entry block only".
    //
    // `middle` is not the function's entry, so the old rule refused what it
    // defines. But `middle` dominates every block the region protects — it IS
    // the block the region was opened in — so the value has been computed at
    // every point that can unwind into this cleanup. Refusing it refused an
    // ordinary program: a `for (let i = …)` whose body has a `try`/`finally`
    // creates the loop's per-pass environment inside the loop, and a `finally`
    // reading `i` reads exactly this shape.
    let types = TypeRegistry::new();
    let mut func = Function::new(Signature {
        params: vec![Repr::Tagged],
        ..Signature::default()
    });
    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let cleanup = b.create_block();
    let middle = b.create_block();
    b.jump(middle, &[]).expect("no parameters");

    let mut b = FuncBuilder::new(&mut func, &types, middle);
    let held = b.declare_const(ConstDecl::Scalar {
        repr: Repr::Tagged,
        bits: ScalarBits(1),
    });
    let held = b.use_const(held);
    let _region = b.open_region(vec![], Some(cleanup));
    b.ret(&[]);

    let mut b = FuncBuilder::new(&mut func, &types, cleanup);
    let promise = b.promise_new();
    b.promise_settle(promise, held, false);
    b.cleanup_done();

    assert!(
        !verify(&func, &types, &FuncRegistry::new())
            .iter()
            .any(|e| matches!(e, VerifyError::CleanupReadsOutsideItself { .. })),
        "a definition that dominates the whole region has run wherever the copy lands"
    );
}

#[test]
fn ending_as_a_cleanup_without_being_one_is_rejected() {
    let types = TypeRegistry::new();
    let mut func = Function::new(Signature::default());
    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    b.cleanup_done();

    assert!(
        verify(&func, &types, &FuncRegistry::new())
            .iter()
            .any(|e| matches!(e, VerifyError::CleanupEndOutsideCleanup { .. })),
        "handing control back to an unwind that is not happening means nothing"
    );
}

#[test]
fn a_cleanup_with_parameters_is_rejected() {
    let types = TypeRegistry::new();
    let mut func = Function::new(Signature::default());
    let entry = func.entry;
    let mut b = FuncBuilder::new(&mut func, &types, entry);
    let cleanup = b.create_block();
    b.add_block_param(cleanup, Repr::Tagged);
    let _region = b.open_region(vec![], Some(cleanup));
    b.ret(&[]);

    let mut b = FuncBuilder::new(&mut func, &types, cleanup);
    b.cleanup_done();

    assert!(
        verify(&func, &types, &FuncRegistry::new())
            .iter()
            .any(|e| matches!(e, VerifyError::CleanupTakesParameters { .. })),
        "every path that unwinds through it has nothing in common to pass"
    );
}

#[test]
fn a_cleanup_of_several_blocks_is_copied_whole() {
    // A cleanup used to be one block, and nothing a language runs on the way
    // out fits in one: `x + y` alone emits a fast path and a slow one, and a
    // disposer is a call. So the piece is now every block reachable from the
    // entry without leaving through a `CleanupDone` — and the copy has to bring
    // all of it, branches and merges included, not only the entry.
    let _serial = reset();
    let types = TypeRegistry::new();

    let mut func = Function::new(Signature {
        params: vec![Repr::Bool, Repr::Tagged],
        returns: vec![Repr::Tagged],
        ..Signature::default()
    });
    let value = param(&func, 1);
    let entry = func.entry;

    let mut b = FuncBuilder::new(&mut func, &types, entry);
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

    // The cleanup: a branch on a condition it computes itself, two arms, and a
    // merge that leaves once. Every value it reads it defines, which is the
    // rule that makes the copy sound and is unchanged by there being four
    // blocks instead of one.
    let mut b = FuncBuilder::new(&mut func, &types, cleanup);
    let yes = b.create_block();
    let no = b.create_block();
    let done = b.create_block();
    let flag = b.declare_const(ConstDecl::Scalar {
        repr: Repr::Bool,
        bits: ScalarBits(1),
    });
    let flag = b.use_const(flag);
    b.branch(flag, (yes, &[]), (no, &[])).expect("a boolean");

    for (block, marker) in [(yes, 21u64), (no, 22)] {
        let mut b = FuncBuilder::new(&mut func, &types, block);
        let promise = b.promise_new();
        let held = b.declare_const(ConstDecl::Scalar {
            repr: Repr::Tagged,
            bits: ScalarBits(marker),
        });
        let held = b.use_const(held);
        b.promise_settle(promise, held, false);
        b.jump(done, &[]).expect("no parameters");
    }

    let mut b = FuncBuilder::new(&mut func, &types, done);
    b.cleanup_done();

    let caught = func.block(handler).expect("exists").params[0];
    let mut b = FuncBuilder::new(&mut func, &types, handler);
    b.ret(&[caught]);

    assert_eq!(verify(&func, &types, &FuncRegistry::new()), vec![]);

    let address = compile(
        "branchy",
        &[Repr::Bool, Repr::Tagged],
        &[Repr::Tagged],
        &func,
    );
    let branchy: extern "C" fn(i64, i64) -> i64 = unsafe { std::mem::transmute(address) };
    assert_eq!(branchy(1, 5), 5, "the value still reaches the handler");
    assert_eq!(
        trace(),
        vec![21],
        "and the arm the cleanup's own branch chose is the one that ran"
    );
}
