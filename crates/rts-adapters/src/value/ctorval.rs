//! Runtime support for `new <value>()` — constructing through a class VALUE (a
//! reified user class held in a local / `globalThis` / a field), slice 2 of
//! class-as-value.
//!
//! A class-value is a `TAG_FUNCTION` word whose stored thunk is the class's
//! NEW-THUNK (it allocates `this`, runs the typed constructor, returns the
//! `TAG_OBJECT` instance — see [`super::super::front::run::thunk`]). A plain
//! function value's thunk is an ordinary body thunk. The two are indistinguishable
//! by tag, so to keep `new` SOUND — a non-constructor must THROW, never
//! mis-construct — every class NEW-THUNK address is registered here at reify time
//! ([`__rtsadp_register_ctor_thunk`], emitted by `reify_class`). `new <value>()`
//! lowers to [`__rtsadp_new_invoke`], which invokes the value ONLY when its stored
//! thunk address is a registered constructor; otherwise it raises a
//! TypeError-style throw.
//!
//! Code addresses are stable for the whole program run and never reused, so the
//! address-set membership is EXACT — no slot/generation-reuse hazard (unlike a
//! handle-keyed marker would have).

use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

use super::PolyValue;
use super::abi_adapter::intern_poly;
use super::errslot::__rtsadp_throw_set;
use super::funcops::{__rtsadp_fn_invoke, __rtsadp_fn_ptr};

/// The set of class NEW-THUNK code addresses — the only thunk targets `new` may
/// invoke as a constructor.
static CTOR_THUNKS: OnceLock<Mutex<HashSet<u64>>> = OnceLock::new();

fn ctor_set() -> &'static Mutex<HashSet<u64>> {
    CTOR_THUNKS.get_or_init(|| Mutex::new(HashSet::new()))
}

/// Drop every registered constructor-thunk address. Called by
/// [`crate::state::reset_codegen_state`] at the start of each program compile so a
/// run does not inherit the previous run's class-reify addresses (stable for the
/// previous run only). Bounded by distinct reified classes, so small — reset is
/// for cross-run determinism, not a within-run leak.
pub fn reset_state() {
    if let Ok(mut s) = ctor_set().lock() {
        s.clear();
        s.shrink_to_fit();
    }
}

/// The number of registered constructor-thunk addresses (leak-test probe).
pub fn ctor_thunk_count() -> usize {
    ctor_set().lock().map(|s| s.len()).unwrap_or(0)
}

/// Register `addr` (a class NEW-THUNK code address) as a valid `new` target.
/// Emitted by `reify_class` whenever a class is reified as a value (idempotent —
/// the same class registers the same stable address every time).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_register_ctor_thunk(addr: u64) {
    if addr != 0 {
        ctor_set()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(addr);
    }
}

/// `new <value>(a0..a3, rest)` — construct through a class VALUE. When `fn_word`'s
/// stored thunk address is a registered class NEW-THUNK, invoke it (the thunk
/// allocates the instance, runs the constructor, and returns the `TAG_OBJECT`
/// instance word). Otherwise the value is NOT a constructor: set a pending
/// TypeError and return `undefined` (the caller's post-call error check unwinds).
/// Never mis-constructs a non-constructor.
/// `Reflect.construct(target, argsArray)` — construct through a class VALUE with
/// the args read from a REAL array word (up to the 4 positional uniform slots; a
/// wider ctor is a later increment). A PROXY target routes through its
/// `construct` trap (#218 phase 2): `handler.construct(target, argsArray)` — the
/// trap's return is the instance; no trap → construct the target.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_construct(target_word: u64, args_word: u64) -> u64 {
    if let Some((target, handler)) = super::objops::proxy_parts(target_word) {
        let trap_key = intern_poly("construct").raw();
        let trap = super::objops::__rtsadp_obj_get(handler, trap_key);
        if PolyValue::from_raw(trap).is_function() {
            let undef = PolyValue::undefined().raw();
            return __rtsadp_fn_invoke(trap, target, args_word, undef, undef, 0);
        }
        return __rtsadp_construct(target, args_word);
    }
    use rts_runtime::namespaces::collections::vec as rt_vec;
    use rts_runtime::namespaces::gc::handles as rt_handles;
    let undef = PolyValue::undefined().raw();
    let args = PolyValue::from_raw(args_word);
    let mut slots = [undef; 4];
    if args.is_object() {
        let h = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(args.as_handle());
        let len = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_LEN(h).max(0);
        for (i, slot) in slots.iter_mut().enumerate() {
            if (i as i64) < len {
                *slot = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_GET(h, i as i64) as u64;
            }
        }
    }
    __rtsadp_new_invoke(target_word, slots[0], slots[1], slots[2], slots[3], undef)
}

#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_new_invoke(
    fn_word: u64,
    a0: u64,
    a1: u64,
    a2: u64,
    a3: u64,
    rest: u64,
) -> u64 {
    let addr = __rtsadp_fn_ptr(fn_word);
    let is_ctor = addr != 0
        && ctor_set()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains(&addr);
    if is_ctor {
        return __rtsadp_fn_invoke(fn_word, a0, a1, a2, a3, rest);
    }
    // A PLAIN function value used as a constructor (`new F()` on a fn that was
    // never elevated to a synthetic class — e.g. an empty-body fn-ctor reached
    // through `bind`/`Reflect.construct`): JS runs the fn and yields its return
    // when that is an object, otherwise the fresh instance. The fn has no `this`
    // slot here, so the fresh instance is an empty keyed object — field writes
    // via `this` inside such a fn are the fn-ctor synthesis path, not this one.
    if addr != 0 && PolyValue::from_raw(fn_word).is_function() {
        let r = __rtsadp_fn_invoke(fn_word, a0, a1, a2, a3, rest);
        if PolyValue::from_raw(r).is_object() {
            return r;
        }
        return empty_keyed_object();
    }
    __rtsadp_throw_set(intern_poly("TypeError: value is not a constructor").raw());
    PolyValue::undefined().raw()
}

/// A fresh `{}` — an empty keyed object word (the `{}` literal's runtime shape).
fn empty_keyed_object() -> u64 {
    use rts_runtime::namespaces::collections::vec as rt_vec;
    use rts_runtime::namespaces::gc::handles as rt_handles;
    let h = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW();
    let shape = crate::shape::intern_global_shape(&[]);
    rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(h, PolyValue::from_i32(shape as i32).raw() as i64);
    PolyValue::from_object_handle(rt_handles::__RTS_FN_NS_GC_POLY_FROM_HANDLE(h)).raw()
}
