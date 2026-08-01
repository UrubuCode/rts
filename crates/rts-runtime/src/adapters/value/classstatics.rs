//! UNDECLARED static properties of a class (`C.p` where `p` is not a static
//! member written in the class body).
//!
//! A class body's `static f = …` is a compile-time member: the front gives it a
//! writable module-global cell and reads/writes it with no runtime lookup. But
//! JS lets any property be attached to the constructor object AFTER the fact —
//! `class C {}; C.$1 = new Map()` — and minified bundles do exactly that (a
//! minifier hoists shared state onto the constructor to save bytes). Those
//! properties have no compile-time slot, so they live here: ONE keyed object per
//! class NAME, reached by the ordinary dynamic property get/set.
//!
//! STATIC INHERITANCE. In JS `class D extends C {}` makes `D`'s `[[Prototype]]`
//! the constructor `C`, so `D.p` finds `C.p`. The front records the compile-time
//! parent once per class ([`__rtsadp_class_static_parent`]) and the lookup walks
//! that map — no proto object games, no dependence on whether the class was ever
//! instantiated. A WRITE always lands on the named class itself (JS: assignment
//! creates an OWN property, shadowing the parent), which is why `set` never
//! walks.
//!
//! GC NOTE: same posture as [`super::protos`] — the per-class object word is a
//! strong reference held for the program's lifetime and its handle is pinned, so
//! a sweep cannot collect it.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use super::PolyValue;

/// class NAME → the object word holding its undeclared static properties.
fn statics_table() -> &'static Mutex<HashMap<String, u64>> {
    static T: OnceLock<Mutex<HashMap<String, u64>>> = OnceLock::new();
    T.get_or_init(|| Mutex::new(HashMap::new()))
}

/// class NAME → its compile-time parent class NAME (`class D extends C`).
fn parent_table() -> &'static Mutex<HashMap<String, String>> {
    static T: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    T.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Drop both tables at a program boundary (called from [`super::protos::reset_state`]).
/// The statics table's VALUES are handle words from the program's pool, which the
/// reset drains — a stale entry would point a recycled slot at the wrong value.
pub(crate) fn reset_state() {
    if let Ok(mut t) = statics_table().lock() {
        t.clear();
    }
    if let Ok(mut t) = parent_table().lock() {
        t.clear();
    }
}

/// The per-class statics object, created lazily and PINned (it is cached for the
/// whole program, like an interned constant — a sweep must never take it).
fn statics_object(name: &str) -> u64 {
    let mut t = statics_table().lock().unwrap_or_else(|e| e.into_inner());
    if let Some(w) = t.get(name) {
        return *w;
    }
    use rts_runtime::namespaces::collections::vec as rt_vec;
    use rts_runtime::namespaces::gc::handles as rt_handles;
    let h = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW();
    let shape = rts_engine::heap::shapes::intern_global_shape(&[]);
    rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(h, PolyValue::from_i32(shape as i32).raw() as i64);
    rt_handles::__RTS_FN_NS_GC_PIN_HANDLE(h);
    let w = PolyValue::from_object_handle(rt_handles::__rtsn_poly_from_handle(h)).raw();
    t.insert(name.to_string(), w);
    w
}

/// `__rtsadp_class_static_parent(child, parent)` — record `class child extends
/// parent` for the static-property walk. Emitted once per class with a parent in
/// the startup prologue; idempotent.
#[rtse::abi]
pub fn rtsadp_class_static_parent(child_word: u64, parent_word: u64) -> u64 {
    let child = super::abi_adapter::resolve_poly(PolyValue::from_raw(child_word));
    let parent = super::abi_adapter::resolve_poly(PolyValue::from_raw(parent_word));
    if !child.is_empty() && !parent.is_empty() {
        if let Ok(mut t) = parent_table().lock() {
            t.insert(child, parent);
        }
    }
    PolyValue::undefined().raw()
}

