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
#[rtse::abi]
pub fn rtsadp_globalthis() -> u64 {
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
    let empty_shape = rts_engine::heap::shapes::intern_global_shape(&[]);
    let slot0 = PolyValue::from_i32(empty_shape as i32).raw() as i64;
    rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(handle, slot0);
    let poly48 = rt_handles::__rtsn_poly_from_handle(handle);
    let word = PolyValue::from_object_handle(poly48).raw();
    GLOBALTHIS.store(word, Ordering::Release);
    // Register the ATOMIC's own address as a GC root: the scanner reads it back as
    // a bare `u64` word (an AtomicU64 is layout-identical to u64) and marks the
    // boxed object it names. `add` dedups by address, so re-registering after a
    // reset is fine.
    global_roots::add(&GLOBALTHIS as *const AtomicU64 as usize);
    word
}

/// A contraparte de ESCRITA de [`__rtsadp_global_ref`]: atribuir a um nome que
/// não casa com binding léxico nenhum CRIA uma propriedade no objeto global —
/// o "global implícito" do modo sloppy, que é o modo de todo `<script>` de
/// página. Devolve o valor atribuído, porque `x = v` é uma expressão cujo valor
/// é `v`.
///
/// DIVERGE do modo STRICT (e de um módulo ES), onde isso é `ReferenceError`. O
/// motor não distingue os dois modos hoje; sloppy é o que a superfície que
/// motivou isto — script de página e bundle — realmente usa, e o Node concorda
/// nos dois contextos que dá para testar. Recusar em compilação não é opção:
/// derrubava o arquivo inteiro.
#[rtse::abi]
pub fn rtsadp_global_set(name_word: u64, value: u64) -> u64 {
    let g = __rtsadp_globalthis();
    super::objops::__rtsadp_obj_set(g, name_word, value);
    value
}

/// The LAST LINK of the JS scope chain, for a free identifier the front could
/// not resolve lexically: an unqualified name that matches no binding resolves
/// against the GLOBAL OBJECT. That is what makes one script's
/// `globalThis.__w = fn` callable as a bare `__w()` from another — the shape a
/// page's module loader is built on, and something no amount of lexical
/// analysis inside a single script can see.
///
/// Returns the property value when the global object carries the key; otherwise
/// throws the spec `ReferenceError` (the same outcome the front emits inline
/// for an unresolved READ) and returns `undefined`, so the caller's ordinary
/// post-call error check routes the unwind.
#[rtse::abi]
pub fn rtsadp_global_ref(name_word: u64) -> u64 {
    let g = __rtsadp_globalthis();
    if super::objops::__rtsadp_obj_has(g, name_word) != 0 {
        return super::objops::__rtsadp_obj_get(g, name_word);
    }
    let name = super::abi_adapter::resolve_poly(PolyValue::from_raw(name_word));
    super::errslot::throw_js_error("ReferenceError", &format!("{name} is not defined"));
    PolyValue::undefined().raw()
}
