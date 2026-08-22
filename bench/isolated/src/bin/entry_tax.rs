//! **Experiment 1 — what it costs to reach the context.**
//!
//! # The question
//!
//! Every runtime entry point in `rts-core` reaches the heap the same way:
//!
//! ```ignore
//! pub(crate) fn with_current<T>(body: impl FnOnce(&mut Context) -> T) -> T {
//!     CONTEXTS.with(|stack| {
//!         let mut borrowed = stack.borrow_mut();
//!         let Some(context) = borrowed.last_mut() else { abort() };
//!         body(context)
//!     })
//! }
//! ```
//!
//! — `crates/rts-core/src/entry/current.rs:218`, over
//! `static CONTEXTS: RefCell<Vec<Context>>` at line 162.
//!
//! That is a thread-local access, a `RefCell` borrow (a store, a compare, and a
//! second store when the guard drops), a `Vec` length check and an index. It is
//! paid by *every* operation compiled code cannot do inline, which in
//! `bench/analytic.ts` is a great many rows sitting at 16–30 ns where bun sits
//! at 0.5.
//!
//! **How much of those 16–30 ns is this?** That is the only question here. If it
//! is 2 ns, replacing it is not the answer to anything and the cost is elsewhere
//! — and knowing that is worth as much as the optimisation would have been.
//!
//! # What is being compared
//!
//! Six shapes, all doing the same visible work — read a counter out of the
//! context, add to it, store it back — so that the difference between rows is
//! only the reaching:
//!
//! 1. **`RefCell<Vec<Context>>`** — what the engine does today.
//! 2. **`RefCell<Context>`** — the same borrow discipline without the stack.
//!    Isolates what the `Vec` costs from what the `RefCell` costs.
//! 3. **`Cell<*mut Context>`** — a raw pointer cached in thread-local storage,
//!    which is the change under consideration. The stack still exists; this is a
//!    memo of its top, written when a context is pushed or popped.
//! 4. **`&mut Context` passed in** — no thread-local at all. Not implementable
//!    (`crates/rts-core/src/entry/current.rs:1` says why: the boundary is
//!    `extern "C"` over ABI scalars and a `&mut Context` does not cross it), but
//!    it is the floor, and a floor is what says whether shape 3 is close enough
//!    to be the end of this line of work.
//! 5. **nothing** — the loop and the opaque call, with no context reached at
//!    all. Everything above is measured against this.
//! 6. **shape 3 plus the throw-pending check**, which compiled code performs
//!    after every call that can raise.
//!
//! Each is called through `#[inline(never)] extern "C"`, because that is what an
//! entry point is: the optimiser cannot see into it, cannot hoist the
//! thread-local access out of the caller's loop, and must treat every
//! caller-saved register as clobbered. Measuring the same code inlined would
//! measure a program the engine cannot produce.

use rts_isolated::{measure, opaque, report};
use std::cell::{Cell, RefCell};

/// Stands in for `rts_core::entry::Context`.
///
/// The real one is large — the region, the key registry, the interner, the
/// literal table, a census, a call stack. The size matters to this measurement
/// only through the `Vec`: `last_mut` computes `ptr + (len - 1) * size_of`, and
/// a multiply by a non-power-of-two size is a real instruction. So the field
/// list is padded to a plausible width rather than left at one counter.
#[repr(C)]
struct Context {
    /// The field every shape reads and writes, so that the work is identical.
    resolves: u64,
    /// Stands in for the region, the tables and the rest.
    _rest: [u64; 47],
}

impl Context {
    const fn new() -> Self {
        Context {
            resolves: 0,
            _rest: [0; 47],
        }
    }
}

// ---------------------------------------------------------------- shape 1

thread_local! {
    /// Exactly the engine's declaration: a stack, const-initialised.
    static CONTEXTS: RefCell<Vec<Context>> = const { RefCell::new(Vec::new()) };
}

fn with_current_stack<T>(body: impl FnOnce(&mut Context) -> T) -> T {
    CONTEXTS.with(|stack| {
        let mut borrowed = stack.borrow_mut();
        let Some(context) = borrowed.last_mut() else {
            std::process::abort();
        };
        body(context)
    })
}

#[inline(never)]
extern "C" fn entry_stack(add: u64) -> u64 {
    with_current_stack(|context| {
        context.resolves = context.resolves.wrapping_add(add);
        context.resolves
    })
}

// ---------------------------------------------------------------- shape 2

thread_local! {
    /// One context, still behind a `RefCell`. What the borrow alone costs.
    static SINGLE: RefCell<Context> = const { RefCell::new(Context::new()) };
}

