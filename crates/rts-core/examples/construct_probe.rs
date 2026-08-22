//! What `new C()` costs, decomposed, with no compiled program around it.
//!
//! # Why this exists beside `entry_probe.rs`
//!
//! `docs/codegen/plan.md` L3 asks for exactly this and names it: *"(C) an
//! `examples/` loop over `entry::construct`"*. `bench/analytic.ts` reports
//! `alloc class instance` at 90.89 ns and nothing says which of the eight
//! `with_current` crossings, the prototype-chain read, the allocation, the
//! prototype link, the jump into the constructor, or the collector's share owns
//! it. This calls the entry points directly so the parts can be subtracted from
//! each other.
//!
//! # How the rows subtract
//!
//! | | what it isolates |
//! |---|---|
//! | `construct(field ctor)` | the whole of `new Callee()`, minus compiled code |
//! | `construct(empty ctor)` | the same without the field write in the body |
//! | `call(plain fn)` | `invoke` alone: two crossings, `resolve`, the jump |
//! | `object_new(0)` | the allocation and nothing else |
//! | `get_property(ctor, "prototype")` | an upper bound on the chain read |
//! | `set_prototype(o, p)` | the `Aside` write that links the fresh object |
//!
//! `construct(empty) - call(plain)` is what `new` costs over a plain call:
//! `allocate_for_target` plus the six extra crossings and the four stack
//! push/pop pairs.
//!
//! # Cold and warm, because they are different operations
//!
//! An allocating row is run twice. **Cold** is a region still bumping: every
//! cell is a first touch of a demand-zero page (`Region::sharded` gets its words
//! from `alloc_zeroed`), so one allocation in thirty-two takes a soft page
//! fault, and that cost is real for a program's first 65 536 objects and absent
//! from every one after.
//!
//! **Warm** is the steady state `bench/analytic.ts` measures. Between the two,
//! [`warm`] allocates far past the region so the collector runs: after it, every
//! allocation comes off the LIFO free list — fifteen zero-word stores into a
//! cell that was just released — and pays one `collect_cycle::release`, whose
//! ~22 `Aside::remove` calls are the sweep. `RTS_GC_DEBUG=1` over 200 000
//! `new Callee()` reports three cycles freeing 63 666 cells each, which is one
//! release per allocation; the warm rows are where that shows up.
//!
//! **The difference between a cold row and its warm twin is the collector.**
//!
//! Absent from both: the calling convention, the throw check after the call, and
//! the inline caches — so a row here is a FLOOR for the same row in
//! `bench/analytic.ts`.
//!
//! Run with `cargo run --release --example construct_probe -p rts-core`.

use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::time::Instant;

use rts_core::entry::{
    Context, call, closure_new, construct, declare_literals, get_property, key_number, mark_derived,
    object_new, set_property, set_prototype, string_const, with_context,
};
use rts_core::value::{Kinds, Singletons, Value};

/// Iterations for a case that allocates nothing.
const EACH: u64 = 200_000;

/// Iterations for a COLD allocating case — one that takes a cell and, because
/// the region is still bumping, never gives it back.
///
/// See `entry_probe.rs`'s own constant for why this cannot run to [`EACH`]:
/// `roots::scan_stack` is conservative and the region holds 65 536 cells. Three
/// cold rows at this count take 27 000 of them, so no cold row reaches a
/// collection — which is the property that makes them *sweep-free*.
const ALLOC_EACH: u64 = 1_000;

/// How far past the region [`warm`] allocates before the warm rows run.
///
/// Two full regions' worth, so at least two cycles have happened and the free
/// list is the only path an allocation takes.
const WARM: u64 = 150_000;

/// Rounds, so the spread between them can be reported.
const ROUNDS: usize = 9;

/// The key `v`, so the constructor body below can write the field.
///
/// A static because the body is an `extern "C"` function the runtime jumps to
/// through a transmuted address — it closes over nothing, exactly as a compiled
/// one does.
static V_KEY: AtomicI64 = AtomicI64::new(0);
/// `undefined`, for the same reason.
static UNDEFINED: AtomicU64 = AtomicU64::new(0);

/// `class Callee { v = 1 }` — what the compiler emits for the body.
extern "C" fn field_ctor(_env: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    set_property(
        this,
        V_KEY.load(Ordering::Relaxed),
        Value::from_f64(1.0).bits(),
    );
    UNDEFINED.load(Ordering::Relaxed)
}

/// `class Empty {}`.
extern "C" fn empty_ctor(_env: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    UNDEFINED.load(Ordering::Relaxed)
}

