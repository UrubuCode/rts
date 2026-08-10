//! `rts`'s `atomic` — integer, boolean and float cells addressed by handle.
//!
//! # Reuse-check: this is not [`rts_core::entry::buffers::atomics`]
//!
//! That module IS `Atomics`, the ES read-modify-write over a typed-array
//! element, and its own doc explains why a single-threaded engine can call a
//! plain read-then-write "atomic" honestly. This namespace answers a different
//! call: `atomic.i64_new(10)` hands back a HANDLE to a cell that exists on its
//! own, with no backing typed array — the shape 5 fixtures in this suite call,
//! ported from the old engine's `rts:atomic`. The read-modify-write itself is
//! the same shape `Atomics` already has (read, apply, write, answer the prior
//! value); what differs is where the cell lives, so this module owns the
//! handle table and calls no code in `buffers::atomics` because there is no
//! typed array here to hand it.
//!
//! # Why "atomic" is honest with no lock, restated for this shape
//!
//! One OS thread runs JavaScript in this engine today — `crates/rts-host`
//! has no code that starts a second one to run a callback, and
//! `crates/rts-core`'s `Context` is reached through a thread-local
//! (`entry::current::with_current`), which a second thread could not share
//! without its own `Context`. So nothing can run between this module's read
//! and its write, and every operation below is a plain read-then-write —
//! observably identical to a genuinely atomic one under the one condition that
//! is true today: nothing else in the process is racing it, because nothing
//! else runs at the same time. **The day a second thread can run a callback,
//! this file is the one that has to grow a real lock — the correctness this
//! module states is conditional on that, not permanent.**
//!
//! # Fences
//!
//! `fence_acquire`/`fence_release`/`fence_seq_cst` are accepted and do nothing:
//! a fence orders a thread's own operations against another thread's view, and
//! with one thread there is no other view to order against. They exist so a
//! program written against the old engine's surface still finds every name
//! callable rather than failing on the one three functions it does not need
//! yet.
//!
//! # Handles
//!
//! A handle is a positive integer, never `0` — so `if (a == 0)` after `_new`
//! reads as failure the way the ported fixtures write it. It indexes a single
//! `Vec` shared by all three kinds; `i64_load` on a handle that names a `bool`
//! or `f64` cell (or names nothing, freed or never allocated) answers the same
//! default an `Atomics` refusal does: no raise, because a native here may raise
//! only at the two sites `crates/rts-core/README.md` names, and this is not
//! one of them.

use std::cell::RefCell;

use rts_core::entry::{self, Context, Provided};

/// The `atomic` namespace.
pub fn install(context: &mut Context, surface: u64) {
    let namespace = entry::make_namespace(context, ATOMIC);
    entry::put_member(context, surface, "atomic", namespace);
}

const ATOMIC: &[(&str, Provided)] = &[
    ("i64_new", i64_new),
    ("i64_load", i64_load),
    ("i64_store", i64_store),
    ("i64_fetch_add", i64_fetch_add),
    ("i64_fetch_sub", i64_fetch_sub),
    ("i64_cas", i64_cas),
    ("bool_new", bool_new),
    ("bool_load", bool_load),
    ("bool_store", bool_store),
    ("bool_swap", bool_swap),
    ("f64_new", f64_new),
    ("f64_load", f64_load),
    ("f64_store", f64_store),
    ("fence_acquire", fence_acquire),
    ("fence_release", fence_release),
    ("fence_seq_cst", fence_seq_cst),
];

/// One cell. The variant is the kind check every accessor performs before
/// touching it — a handle presented to the wrong kind's function is refused
/// exactly like one that names nothing at all.
enum Cell {
    I64(i64),
    Bool(bool),
    F64(f64),
}

thread_local! {
    /// One store per thread. There is exactly one thread that runs JavaScript
    /// today (see the module doc), so this is not a sharding compromise — it
    /// is the whole store.
    static CELLS: RefCell<Vec<Option<Cell>>> = RefCell::new(Vec::new());
}

/// Allocates a cell, answers its handle as a plain number (1-based: `0` is
/// never a valid handle).
fn alloc(cell: Cell) -> u64 {
    CELLS.with(|cells| {
        let mut cells = cells.borrow_mut();
        cells.push(Some(cell));
        entry::make_number(cells.len() as f64)
    })
}

/// The 0-based index a handle names, if it is in range at all. Does not check
/// the kind — callers that care check the variant themselves, so an
/// out-of-kind handle reads exactly like a freed or never-allocated one.
fn index_of(handle: u64) -> Option<usize> {
    let at = entry::number_of(handle)?;
    if !at.is_finite() || at < 1.0 {
        return None;
    }
    let at = (at as usize).checked_sub(1)?;
    Some(at)
}

/// Runs `body` over the `i64` cell a handle names, or answers `default` — for
/// a handle that names nothing, or names a cell of a different kind.
fn with_i64<T>(handle: u64, default: T, body: impl FnOnce(&mut i64) -> T) -> T {
    CELLS.with(|cells| {
        let mut cells = cells.borrow_mut();
        match index_of(handle).and_then(|at| cells.get_mut(at)) {
            Some(Some(Cell::I64(value))) => body(value),
            _ => default,
        }
    })
}

