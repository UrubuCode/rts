//! `performance.timerify(fn[, options])` and the top-level alias.
//!
//! # What was believed impossible here, and what actually answers it
//!
//! Both this module's previous documentation and `rts-std`'s
//! `globals/timing.rs` refuse `timerify` with the same sentence: it needs a
//! native closure over the wrapped function, and `entry::make_callable` hands
//! back a bare function pointer with no environment slot. That is true of
//! `make_callable` and false of the surface — [`rts_core::entry::closure_new`]
//! takes a code address AND an environment, and `entry/native.rs` is explicit
//! that a Rust function may be a callee's code because it has exactly the
//! compiled shape. The environment arrives as the callee's **first argument**,
//! which is the `_e` slot every native in this crate ignores.
//!
//! So a timed wrapper is an ordinary closure whose code happens to be Rust,
//! and nothing new was invented for it. This is the reuse-check finding that
//! changed the answer.
//!
//! # Which duration is recorded
//!
//! The synchronous one: the time to return, not the time for a returned
//! promise to settle. The reference doc (§5.3) marks this `(verify)` against
//! real Node and asks for the literal reading, which is what this is —
//! wrapping an `async fn` therefore measures how long it took to reach its
//! first `await`, and that is stated rather than assumed correct.

use rts_core::entry;

use super::timeline::{self, Record};

/// Where the wrapped function and the optional histogram live inside the
/// closure's environment.
const TARGET: f64 = 0.0;
const HISTOGRAM: f64 = 1.0;

/// `timerify(fn[, options])`.
///
/// A non-callable `fn` answers `undefined` rather than a wrapper that would
/// fail at every call site (Node throws a `TypeError`).
pub(super) extern "C" fn timerify(
    _e: u64,
    _this: u64,
    target: u64,
    options: u64,
    _a2: u64,
    _a3: u64,
) -> u64 {
    let prepared = entry::with_runtime(|context| {
        if !entry::is_callable_in(context, target) {
            return None;
        }
        let histogram = entry::get_member(context, options, "histogram");
        let name = entry::get_member(context, target, "name");
        // An array rather than an object: the environment is read back with
        // the ambient `get_indexed`, which is the only element read available
        // outside a borrow, and an index needs no interned key.
        Some(entry::make_array_in(context, vec![target, histogram, name]))
    });
    let Some(environment) = prepared else {
        return entry::undefined_value();
    };
    // Ambient, and therefore outside the borrow above. `wrapped` has exactly
    // the shape `entry/functions.rs` transmutes a code address to, which is
    // the same shape `native::callable` already relies on.
    entry::closure_new(wrapped as *const () as usize as i64, environment)
}

/// The wrapper a program actually calls.
///
/// Nothing here holds a borrow across [`entry::call`] — the target is read
/// ambiently before it, and the entry is appended after it.
extern "C" fn wrapped(environment: u64, this: u64, a0: u64, a1: u64, a2: u64, a3: u64) -> u64 {
    let target = entry::get_indexed(environment, entry::make_number(TARGET));
    let histogram = entry::get_indexed(environment, entry::make_number(HISTOGRAM));

    let start_ms = super::now_ms();
    let start_ns = super::now_ns();
    let produced = entry::call(target, this, a0, a1, a2, a3);
    let elapsed_ns = super::now_ns() - start_ns;

    // Nanoseconds into the histogram, milliseconds onto the entry. The unit
    // mismatch is Node's own (reference doc §4) and is reproduced rather than
    // quietly harmonised, because a program comparing a histogram reading
    // against Node's would otherwise be off by a million.
    super::histogram::record_value(histogram, elapsed_ns);

    let arguments = entry::make_array(vec![a0, a1, a2, a3]);
    // Read ambiently FIRST, converted under a borrow SECOND. `get_indexed` is
    // ambient; calling it from inside `with_runtime` is the nested borrow that
    // aborts the process, and writing it as two statements is what makes that
    // impossible rather than merely avoided.
    let held = entry::get_indexed(environment, entry::make_number(2.0));
    let name = entry::with_runtime(|context| entry::string_in(context, held).unwrap_or_default());
    timeline::append(Record {
        name,
        kind: "function",
        start: start_ms,
        duration: elapsed_ns / 1_000_000.0,
        // Node's `'function'` entry carries the call's arguments as `detail`.
        // All four convention slots are carried, including the padding an
        // omitted argument leaves — `entry/functions.rs` pads every call to
        // four, so a shorter list cannot be recovered here and a guess would
        // be worse than the padding.
        detail: Some(arguments),
    });
    produced
}
