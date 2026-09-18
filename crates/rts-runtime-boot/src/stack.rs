//! The top of the CURRENT thread's stack, on a platform with a verified method.
//!
//! Duplicated from `rts-host::stack` rather than shared: that crate is not
//! a dependency of this one (naming it would be backwards — this facade is
//! what an AOT binary links against, and `rts-host` is a JIT host that itself
//! depends on nothing this crate produces). Both destinations must seed the
//! same collector contract, but the small platform adapters remain local to
//! the binaries that own their startup paths.
//!
//! A module of its own because `lib.rs` is past the workspace's 500-line
//! ceiling and a file past it does not grow; the macOS arm is what would have
//! grown it. `rts-host::stack`'s documentation carries the per-platform
//! reasoning, including why a `None` here is not a smaller collection but NO
//! collection — which is what an AOT binary on macOS did until this arm.

#[cfg(all(target_arch = "x86_64", target_os = "windows"))]
pub(crate) fn current_thread_stack_high() -> Option<usize> {
    unsafe extern "system" {
        fn GetCurrentThreadStackLimits(low: *mut usize, high: *mut usize);
    }
    let mut low: usize = 0;
    let mut high: usize = 0;
    // SAFETY: an ordinary Win32 call, out parameters only.
    unsafe { GetCurrentThreadStackLimits(&mut low, &mut high) };
    Some(high)
}

/// The top of the current Linux thread's stack, from its pthread attributes.
#[cfg(target_os = "linux")]
pub(crate) fn current_thread_stack_high() -> Option<usize> {
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

/// The top of the current macOS thread's stack. `pthread_get_stackaddr_np`
/// answers the HIGH end on darwin — see `rts-host::stack` for why no size is
/// added, where Linux's call needs one.
#[cfg(target_os = "macos")]
pub(crate) fn current_thread_stack_high() -> Option<usize> {
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
pub(crate) fn current_thread_stack_high() -> Option<usize> {
    None
}