/// class NEW-THUNK code address → the class NAME it reifies. Keyed by ADDRESS for
/// the same reason [`super::ctorval`] is: a code address is stable for the whole
/// run and never reused, so membership is EXACT — no slot/generation hazard.
fn value_name_table() -> &'static Mutex<HashMap<u64, String>> {
    static T: OnceLock<Mutex<HashMap<u64, String>>> = OnceLock::new();
    T.get_or_init(|| Mutex::new(HashMap::new()))
}

/// `__rtsadp_class_value_name(addr, name)` — record that the class-value whose
/// thunk is `addr` is the class `name`. Emitted next to
/// `__rtsadp_register_ctor_thunk` at reify time; idempotent.
#[rtse::abi]
pub fn rtsadp_class_value_name(addr: u64, name_word: u64) -> u64 {
    let name = super::abi_adapter::resolve_poly(PolyValue::from_raw(name_word));
    if addr != 0 && !name.is_empty() {
        if let Ok(mut t) = value_name_table().lock() {
            t.insert(addr, name);
        }
    }
    PolyValue::undefined().raw()
}

/// The class a FUNCTION word reifies, if it is a class-value. Lets the dynamic
/// property paths treat `const K = C; K.p` exactly like `C.p`.
pub(crate) fn class_of_fn_word(word: u64) -> Option<String> {
    let v = PolyValue::from_raw(word);
    if !v.is_function() {
        return None;
    }
    let addr = super::funcops::__rtsadp_fn_ptr(word);
    if addr == 0 {
        return None;
    }
    value_name_table().lock().ok()?.get(&addr).cloned()
}

/// The static property `key` carried by `class` or an ancestor, if any. Used by
/// the dynamic property read on a class-VALUE receiver, which must fall through
/// to its ordinary behaviour when the class carries no such property.
pub(crate) fn lookup(class: &str, key_word: u64) -> Option<u64> {
    let mut name = class.to_string();
    let limit = parent_table().lock().map(|t| t.len()).unwrap_or(0) + 1;
    for _ in 0..limit {
        let obj = statics_object(&name);
        if PolyValue::from_raw(super::objops::__rtsadp_has_own(obj, key_word)).is_truthy() {
            return Some(super::objops::__rtsadp_obj_get(obj, key_word));
        }
        let up = parent_table().lock().ok()?.get(&name).cloned()?;
        name = up;
    }
    None
}

/// Write `key = value` as an own static property of `class` (the class-VALUE
/// receiver form of [`__rtsadp_class_static_set`]).
pub(crate) fn store(class: &str, key_word: u64, val_word: u64) {
    let obj = statics_object(class);
    super::objops::__rtsadp_obj_set(obj, key_word, val_word);
}

/// `__rtsadp_class_static_get(class, key, fallback)` — read a static property
/// that has no compile-time cell OWNED by `class`, walking the parent chain.
///
/// `fallback` is what the answer is when no class in the chain carries the
/// property here. Two callers, two fallbacks: an entirely undeclared property
/// passes `undefined` (the JS answer for an absent property); a read of a static
/// field DECLARED BY AN ANCESTOR passes that ancestor's cell value, so a
/// subclass shadow write (`D.f = v`, which lands here rather than mutating the
/// parent's cell) wins over the inherited declaration exactly as JS orders them.
#[rtse::abi]
pub fn rtsadp_class_static_get(name_word: u64, key_word: u64, fallback: u64) -> u64 {
    let name = super::abi_adapter::resolve_poly(PolyValue::from_raw(name_word));
    lookup(&name, key_word).unwrap_or(fallback)
}

/// `__rtsadp_class_static_set(class, key, value)` — write an undeclared static
/// property. Always an OWN property of `class` (JS assignment semantics), never
/// the inherited one. Returns the assigned value (`C.p = v` yields `v`).
#[rtse::abi]
pub fn rtsadp_class_static_set(name_word: u64, key_word: u64, val_word: u64) -> u64 {
    let name = super::abi_adapter::resolve_poly(PolyValue::from_raw(name_word));
    let obj = statics_object(&name);
    super::objops::__rtsadp_obj_set(obj, key_word, val_word);
    val_word
}
