//! Call-stack depth limit for user functions.
//!
//! Tracks the current recursion depth per thread. When depth exceeds the
//! configured limit, sets the runtime error slot so try/catch can intercept
//! it, and returns `false` so the call site can early-return a sentinel.
//!
//! Limit is read once from `RTS_STACK_LIMIT` (default 10 000).

use std::cell::Cell;
use std::sync::OnceLock;

use super::handles::{Entry, alloc_entry};

thread_local! {
    static DEPTH: Cell<u32> = const { Cell::new(0) };
}

static LIMIT: OnceLock<u32> = OnceLock::new();

fn stack_limit() -> u32 {
    *LIMIT.get_or_init(|| {
        std::env::var("RTS_STACK_LIMIT")
            .ok()
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(10_000)
    })
}

fn set_stack_overflow_error() {
    let msg = b"RangeError: Maximum call stack size exceeded".to_vec();
    let handle = alloc_entry(Entry::String(msg));
    super::error::__RTS_FN_RT_ERROR_SET(handle);
}

/// Called at the entry of every non-tail user function.
/// Returns `1` (ok) or `0` (overflow — error slot is set).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_RT_STACK_PUSH() -> i32 {
    let d = DEPTH.with(|c| {
        let v = c.get();
        c.set(v + 1);
        v + 1
    });
    if d > stack_limit() {
        DEPTH.with(|c| c.set(c.get().saturating_sub(1)));
        set_stack_overflow_error();
        0
    } else {
        1
    }
}

/// Called at every return point of every non-tail user function.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_RT_STACK_POP() {
    DEPTH.with(|c| c.set(c.get().saturating_sub(1)));
}

/// Current call depth — exposed for `runtime.stack_depth()` if needed.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_RT_STACK_DEPTH() -> i64 {
    DEPTH.with(|c| c.get() as i64)
}

/// Resets depth to zero — used between JIT test runs to avoid leakage.
pub fn reset_stack_depth() {
    DEPTH.with(|c| c.set(0));
}
