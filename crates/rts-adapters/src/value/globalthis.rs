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

use std::sync::OnceLock;

use rts_engine::collector::global_roots;
use rts_runtime::namespaces::collections::vec as rt_vec;
use rts_runtime::namespaces::gc::handles as rt_handles;

use super::PolyValue;

/// The singleton `globalThis` object word (`TAG_OBJECT`), set once on first access.
static GLOBALTHIS: OnceLock<u64> = OnceLock::new();

/// Return the singleton `globalThis` object word, creating it on first call. The
/// instance is an EMPTY keyed object (slot 0 = the empty-keys global shape-id,
/// exactly like an object literal `{}`), and is registered as a permanent GC root
/// so it — and every heap value it holds — survives collection.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_globalthis() -> u64 {
    let word = *GLOBALTHIS.get_or_init(|| {
        let handle = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW();
        // slot 0 = the empty-shape id, the same header a `{}` literal carries.
        let empty_shape = crate::shape::intern_global_shape(&[]);
        let slot0 = PolyValue::from_i32(empty_shape as i32).raw() as i64;
        rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(handle, slot0);
        let poly48 = rt_handles::__RTS_FN_NS_GC_POLY_FROM_HANDLE(handle);
        PolyValue::from_object_handle(poly48).raw()
    });
    // Pin as a permanent root (idempotent — `add` dedups by address). Done AFTER
    // init so the address read is the stable `OnceLock` slot holding the word.
    global_roots::add(GLOBALTHIS.get().expect("just initialised") as *const u64 as usize);
    word
}
