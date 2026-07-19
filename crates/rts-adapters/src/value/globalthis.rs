//! `globalThis` — the singleton global object (foundation: VALUE properties).
//!
//! `globalThis` is a process-wide dynamic object: an arbitrary string→PolyValue
//! property bag readable/writable from anywhere (`globalThis.x = 5; globalThis.x`).
//! It is backed by the SAME keyed-object representation as an object literal (a
//! `VEC` whose slot 0 is a global shape-id, values at `1 + index`), so
//! `globalThis.prop` get/set route through the ordinary dynamic-object trampolines
//! ([`super::objops`]) — no new property machinery.
//!
//! The single instance is created lazily on first reference and pinned as a
//! permanent GC root: it lives only in a `OnceLock`, off every stack, so without
//! the root the conservative sweep would collect it (and any heap value it solely
//! holds). The root marks the boxed object word; `Entry::Vec`'s child tracing then
//! keeps the stored property values alive.
//!
//! SCOPE — VALUE properties only. `globalThis.X = class X {…}` (class-as-value) and
//! `new (globalThis.X)()` (dynamic construct) are a deferred follow-up: they need
//! HIR class-expressions, which the front-end does not model yet.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use rts_engine::collector::global_roots;
use rts_runtime::namespaces::collections::vec as rt_vec;
use rts_runtime::namespaces::gc::handles as rt_handles;

use super::PolyValue;

/// The singleton `globalThis` object word (`TAG_OBJECT`), created lazily on first
/// access and cleared by [`reset`] at a program boundary.
///
/// Held in an `AtomicU64` (not a `OnceLock`) for two reasons: its ADDRESS is what
/// the conservative GC registers as a root and reads back as a bare `u64` (an
/// `AtomicU64` has the layout of a `u64`, a `OnceLock<u64>` does not), and the
/// word must be CLEARABLE. The word wraps a HandleTable slot from the *current*
/// program's string pool, and `reset_codegen_state` drains that pool between
/// programs (unit tests run hundreds in one process). A `OnceLock` kept the FIRST
/// program's stale word forever; the GC then read it as a root and touched a
/// recycled slot — the intermittent crash the unit-test binary hit under the
/// sound (post-pin-leak) GC. `0` means "not yet created for this program".
static GLOBALTHIS: AtomicU64 = AtomicU64::new(0);
/// Serializes the create-and-register in [`__rtsadp_globalthis`] so two threads
/// racing the first access build exactly one instance.
static INIT_LOCK: Mutex<()> = Mutex::new(());

/// Drop the cached `globalThis` and unregister its GC root. Called from
/// `reset_codegen_state` at the quiescent program boundary — the previous
/// program is finished (its pool is about to be drained) and the next has not
/// run, so no live code holds the old word.
pub fn reset() {
    let _guard = INIT_LOCK.lock().expect("globalthis init lock");
    global_roots::remove(&GLOBALTHIS as *const AtomicU64 as usize);
    GLOBALTHIS.store(0, Ordering::SeqCst);
}

/// Return the singleton `globalThis` object word, creating it on first call. The
/// instance is an EMPTY keyed object (slot 0 = the empty-keys global shape-id,
/// exactly like an object literal `{}`), and is registered as a permanent GC root
/// so it — and every heap value it holds — survives collection.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_globalthis() -> u64 {
    // Fast path: already built for this program.
    let existing = GLOBALTHIS.load(Ordering::Acquire);
    if existing != 0 {
        return existing;
    }
    // Build exactly once per program under the init lock (racing the reset and
    // any other first-access thread).
    let _guard = INIT_LOCK.lock().expect("globalthis init lock");
    let already = GLOBALTHIS.load(Ordering::Acquire);
    if already != 0 {
        return already;
    }
    let handle = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW();
    // slot 0 = the empty-shape id, the same header a `{}` literal carries.
    let empty_shape = crate::shape::intern_global_shape(&[]);
    let slot0 = PolyValue::from_i32(empty_shape as i32).raw() as i64;
    rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(handle, slot0);
    let poly48 = rt_handles::__RTS_FN_NS_GC_POLY_FROM_HANDLE(handle);
    let word = PolyValue::from_object_handle(poly48).raw();
    GLOBALTHIS.store(word, Ordering::Release);
    // Register the ATOMIC's own address as a GC root: the scanner reads it back as
    // a bare `u64` word (an AtomicU64 is layout-identical to u64) and marks the
    // boxed object it names. `add` dedups by address, so re-registering after a
    // reset is fine.
    global_roots::add(&GLOBALTHIS as *const AtomicU64 as usize);
    word
}