#[inline(never)]
extern "C" fn entry_single(add: u64) -> u64 {
    SINGLE.with(|cell| {
        let mut context = cell.borrow_mut();
        context.resolves = context.resolves.wrapping_add(add);
        context.resolves
    })
}

// ---------------------------------------------------------------- shape 3

thread_local! {
    /// A memo of the stack's top.
    ///
    /// Null until something installs a context, which is the same condition
    /// `with_current` aborts on today — so the check is not new work, it is the
    /// `Option` that `last_mut` already returns, moved to a place that costs one
    /// compare against zero instead of a length load.
    static CURRENT: Cell<*mut Context> = const { Cell::new(std::ptr::null_mut()) };
}

#[inline(never)]
extern "C" fn entry_pointer(add: u64) -> u64 {
    CURRENT.with(|slot| {
        let pointer = slot.get();
        if pointer.is_null() {
            std::process::abort();
        }
        // SAFETY: in the engine this is upheld by the same invariant that makes
        // `last_mut()` sound today — a context is installed for the duration of
        // a program and entry points run only while one is, one thread at a
        // time, and no entry point holds a `&mut Context` across a call that
        // could re-enter. The experiment upholds it by construction.
        let context = unsafe { &mut *pointer };
        context.resolves = context.resolves.wrapping_add(add);
        context.resolves
    })
}

// ---------------------------------------------------------------- shape 4

#[inline(never)]
extern "C" fn entry_argument(context: *mut Context, add: u64) -> u64 {
    // SAFETY: the caller owns the context for the duration of the call.
    let context = unsafe { &mut *context };
    context.resolves = context.resolves.wrapping_add(add);
    context.resolves
}

// ---------------------------------------------------------------- shape 5

/// The call and the loop, reaching nothing. Everything above is this plus the
/// reaching, so this is what gets subtracted to attribute a cost.
#[inline(never)]
extern "C" fn entry_nothing(add: u64) -> u64 {
    opaque(add).wrapping_add(1)
}

// ------------------------------------------------- the throw-pending check

thread_local! {
    /// `crates/rts-core/src/entry/current.rs:67` — the word compiled code reads
    /// after every call that can raise. Measured here because "the asking was
    /// the expensive half" is a claim that file makes about the design it
    /// replaced, and the shape that replaced it has never been priced.
    static THROWN: Cell<i64> = const { Cell::new(0) };
}

#[inline(never)]
extern "C" fn entry_pointer_and_check(add: u64) -> u64 {
    let out = entry_pointer(add);
    if THROWN.with(|slot| slot.get()) != 0 {
        return 0;
    }
    out
}

fn main() {
    // Install a context for shapes 1 and 3, exactly as the host would.
    CONTEXTS.with(|stack| stack.borrow_mut().push(Context::new()));
    CONTEXTS.with(|stack| {
        let mut borrowed = stack.borrow_mut();
        let top: *mut Context = borrowed.last_mut().unwrap();
        CURRENT.with(|slot| slot.set(top));
    });

    let mut owned = Context::new();
    let owned_pointer: *mut Context = &mut owned;

    let rows = vec![
        measure("1. RefCell<Vec<Context>>  (engine today)", |n| {
            let mut acc = 0u64;
            for i in 0..n {
                acc = acc.wrapping_add(entry_stack(opaque(i)));
            }
            acc
        }),
        measure("2. RefCell<Context>       (no stack)", |n| {
            let mut acc = 0u64;
            for i in 0..n {
                acc = acc.wrapping_add(entry_single(opaque(i)));
            }
            acc
        }),
        measure("3. Cell<*mut Context>     (memo of the top)", |n| {
            let mut acc = 0u64;
            for i in 0..n {
                acc = acc.wrapping_add(entry_pointer(opaque(i)));
            }
            acc
        }),
        measure("4. &mut Context passed in (the floor)", |n| {
            let mut acc = 0u64;
            for i in 0..n {
                acc = acc.wrapping_add(entry_argument(owned_pointer, opaque(i)));
            }
            acc
        }),
        measure("5. nothing reached        (call + loop only)", |n| {
            let mut acc = 0u64;
            for i in 0..n {
                acc = acc.wrapping_add(entry_nothing(opaque(i)));
            }
            acc
        }),
        measure("6. shape 3 + throw-pending check", |n| {
            let mut acc = 0u64;
            for i in 0..n {
                acc = acc.wrapping_add(entry_pointer_and_check(opaque(i)));
            }
            acc
        }),
    ];

    report(
        "Experiment 1 - reaching the context from an entry point",
        &rows,
    );
    println!();
    println!("Read rows 1-4 minus row 5 as the cost of REACHING; row 5 is the");
    println!("call and the loop, which no change to the context can remove.");
}
