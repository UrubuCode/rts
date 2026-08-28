//! **Experiment 9 — what the three per-activation stacks cost, and what one costs.**
//!
//! # The question
//!
//! Every JavaScript call in this engine pushes and pops **three** separate
//! `Vec`s before and after one jump. `crates/rts-core/src/entry/functions.rs`,
//! `called` and `invoke`:
//!
//! ```ignore
//! // called:
//! with_current(|context| {
//!     context.pending_arguments.push(absent);   // the rest/arguments vector
//!     context.pending_counts.push(count);       // how many were written
//! });
//! let produced = invoke(...);                   // invoke pushes a third:
//! //   with_current(|context| { context.callees.push(callee); resolve(...) });
//! //   <the jump>
//! //   with_current(|context| context.callees.pop());
//! with_current(|context| {
//!     context.pending_arguments.pop();
//!     context.pending_counts.pop();
//! });
//! ```
//!
//! Four `with_current` borrows and six `Vec` operations, per call.
//!
//! **Ablating all six in the engine is worth 7.3–10.2 ns** — measured
//! 2026-08-28 over `b83eac1a`, release, and recorded in
//! `docs/codegen/native-call-floor.md` §3a. That ablation is not shippable: a
//! callee's rest parameter and its `arguments` object read those stacks.
//!
//! So the question this answers is the shippable one: **how much of that 7.3–10.2
//! ns comes back if the three stacks become one stack of one struct?** Every
//! activation still records its own callee, its own vector and its own count —
//! nothing is dropped — but there is one capacity check, one length update and
//! one `Vec` pointer load instead of three.
//!
//! # Why this is not the question `action-table-2026-08-26.md` §4 already refuted
//!
//! That one merged the **borrows** and kept three `Vec`s, and bought nothing —
//! `c.m(a)` 26.02 → 25.61. Its own conclusion says why: "it is the stacks that
//! carry the cost". This merges the stacks and is therefore the experiment that
//! was never run, not a repeat of the one that was.
//!
//! # What is being compared
//!
//! 1. **Three `Vec`s, four borrows** — the engine today.
//! 2. **Two `Vec`s, four borrows** — `pending_arguments` and `pending_counts`
//!    merged, `callees` left alone. The conservative half of the change, and the
//!    one that needs no restructuring of who pushes: both are already pushed and
//!    popped by the same function, in lockstep, at every one of their sites.
//! 3. **One `Vec<Activation>`, two borrows** — the whole change. It needs
//!    `invoke` to become the single owner of the push, because `call_with_args`
//!    currently pushes two of the three before calling it.
//! 4. **Nothing pushed, two borrows** — the ablation's shape, as the ceiling.
//! 5. **Nothing reached at all** — the loop and the opaque call, as the floor.
//!
//! The jump is `#[inline(never)] extern "C"`, because that is what a callee is:
//! the optimiser cannot see through it, cannot hoist the thread-local out of the
//! loop, and must treat caller-saved registers as clobbered.
//!
//! # RESULT
//!
//! Filled in by the run; see the table this prints and
//! `docs/codegen/native-call-floor.md` for what the engine then did.

use rts_isolated::{measure, opaque, report};
use std::cell::RefCell;

/// Padding so the context stand-in is a realistic size and `last_mut()`'s
/// scaled index is a real multiply, as it is in the engine.
const FILLER: usize = 40;

/// What one activation records, in the merged shape.
///
/// Three words, which is exactly what the three stacks hold between them: the
/// callee that is running, the vector its rest parameter reads, and how many
/// arguments the site wrote. `Option<usize>` rather than a bare count because
/// `None` is what a native calling another function honestly says.
#[derive(Clone, Copy)]
struct Activation {
    callee: u64,
    arguments: u64,
    count: Option<usize>,
}

struct Context {
    counter: u64,
    /// Shape 1 and 2 and 4.
    callees: Vec<u64>,
    /// Shape 1 and 4.
    pending_arguments: Vec<u64>,
    /// Shape 1 and 4.
    pending_counts: Vec<Option<usize>>,
    /// Shape 2 — the two that already move in lockstep, merged.
    paired: Vec<(u64, Option<usize>)>,
    /// Shape 3 — all three.
    activations: Vec<Activation>,
    _filler: [u64; FILLER],
}

