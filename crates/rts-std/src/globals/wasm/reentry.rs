//! Where the `&mut Store` of an instance that is RUNNING can be found.
//!
//! # Reuse-check: this is a second form of a question `store.rs` already answers
//!
//! The question is *"where is the store of instance N right now"*, and until this
//! module there was one answer: the `INSTANCES` table. That answer is incomplete
//! rather than wrong — while a call is in flight the row is `None`, because
//! `store::call` TAKES the instance out so as not to hold the table's lock across
//! a call that may re-enter.
//!
//! `wasmi` has no second way in. `Func::call` wants `impl AsContextMut`, and the
//! only implementor reachable from inside a host function is the `Caller` the
//! callback was handed (`Caller: AsContextMut`, `wasmi-0.31.2/src/func/caller.rs`
//! line 98). A newer `wasmi` changes nothing here and that was checked rather
//! than assumed: every call entry point in every version takes a store context,
//! because the memory and the globals ARE the store. So "reach the active
//! `Caller`" is not one option among several — it is the only one that does not
//! mean a different engine.
//!
//! So this module is the *other half* of the one question, and the two halves are
//! joined in `store::with_context` and `store::call` so that no caller has to
//! know which half answered. A caller that asked only the table got a silent
//! wrong answer — `memory_bytes` answered an EMPTY slice for a running instance,
//! and `memory.rs::rebuild_with` carries the comment that had to work around it.
//!
//! # Why this exists at all: `__wbindgen_malloc`
//!
//! A `wasm-bindgen` import that returns data to the guest allocates it by calling
//! `__wbindgen_malloc` — an EXPORT of the instance whose body is suspended on
//! that very import. Refusing it refuses the generator's normal output, not a
//! corner of it: the `whatsapp-rust-bridge` that `@whiskeysockets/baileys` uses
//! for `hkdf` raised on its first call. The previous note here argued the refusal
//! was affordable because `wasm-bindgen`'s finalizers run from a
//! `FinalizationRegistry` and therefore after the call. That is true, and it was
//! the wrong thing to have checked.
//!
//! # The invariant, and what breaks it
//!
//! A frame's pointer is reachable only while that frame is suspended in
//! [`with_active`]'s `body`, and an entry is *taken out* of the stack while it is
//! borrowed. Together those give the rule the `unsafe` rests on:
//!
//! > **For each pointer on this stack there is at most one live `&mut Caller`
//! > derived from it, and the `&mut Caller` the pointer was made from is not
//! > touched while the stack holds it.**
//!
//! Three things would break it, and each is local to one place:
//!
//! - **Touching the outer `&mut Caller` inside `body`.** `imports.rs` mirrors the
//!   memory through `caller` immediately before and immediately after the call to
//!   [`with_active`], never inside it. That is the one site, and it is why the
//!   mirroring is not folded into the closure.
//! - **Handing the pointer out twice.** [`use_active`] replaces the entry with
//!   null for the duration, so a second request for the same frame finds nothing
//!   and falls to a frame further down — the same take/give-back shape
//!   `store.rs` uses on `Live`, for the same reason. It is enforced by the data,
//!   not by this comment.
//! - **Leaving a stale pointer behind.** The push and the pop are one function
//!   and strictly LIFO, and a debug assertion refuses a pop that is not the push.
//!   An `extern "C"` frame here cannot unwind, so a panic ends the process rather
//!   than skipping the pop — the same cost `store::call` already states.
//!
//! What is NOT a risk is another thread: the stack is thread-local, and nothing
//! in this workspace publishes a reference from one thread to another, so an
//! instance running on this thread cannot be reached from a second one.

use super::store::HostState;
use std::cell::RefCell;
use wasmi::Caller;

/// A pointer to a `Caller` whose host function is suspended on this thread's
/// stack, with its lifetime erased.
///
/// Erased because the lifetime is the one thing that cannot be carried: it names
/// a borrow inside the `wasmi` frame that called us, and no signature reachable
/// from the JavaScript side can mention it. The pointer is what the invariant
/// above is stated about.
type Frame = *mut Caller<'static, HostState>;

thread_local! {
    /// The suspended host-function frames of this thread, innermost last, each
    /// with the instance row its `Caller` belongs to.
    ///
    /// A `Vec` and not one slot: an import of instance A may call an export of B,
    /// whose body calls an import of B, which calls back into A. Both frames are
    /// then suspended and either may be the one a call needs.
    static ACTIVE: RefCell<Vec<(usize, Frame)>> = const { RefCell::new(Vec::new()) };
}

/// Runs `body` with `caller` reachable by [`use_active`] under `row`.
///
/// `caller` is borrowed for the whole of `body` and must not be touched through
/// that borrow inside it — see the invariant in this module's header. The borrow
/// is taken rather than a raw pointer being accepted so that a caller cannot
/// register a `Caller` it does not exclusively hold.
pub(super) fn with_active<T>(
    caller: &mut Caller<'_, HostState>,
    row: usize,
    body: impl FnOnce() -> T,
) -> T {
    // SAFETY of the cast itself: only the lifetime changes, and the pointer is
    // never dereferenced outside the dynamic extent of `body`, which is inside
    // the borrow above. `Caller` is not `Copy` and nothing here reads it.
    let erased = caller as *mut Caller<'_, HostState> as Frame;
    ACTIVE.with(|stack| stack.borrow_mut().push((row, erased)));
    let answer = body();
    let popped = ACTIVE.with(|stack| stack.borrow_mut().pop());
    debug_assert!(
        popped == Some((row, erased)),
        "the frame popped must be the frame pushed, or a pointer outlived its frame"
    );
    answer
}

/// Runs `body` over the innermost available `Caller` of `row`, if one is
/// suspended on this thread.
///
/// `None` means no frame of that instance is running here, which is what makes it
/// safe for the caller to decide between this and the instance table without
/// asking which case it is in.
pub(super) fn use_active<T>(
    row: usize,
    body: impl FnOnce(&mut Caller<'static, HostState>) -> T,
) -> Option<T> {
    let (at, taken) = ACTIVE.with(|stack| {
        let mut stack = stack.borrow_mut();
        let at = stack.iter().rposition(|(found, ptr)| *found == row && !ptr.is_null())?;
        // TAKEN, not copied: the null left behind is what refuses a second
        // `&mut` to this frame, and it is why the search above skips nulls.
        Some((at, std::mem::replace(&mut stack[at].1, std::ptr::null_mut())))
    })?;
    // SAFETY: `taken` was registered by `with_active`, whose frame is suspended
    // in its `body` — so the `Caller` is alive, is not being touched through the
    // borrow `with_active` holds, and has been removed from the stack, so this is
    // the only `&mut` derived from it. The module header states the three things
    // that would break that and where each one lives.
    let answer = body(unsafe { &mut *taken });
    // `at` is still this entry: `body` can only PUSH (a deeper host function,
    // appended after `at`) and its pops are LIFO above `at`, so nothing before it
    // moves or disappears while this frame is suspended.
    ACTIVE.with(|stack| {
        if let Some(entry) = stack.borrow_mut().get_mut(at) {
            entry.1 = taken;
        }
    });
    Some(answer)
}