fn main() {
    if cfg!(debug_assertions) {
        println!("DEBUG BUILD — these are not numbers\n");
    }

    let mut context = Context::new(
        Singletons {
            undefined: 0,
            null: 1,
            hole: 2,
        },
        Kinds {
            symbol: 4,
            bigint: 5,
        },
    );
    let names: Vec<Vec<u16>> = ["prototype", "v"]
        .iter()
        .map(|name| name.encode_utf16().collect())
        .collect();
    declare_literals(&mut context, &names);

    // Without this the collector answers zero and frees nothing —
    // `collect_cycle::collect` returns immediately when `stack_high` is absent —
    // so the region grows to its whole reservation and the program dies with
    // "heap exhausted". That is exactly what happens to `entry_probe.rs`, and
    // what its comment about a conservative scan pinning cells attributes to the
    // wrong cause: nothing was scanned at all.
    //
    // A local of THIS frame is the high end. `main` is the outermost frame that
    // matters, everything measured runs deeper, and a word above this one cannot
    // hold a reference the measurement made.
    let top = 0u64;
    context.stack_high = Some(&top as *const u64 as usize);

    let (_context, ()) = with_context(context, || {
        let undefined = Value::from_singleton(0).bits();
        UNDEFINED.store(undefined, Ordering::Relaxed);

        let prototype_key = key_number(string_const(0));
        V_KEY.store(key_number(string_const(1)), Ordering::Relaxed);

        // Three callables, built once outside every timed region.
        let with_field = closure_new(field_ctor as usize as i64, undefined);
        let without_field = closure_new(empty_ctor as usize as i64, undefined);
        let plain = closure_new(empty_ctor as usize as i64, undefined);
        // A DERIVED constructor: `construct_inner` hands it `undefined` for
        // `this` and never calls `allocate_for_target` at all, because the
        // language says the object comes from its `super()`. So the difference
        // between this row and `construct(empty)` is `allocate_for_target`
        // exactly — the prototype read, the allocation, the type and the link —
        // and nothing else in the path differs by an instruction.
        let derived = closure_new(empty_ctor as usize as i64, undefined);
        mark_derived(derived);

        // The subjects the non-constructing rows read, also built once.
        let ctor_prototype = get_property(with_field, prototype_key);
        let victim = object_new(2);

        // Warm every path once, so that no row pays for a first transition or a
        // first `Aside` growth that the other rows do not.
        std::hint::black_box(construct(with_field, undefined, undefined, undefined, undefined));
        std::hint::black_box(construct(
            without_field,
            undefined,
            undefined,
            undefined,
            undefined,
        ));
        std::hint::black_box(call(plain, undefined, undefined, undefined, undefined, undefined));

        report("floor: empty loop", EACH, |sink, i| {
            sink.wrapping_add(std::hint::black_box(i))
        });
        report("call(plain fn)", EACH, move |sink, _| {
            sink.wrapping_add(call(plain, undefined, undefined, undefined, undefined, undefined))
        });
        report("construct(derived)", EACH, move |sink, _| {
            sink.wrapping_add(construct(derived, undefined, undefined, undefined, undefined))
        });
        report("get_property(ctor,proto)", EACH, move |sink, _| {
            sink.wrapping_add(get_property(with_field, prototype_key))
        });
        report("set_prototype(o,p)", EACH, move |sink, _| {
            sink.wrapping_add(set_prototype(victim, ctor_prototype))
        });
        report("set_property(o,v)", EACH, move |sink, i| {
            sink.wrapping_add(set_property(
                victim,
                V_KEY.load(Ordering::Relaxed),
                Value::from_f64(i as f64).bits(),
            ))
        });
        report("object_new(0) cold", ALLOC_EACH, |sink, _| {
            sink.wrapping_add(object_new(0))
        });
        report("construct(empty) cold", ALLOC_EACH, move |sink, _| {
            sink.wrapping_add(construct(
                without_field,
                undefined,
                undefined,
                undefined,
                undefined,
            ))
        });
        report("construct(field) cold", ALLOC_EACH, move |sink, _| {
            sink.wrapping_add(construct(
                with_field,
                undefined,
                undefined,
                undefined,
                undefined,
            ))
        });

        warm();

        report("object_new(0) warm", EACH, |sink, _| {
            sink.wrapping_add(object_new(0))
        });
        report("construct(empty) warm", EACH, move |sink, _| {
            sink.wrapping_add(construct(
                without_field,
                undefined,
                undefined,
                undefined,
                undefined,
            ))
        });
        report("construct(field) warm", EACH, move |sink, _| {
            sink.wrapping_add(construct(
                with_field,
                undefined,
                undefined,
                undefined,
                undefined,
            ))
        });
    });
}

/// Fills the region past its capacity so the collector runs and the free list
/// becomes the only allocation path.
///
/// Each reference is dropped rather than accumulated: `roots::scan_stack` is
/// conservative, so a loop that SUMS references leaves words on the stack that
/// look like live ones and pins the cells they name. One live at a time is what
/// lets a cycle reclaim essentially everything, which is what the engine's own
/// `RTS_GC_DEBUG=1` reports for the same shape of loop in compiled code.
fn warm() {
    for _ in 0..WARM {
        std::hint::black_box(object_new(0));
    }
}

/// Runs one case for [`ROUNDS`] rounds and prints the minimum with its spread.
///
/// The minimum rather than the mean, and the spread beside it, for the reason
/// `entry_probe.rs` states: the fastest round is the one least interfered with,
/// and a number without its own noise cannot be compared to another number.
fn report(what: &str, each: u64, mut body: impl FnMut(u64, u64) -> u64) {
    let mut best = f64::INFINITY;
    let mut worst: f64 = 0.0;
    let mut sink = 0u64;
    for _ in 0..ROUNDS {
        let at = Instant::now();
        for i in 0..each {
            sink = body(sink, i);
        }
        let nanos = at.elapsed().as_nanos() as f64 / each as f64;
        best = best.min(nanos);
        worst = worst.max(nanos);
    }
    let spread = if best > 0.0 {
        100.0 * (worst - best) / best
    } else {
        0.0
    };
    println!("{what:<26} {best:>9.2} ns/op   spread {spread:>5.1}%   (checksum {sink})");
}