impl Default for Context {
    fn default() -> Self {
        Self {
            counter: 0,
            callees: Vec::new(),
            pending_arguments: Vec::new(),
            pending_counts: Vec::new(),
            paired: Vec::new(),
            activations: Vec::new(),
            _filler: [0; FILLER],
        }
    }
}

thread_local! {
    static CONTEXTS: RefCell<Vec<Context>> = const { RefCell::new(Vec::new()) };
}

fn with_current<T>(body: impl FnOnce(&mut Context) -> T) -> T {
    CONTEXTS.with(|stack| {
        let mut borrowed = stack.borrow_mut();
        let context = borrowed.last_mut().expect("a context is installed");
        body(context)
    })
}

/// The jump. Opaque to the optimiser, exactly as a compiled callee is.
#[inline(never)]
extern "C" fn jump(a0: u64) -> u64 {
    with_current(|context| {
        context.counter = context.counter.wrapping_add(a0);
        context.counter
    })
}

/// Shape 1 — three `Vec`s, four borrows. What the engine does today.
#[inline(never)]
extern "C" fn three_stacks(callee: u64, arguments: u64, count: usize) -> u64 {
    with_current(|context| {
        context.pending_arguments.push(arguments);
        context.pending_counts.push(Some(count));
    });
    let produced = {
        with_current(|context| context.callees.push(callee));
        let produced = jump(callee);
        with_current(|context| {
            context.callees.pop();
        });
        produced
    };
    with_current(|context| {
        context.pending_arguments.pop();
        context.pending_counts.pop();
    });
    produced
}

/// Shape 2 — the two lockstep stacks merged, `callees` untouched.
#[inline(never)]
extern "C" fn two_stacks(callee: u64, arguments: u64, count: usize) -> u64 {
    with_current(|context| context.paired.push((arguments, Some(count))));
    let produced = {
        with_current(|context| context.callees.push(callee));
        let produced = jump(callee);
        with_current(|context| {
            context.callees.pop();
        });
        produced
    };
    with_current(|context| {
        context.paired.pop();
    });
    produced
}

/// Shape 3 — one stack, two borrows. `invoke` owns the push.
#[inline(never)]
extern "C" fn one_stack(callee: u64, arguments: u64, count: usize) -> u64 {
    with_current(|context| {
        context.activations.push(Activation {
            callee,
            arguments,
            count: Some(count),
        });
    });
    let produced = jump(callee);
    with_current(|context| {
        context.activations.pop();
    });
    produced
}

/// Shape 4 — the bookkeeping gone entirely. Not shippable; the ceiling.
#[inline(never)]
extern "C" fn no_stacks(callee: u64, _arguments: u64, _count: usize) -> u64 {
    with_current(|_| ());
    let produced = jump(callee);
    with_current(|_| ());
    produced
}

/// Shape 5 — the loop and the jump, with nothing else. The floor.
#[inline(never)]
extern "C" fn bare(callee: u64, _arguments: u64, _count: usize) -> u64 {
    jump(callee)
}

fn drive(shape: extern "C" fn(u64, u64, usize) -> u64, n: u64) -> u64 {
    let mut sink = 0u64;
    for i in 0..n {
        sink = sink.wrapping_add(shape(opaque(i | 1), opaque(i), opaque(3)));
    }
    sink
}

fn main() {
    CONTEXTS.with(|stack| {
        let mut context = Context::default();
        // Warmed to the depth a running program keeps them at, so a push is a
        // capacity check and a store rather than a growth. The engine's stacks
        // reach a steady depth for the same reason: they are pushed and popped
        // around one jump.
        context.callees.reserve(64);
        context.pending_arguments.reserve(64);
        context.pending_counts.reserve(64);
        context.paired.reserve(64);
        context.activations.reserve(64);
        stack.borrow_mut().push(context);
    });

    let rows = [
        measure("1. three Vecs, four borrows (today)", |n| {
            drive(three_stacks, n)
        }),
        measure("2. two Vecs (args+count paired)", |n| drive(two_stacks, n)),
        measure("3. one Vec<Activation>, two borrows", |n| {
            drive(one_stack, n)
        }),
        measure("4. no bookkeeping (the ceiling)", |n| drive(no_stacks, n)),
        measure("5. jump only (the floor)", |n| drive(bare, n)),
    ];

    report("Experiment 9 - the per-activation stacks", &rows);
}
