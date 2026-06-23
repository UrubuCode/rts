//! Object PROTOTYPE chain (`Object.create` + proto-walk property lookup).
//!
//! The engine's objects are FLAT shape-slot Vecs with NO built-in prototype link.
//! `Object.create(proto)` makes a fresh bare object and records its `[[Prototype]]`
//! in a side-table keyed by the object word; a property read that misses the own
//! slots ([`super::objops::__rtsadp_obj_get`]) walks this table. `getPrototypeOf` /
//! `isPrototypeOf` read it too.
//!
//! GC NOTE: the table holds the proto word as a STRONG reference but is NOT scanned
//! as a GC root (a prototype is virtually always a long-lived object, like the
//! `WeakMap` interim). A precise weak/rooted table is a later increment (#217-style).

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use rts_runtime::namespaces::collections::vec as rt_vec;
use rts_runtime::namespaces::gc::handles as rt_handles;

use super::PolyValue;

/// object word → its `[[Prototype]]` object word. Absent ⇒ a null prototype
/// (`Object.create(null)` or a plain literal/instance).
fn proto_table() -> &'static Mutex<HashMap<u64, u64>> {
    static T: OnceLock<Mutex<HashMap<u64, u64>>> = OnceLock::new();
    T.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The recorded prototype of `obj_word`, if any (an object word). Used by the
/// property-read proto-walk in `objops`.
pub(crate) fn proto_of(obj_word: u64) -> Option<u64> {
    proto_table().lock().ok()?.get(&obj_word).copied()
}

/// Allocate a fresh BARE keyed object (the `{}` shape) and record `proto_word` as
/// its prototype when `proto_word` is an object (a null/number/other proto — JS
/// `Object.create(null)` / a non-object arg — records nothing → a null prototype).
/// Returns the new object word.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_obj_create(proto_word: u64) -> u64 {
    let obj_handle = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW();
    let empty_shape = crate::shape::intern_global_shape(&[]);
    let slot0 = PolyValue::from_i32(empty_shape as i32).raw() as i64;
    rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(obj_handle, slot0);
    let obj_word =
        PolyValue::from_object_handle(rt_handles::__RTS_FN_NS_GC_POLY_FROM_HANDLE(obj_handle)).raw();

    if PolyValue::from_raw(proto_word).is_object() {
        if let Ok(mut t) = proto_table().lock() {
            t.insert(obj_word, proto_word);
        }
    }
    obj_word
}

/// `Object.getPrototypeOf(obj)` → the recorded prototype object word, or `null`.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_obj_proto_of(obj_word: u64) -> u64 {
    proto_of(obj_word).unwrap_or_else(|| PolyValue::null().raw())
}

/// `Object.setPrototypeOf(obj, proto)` / `Reflect.setPrototypeOf` — record (or
/// clear) `obj`'s prototype. A non-object `proto` (`null`) REMOVES the link (a null
/// prototype). Returns `obj` (Object.setPrototypeOf's contract). A non-object `obj`
/// is a no-op pass-through.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_obj_set_proto(obj_word: u64, proto_word: u64) -> u64 {
    if PolyValue::from_raw(obj_word).is_object() {
        if let Ok(mut t) = proto_table().lock() {
            if PolyValue::from_raw(proto_word).is_object() {
                t.insert(obj_word, proto_word);
            } else {
                t.remove(&obj_word);
            }
        }
    }
    obj_word
}

/// `proto.isPrototypeOf(obj)` → walk `obj`'s prototype chain; `true` iff `proto`
/// appears in it. A bounded walk (a cycle — which `Object.create` cannot build —
/// stops at the visited cap) returns `false`.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_is_prototype_of(proto_word: u64, obj_word: u64) -> u64 {
    let mut cur = proto_of(obj_word);
    let mut guard = 0;
    while let Some(p) = cur {
        if p == proto_word {
            return PolyValue::bool(true).raw();
        }
        guard += 1;
        if guard > 10_000 {
            break;
        }
        cur = proto_of(p);
    }
    PolyValue::bool(false).raw()
}
