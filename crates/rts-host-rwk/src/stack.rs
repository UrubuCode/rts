//! The one OS call `rts-core-rwk` is not allowed to make for itself.
//!
//! `entry::roots::scan_stack`'s own documentation draws the line: reading the
//! current stack pointer is a machine fact, but the TOP of a thread's stack is
//! an operating-system fact — on Windows, `GetCurrentThreadStackLimits` — and
//! `rts-core-rwk` must build for wasm, where that call does not exist. So this
//! crate makes it, once per thread, and hands the answer down through
//! [`rts_core_rwk::entry::Context::stack_high`] — the same shape
//! `declare_evaluator` and `declare_rest` already use for a capability that can
//! only arrive from above.
//!
//! # Why `GetCurrentThreadStackLimits` and not the TIB
//!
//! `crates/rts-natives/src/collector/scan.rs` is the precedent this follows,
//! and its own comment says why: the TIB's `StackBase`
//! (`gs:[0x10]`/`NtCurrentTeb()->StackBase`) can read BELOW the current stack
//! pointer in some circumstances, which would hand the collector a `high` that
//! excludes live frames. `GetCurrentThreadStackLimits` is the API Microsoft
//! documents as staying consistent with the actual stack, and it is what that
//! module uses.
//!
//! # Confidence, per platform
//!
//! **Windows x86-64 — the platform this was written and tested on: sound.**
//! `GetCurrentThreadStackLimits` is the documented, correct call, and this is a
//! direct, minimal use of it: no TIB, no derived arithmetic.
//!
//! **Everything else — deliberately absent, not guessed.** A Linux port has a
//! precedent too (`/proc/self/maps`, in the same old-engine module), but
//! nobody has exercised it here and this crate's task was to make Windows
//! sound, not to port an untested read to a second platform and call it done.
//! [`current_thread_stack_high`] answers `None` off Windows, and
//! [`rts_core_rwk::entry::collect_cycle`]'s own contract is to skip the stack
//! half of a collection entirely rather than run one with a bound nobody
//! checked — see `Context::stack_high`'s own documentation for why a missing
//! bound must never be treated as a small one.
use rts_core_rwk::entry::Context;

/// The top of the CURRENT thread's stack, if this platform can answer honestly.
///
/// Installed on [`Context`] once, right after it is built and before it is
/// handed to compiled code — a `Context` never crosses a thread, so one call
/// per `Context` is one call per thread, which is what the API answers about.
#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
pub fn current_thread_stack_high() -> Option<usize> {
    unsafe extern "system" {
        fn GetCurrentThreadStackLimits(low: *mut usize, high: *mut usize);
    }
    let mut low: usize = 0;
    let mut high: usize = 0;
    // SAFETY: an ordinary Win32 call, out parameters only, no preconditions
    // beyond running on the thread being asked about — which this always
    // does, since there is no thread handle to name another one.
    unsafe { GetCurrentThreadStackLimits(&mut low, &mut high) };
    Some(high)
}

/// The honest answer off Windows: nobody has wired a sound call yet.
#[cfg(not(all(target_arch = "x86_64", target_os = "windows")))]
pub fn current_thread_stack_high() -> Option<usize> {
    None
}

/// Installs this thread's stack top on a freshly built [`Context`].
///
/// A free function rather than inlined at each call site because there are
/// two — [`crate::run::run_region`] and the per-thread closure
/// [`rts_core_rwk::entry::Compiled::run_on`] spawns — and a bound installed at
/// one and not the other is a collection that is sound on the main thread and
/// silently a no-op everywhere else, which is exactly the kind of asymmetry
/// this crate's rule 2 exists to make impossible to miss.
pub fn install(context: &mut Context) {
    context.stack_high = current_thread_stack_high();
}
