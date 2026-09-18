//! The one OS call `rts-core` is not allowed to make for itself.
//!
//! `entry::roots::scan_stack`'s own documentation draws the line: reading the
//! current stack pointer is a machine fact, but the TOP of a thread's stack is
//! an operating-system fact — on Windows, `GetCurrentThreadStackLimits` — and
//! `rts-core` must build for wasm, where that call does not exist. So this
//! crate makes it, once per thread, and hands the answer down through
//! [`rts_core::entry::Context::stack_high`] — the same shape
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
//! **Linux — `pthread_getattr_np` is the selected implementation.** It returns
//! the bounds of the CURRENT thread, including worker threads created by
//! [`Compiled::run_on`]. The call happens once per context, not inside an
//! allocation or collection, so the platform query does not become part of the
//! hot path. A `/proc/self/maps` lookup would identify the process mapping and
//! can miss a worker thread's stack, so it is deliberately not used here.
//!
//! **macOS — `pthread_get_stackaddr_np`, and it answers the TOP.** Darwin's
//! "stack address" is the HIGH end, the one the stack grows down from; Rust's
//! own `std` reads it that way when it places the guard page
//! (`stackaddr − pthread_get_stacksize_np`). So nothing is added to it, where
//! Linux's `pthread_attr_getstack` answers the LOW end and needs the size. Same
//! per-thread property as Linux: it names the calling thread, so a worker of
//! [`Compiled::run_on`] gets its own stack and not the main thread's.
//!
//! **Why macOS mattered more than "one more platform".** Until this arm it
//! answered `None`, and the contract below turns `None` into a collection
//! that frees NOTHING — so every program that allocated past the region's
//! reservation died with `heap exhausted … all of them are in use even after a
//! collection`, which reads as retention and was the absence of any cycle at
//! all. `RTS_GC_DEBUG=1` says `rts-gc REFUSED: no stack bound installed` on
//! every trigger; that line, not the exhaustion message, is the diagnosis. The
//! three suite files that exercise the collector failed on the macOS runner
//! only, for exactly this reason.
//!
//! **Everything else — deliberately absent, not guessed.** [`current_thread_stack_high`]
//! answers `None` on platforms whose stack-top mechanism is not implemented, and
//! [`rts_core::entry::collect_cycle`]'s own contract is to skip the stack half of
//! a collection rather than run one with a bound nobody checked — see
//! `Context::stack_high`'s own documentation for why a missing bound must never
//! be treated as a small one.
use rts_core::entry::Context;

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

/// The top of the current Linux thread's stack, from its pthread attributes.
#[cfg(target_os = "linux")]
pub fn current_thread_stack_high() -> Option<usize> {
    let mut attributes = std::mem::MaybeUninit::<libc::pthread_attr_t>::uninit();
    // SAFETY: `pthread_getattr_np` initializes `attributes` when it returns 0;
    // `pthread_self` names this thread, so the returned bounds belong to the
    // stack that will later be scanned by the collector.
    let status = unsafe { libc::pthread_getattr_np(libc::pthread_self(), attributes.as_mut_ptr()) };
    if status != 0 {
        return None;
    }

    let mut attributes = unsafe { attributes.assume_init() };
    let mut base = std::ptr::null_mut();
    let mut size = 0usize;
    // SAFETY: `attributes` was initialized by pthread_getattr_np and both
    // output pointers are valid for the duration of this call.
    let status = unsafe { libc::pthread_attr_getstack(&attributes, &mut base, &mut size) };
    // SAFETY: pthread_attr_destroy accepts an initialized pthread attribute.
    let destroy_status = unsafe { libc::pthread_attr_destroy(&mut attributes) };
    if status != 0 || destroy_status != 0 || base.is_null() {
        return None;
    }
    Some(base as usize + size)
}

/// The top of the current macOS thread's stack — the HIGH end, which is what
/// `pthread_get_stackaddr_np` answers on darwin; see the module doc.
#[cfg(target_os = "macos")]
pub fn current_thread_stack_high() -> Option<usize> {
    // SAFETY: `pthread_self` names this thread; the call only reads its
    // descriptor.
    let top = unsafe { libc::pthread_get_stackaddr_np(libc::pthread_self()) } as usize;
    (top != 0).then_some(top)
}

/// The honest answer on platforms without a verified stack-top mechanism.
#[cfg(not(any(
    target_os = "linux",
    target_os = "macos",
    all(target_arch = "x86_64", target_os = "windows")
)))]
pub fn current_thread_stack_high() -> Option<usize> {
    None
}

/// Installs this thread's stack top on a freshly built [`Context`].
///
/// A free function rather than inlined at each call site because there are
/// two — [`crate::run::run_region`] and the per-thread closure
/// [`rts_core::entry::Compiled::run_on`] spawns — and a bound installed at
/// one and not the other is a collection that is sound on the main thread and
/// silently a no-op everywhere else, which is exactly the kind of asymmetry
/// this crate's rule 2 exists to make impossible to miss.
pub fn install(context: &mut Context) {
    context.stack_high = current_thread_stack_high();
}

#[cfg(all(
    test,
    any(target_os = "linux", target_os = "macos", all(target_arch = "x86_64", target_os = "windows"))
))]
mod tests {
    use super::current_thread_stack_high;

    #[test]
    fn current_thread_stack_high_is_above_a_live_frame() {
        let anchor = 0u8;
        let frame = &anchor as *const u8 as usize;
        let high = current_thread_stack_high().expect("the current platform has a stack bound");
        assert!(high > frame, "stack high {high:#x} is not above frame {frame:#x}");
    }
}
