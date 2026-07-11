//! node:assert — the runtime bridges the assertions build on: JS equality
//! (`strict_eq`/`loose_eq`), truthiness (`to_boolean`), the pending-error slot
//! (for `throws`/`doesNotThrow`), the function-value invoker, and the throw
//! bridge — all reached by symbol at the final link. Plus a native structural
//! deep-equal over the engine value model.

use rts_engine::heap::handles::{with_entry, Entry};
use rts_engine::heap::poly::{
    poly_handle_normalize, POLY_BOX_BASE, POLY_PAYLOAD_MASK, POLY_TAG_MASK, POLY_TAG_OBJECT,
    POLY_TAG_SHIFT, POLY_UNDEFINED,
};
use rts_engine::heap::shapes::global_shape_keys;

unsafe extern "C" {
    fn __rtsadp_strict_eq(a: u64, b: u64) -> u64;
    fn __rtsadp_loose_eq(a: u64, b: u64) -> u64;
    fn __rtsadp_to_boolean(a: u64) -> u64;
    fn __rtsadp_err_pending() -> i64;
    fn __rtsadp_err_clear();
    fn __rtsadp_fn_invoke_method(f: u64, this: u64, a0: u64, a1: u64, a2: u64, rest: u64) -> u64;
    fn __rtsadp_throw_js_error(kp: *const u8, kl: i64, mp: *const u8, ml: i64);
}

/// JS `a === b`.
pub fn strict_eq(a: u64, b: u64) -> bool {
    poly_bool(unsafe { __rtsadp_strict_eq(a, b) })
}

/// JS `a == b`.
pub fn loose_eq(a: u64, b: u64) -> bool {
    poly_bool(unsafe { __rtsadp_loose_eq(a, b) })
}

/// JS truthiness of `v`. (`__rtsadp_to_boolean` returns a raw `0/1`, unlike the
/// `strict_eq`/`loose_eq` PolyValue-bool returns.)
pub fn truthy(v: u64) -> bool {
    (unsafe { __rtsadp_to_boolean(v) }) != 0
}

/// Invoke a function VALUE with no args; `true` iff it threw (the pending-error
/// slot was set), clearing the slot so the throw is "caught".
pub fn invoke_and_caught(fn_word: u64) -> bool {
    let u = POLY_UNDEFINED;
    unsafe { __rtsadp_fn_invoke_method(fn_word, u, u, u, u, u) };
    if unsafe { __rtsadp_err_pending() } != 0 {
        unsafe { __rtsadp_err_clear() };
        true
    } else {
        false
    }
}

/// Throw a JS `AssertionError` with `message` (the assertions' failure path).
pub fn throw_assertion(message: &str) {
    let kind = "AssertionError";
    unsafe {
        __rtsadp_throw_js_error(kind.as_ptr(), kind.len() as i64, message.as_ptr(), message.len() as i64);
    }
}

fn poly_bool(w: u64) -> bool {
    // `true` singleton word carries payload 3.
    (w & POLY_BOX_BASE) == POLY_BOX_BASE && (w & POLY_PAYLOAD_MASK) == 3
}

/// `null`/`undefined` singleton test (for `ifError`).
pub fn is_nullish(w: u64) -> bool {
    if (w & POLY_BOX_BASE) != POLY_BOX_BASE {
        return false;
    }
    // singleton tag with null(1)/undefined(0) payloads.
    let tag = (w >> POLY_TAG_SHIFT) & POLY_TAG_MASK;
    tag == 2 && matches!(w & POLY_PAYLOAD_MASK, 0 | 1)
}

/// Structural deep-equal of two value words. `strict` uses `===` at the leaves
/// and requires matching container shapes; loose uses `==`.
pub fn deep_equal(a: u64, b: u64, strict: bool) -> bool {
    if strict {
        if strict_eq(a, b) {
            return true;
        }
    } else if loose_eq(a, b) {
        return true;
    }
    // Both must be object/array containers to recurse.
    let (ha, hb) = match (poly_handle_normalize(a), poly_handle_normalize(b)) {
        (Some(x), Some(y)) => (x, y),
        _ => return false,
    };
    if !is_object_word(a) || !is_object_word(b) {
        return false;
    }
    match (structure(ha), structure(hb)) {
        (Struct::Array(xs), Struct::Array(ys)) => {
            xs.len() == ys.len() && xs.iter().zip(&ys).all(|(&x, &y)| deep_equal(x, y, strict))
        }
        (Struct::Object(xs), Struct::Object(ys)) => {
            xs.len() == ys.len()
                && xs.iter().all(|(k, xv)| {
                    ys.iter().find(|(k2, _)| k2 == k).is_some_and(|(_, yv)| deep_equal(*xv, *yv, strict))
                })
        }
        _ => false,
    }
}

fn is_object_word(w: u64) -> bool {
    (w & POLY_BOX_BASE) == POLY_BOX_BASE && ((w >> POLY_TAG_SHIFT) & POLY_TAG_MASK) == POLY_TAG_OBJECT
}

enum Struct {
    Array(Vec<u64>),
    Object(Vec<(String, u64)>),
    Other,
}

/// Classify a container handle: shaped object (slot 0 = a registered shape id) or
/// plain array (`Entry::Vec` of elements).
fn structure(handle: u64) -> Struct {
    with_entry(handle, |e| {
        let slots = match e {
            Some(Entry::Vec(v)) => v,
            _ => return Struct::Other,
        };
        if slots.is_empty() {
            return Struct::Array(Vec::new());
        }
        let w0 = slots[0] as u64;
        if (w0 & POLY_BOX_BASE) == POLY_BOX_BASE {
            let id = (w0 & POLY_PAYLOAD_MASK) as u32;
            if let Some(keys) = global_shape_keys(id) {
                if keys.len() + 1 == slots.len() {
                    return Struct::Object(
                        keys.into_iter().zip(slots[1..].iter().map(|&w| w as u64)).collect(),
                    );
                }
            }
        }
        Struct::Array(slots.iter().map(|&w| w as u64).collect())
    })
}