/// Same shape, for a `bool` cell.
fn with_bool<T>(handle: u64, default: T, body: impl FnOnce(&mut bool) -> T) -> T {
    CELLS.with(|cells| {
        let mut cells = cells.borrow_mut();
        match index_of(handle).and_then(|at| cells.get_mut(at)) {
            Some(Some(Cell::Bool(value))) => body(value),
            _ => default,
        }
    })
}

/// Same shape, for an `f64` cell.
fn with_f64<T>(handle: u64, default: T, body: impl FnOnce(&mut f64) -> T) -> T {
    CELLS.with(|cells| {
        let mut cells = cells.borrow_mut();
        match index_of(handle).and_then(|at| cells.get_mut(at)) {
            Some(Some(Cell::F64(value))) => body(value),
            _ => default,
        }
    })
}

/// `atomic.i64_new(initial)`.
extern "C" fn i64_new(_e: u64, _this: u64, initial: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let initial = entry::number_of(initial).unwrap_or(0.0) as i64;
    alloc(Cell::I64(initial))
}

/// `atomic.i64_load(handle)`.
extern "C" fn i64_load(_e: u64, _this: u64, handle: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    entry::make_number(with_i64(handle, 0i64, |value| *value) as f64)
}

/// `atomic.i64_store(handle, value)`.
extern "C" fn i64_store(_e: u64, _this: u64, handle: u64, value: u64, _a2: u64, _a3: u64) -> u64 {
    let value = entry::number_of(value).unwrap_or(0.0) as i64;
    with_i64(handle, (), |cell| *cell = value);
    entry::undefined_value()
}

/// `atomic.i64_fetch_add(handle, delta)` — answers the value BEFORE the add.
extern "C" fn i64_fetch_add(_e: u64, _this: u64, handle: u64, delta: u64, _a2: u64, _a3: u64) -> u64 {
    let delta = entry::number_of(delta).unwrap_or(0.0) as i64;
    let prior = with_i64(handle, 0i64, |cell| {
        let prior = *cell;
        *cell = cell.wrapping_add(delta);
        prior
    });
    entry::make_number(prior as f64)
}

/// `atomic.i64_fetch_sub(handle, delta)` — answers the value BEFORE the sub.
extern "C" fn i64_fetch_sub(_e: u64, _this: u64, handle: u64, delta: u64, _a2: u64, _a3: u64) -> u64 {
    let delta = entry::number_of(delta).unwrap_or(0.0) as i64;
    let prior = with_i64(handle, 0i64, |cell| {
        let prior = *cell;
        *cell = cell.wrapping_sub(delta);
        prior
    });
    entry::make_number(prior as f64)
}

/// `atomic.i64_cas(handle, expected, replacement)` — answers the value that
/// was there; stores `replacement` only when it equalled `expected`.
extern "C" fn i64_cas(_e: u64, _this: u64, handle: u64, expected: u64, replacement: u64, _a4: u64) -> u64 {
    let expected = entry::number_of(expected).unwrap_or(0.0) as i64;
    let replacement = entry::number_of(replacement).unwrap_or(0.0) as i64;
    let prior = with_i64(handle, 0i64, |cell| {
        let prior = *cell;
        if prior == expected {
            *cell = replacement;
        }
        prior
    });
    entry::make_number(prior as f64)
}

/// `atomic.bool_new(initial)`.
extern "C" fn bool_new(_e: u64, _this: u64, initial: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    alloc(Cell::Bool(entry::to_boolean(initial)))
}

/// `atomic.bool_load(handle)`.
extern "C" fn bool_load(_e: u64, _this: u64, handle: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    entry::boolean_value(with_bool(handle, false, |value| *value))
}

/// `atomic.bool_store(handle, value)`.
extern "C" fn bool_store(_e: u64, _this: u64, handle: u64, value: u64, _a2: u64, _a3: u64) -> u64 {
    let value = entry::to_boolean(value);
    with_bool(handle, (), |cell| *cell = value);
    entry::undefined_value()
}

/// `atomic.bool_swap(handle, value)` — answers the value BEFORE the swap.
extern "C" fn bool_swap(_e: u64, _this: u64, handle: u64, value: u64, _a2: u64, _a3: u64) -> u64 {
    let value = entry::to_boolean(value);
    let prior = with_bool(handle, false, |cell| {
        let prior = *cell;
        *cell = value;
        prior
    });
    entry::boolean_value(prior)
}

/// `atomic.f64_new(initial)`.
extern "C" fn f64_new(_e: u64, _this: u64, initial: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    alloc(Cell::F64(entry::number_of(initial).unwrap_or(0.0)))
}

/// `atomic.f64_load(handle)`.
extern "C" fn f64_load(_e: u64, _this: u64, handle: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    entry::make_number(with_f64(handle, 0.0, |value| *value))
}

/// `atomic.f64_store(handle, value)`.
extern "C" fn f64_store(_e: u64, _this: u64, handle: u64, value: u64, _a2: u64, _a3: u64) -> u64 {
    let value = entry::number_of(value).unwrap_or(0.0);
    with_f64(handle, (), |cell| *cell = value);
    entry::undefined_value()
}

/// `atomic.fence_acquire()` — a no-op. See the module doc.
extern "C" fn fence_acquire(_e: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    entry::undefined_value()
}

/// `atomic.fence_release()` — a no-op. See the module doc.
extern "C" fn fence_release(_e: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    entry::undefined_value()
}

/// `atomic.fence_seq_cst()` — a no-op. See the module doc.
extern "C" fn fence_seq_cst(_e: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    entry::undefined_value()
}
