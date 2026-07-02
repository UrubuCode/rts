//! Codegen-owned thread-local pending-error slot for `throw` / `try-catch`
//! (P5.13). The new engine's manual-unwind exception model, mirroring the old
//! engine's phase-1 mechanism (`rts-codegen-old` `__RTS_FN_RT_ERROR_*`) but
//! native to the [`PolyValue`](super::PolyValue) value model.
//!
//! ## Why a codegen-owned slot, not the real `__RTS_FN_RT_ERROR_*`
//!
//! The real runtime slot (`rts-std` `collector::error`) stores a `u64` GC
//! **handle** (it captures + frees a stack-trace handle on every set). The new
//! engine throws an arbitrary [`PolyValue`] **word** — a string/number/bool/object
//! — which is NOT a bare handle (a thrown `42` is an inline tagged-int word, a
//! thrown `"oops"` is a string word, a thrown `new Error(..)` is a `TAG_OBJECT`
//! word). Storing the raw word directly is the honest, lossless representation;
//! decoding it through the real handle slot would corrupt non-object throws.
//!
//! So this is a tiny codegen-owned thread-local holding the raw thrown word + a
//! set flag. No stack capture (the new engine has no frame stack yet); that is a
//! later increment, not needed for sound `try/catch/finally`.
//!
//! ## The manual-unwind contract (soundness)
//!
//! - `throw e`: lower `e` to a word, `__rtsadp_throw_set(word)`, then RETURN a
//!   sentinel from the current function — the throwing block is terminated, so no
//!   code after the `throw` in that function runs.
//! - After EVERY user-function call / function-value invoke, the lowering emits
//!   `if __rtsadp_err_pending()` → branch to the innermost active `catch` (when
//!   inside a `try`) or RETURN a sentinel (propagate to the caller, whose own
//!   post-call check handles it).
//! - `try { B } catch (e) { H } finally { F }`: `B` runs; a pending error after
//!   `B` (set by a lexical `throw` or a propagated call) routes to `H`, which
//!   `__rtsadp_err_take()`s the word (clearing the slot) and binds it to `e`; `F`
//!   always runs on both paths. A `throw` inside `H`/`F` re-sets the slot.

use std::cell::Cell;

thread_local! {
    /// The pending thrown value (a raw [`PolyValue`](super::PolyValue) word) and
    /// whether one is set. `set = false` ⇒ no pending error (the word is stale and
    /// must not be read).
    static PENDING: Cell<(u64, bool)> = const { Cell::new((0, false)) };
}

/// `throw e` — record the thrown PolyValue `word` as the pending error. The
/// lowering follows this with a sentinel `return` (manual unwind).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_throw_set(word: u64) {
    PENDING.with(|p| p.set((word, true)));
}

/// `1` iff a thrown value is pending (an unwind is in progress), else `0`. Emitted
/// after every call that can throw; the lowering branches on the result.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_err_pending() -> i64 {
    PENDING.with(|p| p.get().1 as i64)
}

/// Read AND clear the pending thrown value (the `catch (e)` bind). Returns the raw
/// PolyValue word; clears the flag so the handler runs with no pending error.
/// Returns the `undefined` word if (defensively) nothing was pending.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_err_take() -> u64 {
    PENDING.with(|p| {
        let (word, set) = p.get();
        p.set((0, false));
        if set {
            word
        } else {
            super::PolyValue::undefined().raw()
        }
    })
}

/// Clear the pending error without reading it (`catch {}` with no binding, and the
/// belt-and-braces clear a `try` emits before its body so a stale outer error
/// cannot leak into an inner try's fall-through check).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_err_clear() {
    PENDING.with(|p| p.set((0, false)));
}

/// Clear the pending-error slot on THIS thread. Called by
/// [`crate::state::reset_codegen_state`] at the start of each program compile.
/// This is a SINGLE-CELL hygiene reset, NOT a leak fix (the slot never grows) —
/// it prevents a stale pending error from one run leaking into the next on a
/// reused thread.
pub fn reset_state() {
    PENDING.with(|p| p.set((0, false)));
}

/// Register THIS slot's readers with the runtime's `promise.create` watcher, so
/// a `throw` inside a spawned async-fn body (which lands HERE, not in the
/// legacy handle slot) rejects the Promise instead of resolving with the
/// sentinel. Idempotent (`OnceLock` on the runtime side). Called by the engine
/// bootstrap: JIT via `compile_program`, AOT via [`__rtsadp_engine_bootstrap`]
/// in the generated `main` shim.
pub fn install_async_error_hook() {
    rts_runtime::namespaces::promise::set_async_error_hook(
        __rtsadp_err_pending,
        __rtsadp_err_take,
    );
}

/// AOT bootstrap entry: the generated `main` shim calls this right after
/// `__RTS_FN_RT_INIT` (the shim cannot reach Rust-side adapters fns except by
/// symbol). Today it only installs the async-error hook.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_engine_bootstrap() {
    install_async_error_hook();
}
