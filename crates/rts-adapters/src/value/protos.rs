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

/// object word → the REGISTRY class NAME that constructed it (`new Date()` →
/// "Date"), recorded by `emit_registry_ctor`. Lets `getPrototypeOf(new Date())`
/// resolve to `Date.prototype` (a real object, not `null`) without a fixed
/// per-word proto link. This is a PLAIN insert (never `obj_set_proto`), so a Proxy
/// instance recorded here is unaffected: `__rtsadp_obj_proto_of` checks
/// `proxy_parts` FIRST and routes a Proxy through its `getPrototypeOf` trap, never
/// reaching this fallback. An explicit `setPrototypeOf` (a `proto_table` entry)
/// still wins — it is matched as `Some(..)` before this `None`-branch fallback.
fn registry_class_table() -> &'static Mutex<HashMap<u64, String>> {
    static T: OnceLock<Mutex<HashMap<u64, String>>> = OnceLock::new();
    T.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Record that `obj_word` was constructed by the Registry class named `name_word`.
/// Returns `obj_word` unchanged (pass-through, so the call site keeps the instance
/// word).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_record_registry_class(obj_word: u64, name_word: u64) -> u64 {
    let name = super::abi_adapter::resolve_poly(PolyValue::from_raw(name_word));
    if let Ok(mut t) = registry_class_table().lock() {
        t.insert(obj_word, name);
    }
    obj_word
}

fn registry_class_of(obj_word: u64) -> Option<String> {
    registry_class_table().lock().ok()?.get(&obj_word).cloned()
}

/// The INTRINSIC prototype of a PRIMITIVE receiver — the one JS reaches by
/// AUTOBOXING on property access (`"abc".toUpperCase`, `(255).toString`,
/// `true.toString`). A primitive is not a heap object, so it carries no
/// `[[Prototype]]` entry in [`proto_table`] and the chain walk in
/// [`super::objops::__rtsadp_obj_get`] would terminate at its own-slot miss —
/// which is why a RUNTIME-named member (`s[k]`, `s[k]()`) could not reach the
/// primitive method surface even though a statically-named one could.
///
/// The mapping is by TAG (never by a name match on the key), onto the PRIMORDIAL
/// wrapper classes `String`/`Number`/`Boolean` — whose prototypes are exactly
/// where the prelude `.ts` primitive-method libraries publish their methods
/// (`rts-codegen-new`'s `methodtable.rs` reifies every class method onto
/// `__rtsadp_class_proto(<class>)`). So ANY method those `.ts` classes define is
/// reachable by runtime name through the SAME chain walk an object uses — no
/// per-method table, no name special-casing.
///
/// `None` for a heap value (object/function — it has its own recorded link or an
/// implicit one) and for `undefined`/`null` (no wrapper: JS throws on property
/// access, which the `undefined` result already models here).
pub(crate) fn intrinsic_proto(word: u64) -> Option<u64> {
    let v = PolyValue::from_raw(word);
    let class = if v.is_string() {
        "String"
    } else if v.is_int32() || v.is_double() {
        "Number"
    } else if v.is_bool() {
        "Boolean"
    } else {
        return None;
    };
    let name = super::abi_adapter::intern_poly(class).raw();
    Some(__rtsadp_class_proto(name))
}

/// The next `[[Prototype]]` of ANY value — the ONE proto-step the unified chain
/// walk uses. Matches `Object.getPrototypeOf` (proxy traps, Array/Object implicit
/// roots, explicit null) and AUTOBOXES a primitive to its wrapper prototype (for
/// property-lookup consumers). Returns `null` at the end of a chain.
///
/// NOTE `instanceof` / `isPrototypeOf` must NOT autobox their START value (a
/// non-object `V` is `false` by spec); those callers guard the receiver kind
/// before walking, so the autobox branch only fires on a genuine primitive
/// receiver in a property-lookup walk.
pub(crate) fn proto_of_value(word: u64) -> u64 {
    // string / number / boolean → intrinsic wrapper prototype.
    if let Some(p) = intrinsic_proto(word) {
        return p;
    }
    // Objects (incl. arrays) AND functions: `getPrototypeOf` is the single source
    // (a function resolves to the shared `Function.prototype` there).
    __rtsadp_obj_proto_of(word)
}

/// Walk the prototype chain of `word` via [`proto_of_value`], calling `f` on each
/// prototype until it returns `true` (found) or the chain ends at `null`/`undefined`.
/// Bounded (a cycle stops at the visited cap). Returns whether `f` matched. This is
/// the ONE chain walk the proto-dependent operators (`isPrototypeOf`, and — as they
/// migrate — `instanceof` / `in` / property lookup) share.
pub(crate) fn walk_proto_chain(word: u64, mut f: impl FnMut(u64) -> bool) -> bool {
    let null = PolyValue::null().raw();
    let undef = PolyValue::undefined().raw();
    let mut cur = proto_of_value(word);
    let mut guard = 0;
    while cur != null && cur != undef && cur != 0 {
        if f(cur) {
            return true;
        }
        guard += 1;
        if guard > 10_000 {
            break;
        }
        cur = proto_of_value(cur);
    }
    false
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
    } else {
        // `Object.create(null)` — record the EXPLICIT null prototype (0) so
        // the `in` walk can tell "no Object.prototype at all" apart from a
        // plain object's implicit Object.prototype tail.
        if let Ok(mut t) = proto_table().lock() {
            t.insert(obj_word, 0);
        }
    }
    obj_word
}

/// `Object.getPrototypeOf(obj)` → the recorded prototype object word, or `null`.
/// A PROXY receiver routes through its `getPrototypeOf` trap (#218 phase 3);
/// no trap → forward to the target.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_obj_proto_of(obj_word: u64) -> u64 {
    if let Some((target, handler)) = super::objops::proxy_parts(obj_word) {
        let trap_key = super::abi_adapter::intern_poly("getPrototypeOf").raw();
        let trap = super::objops::__rtsadp_obj_get(handler, trap_key);
        if PolyValue::from_raw(trap).is_function() {
            let undef = PolyValue::undefined().raw();
            return super::funcops::__rtsadp_fn_invoke(trap, target, undef, undef, undef, 0);
        }
        return __rtsadp_obj_proto_of(target);
    }
    match proto_of(obj_word) {
        // The EXPLICIT null prototype (`Object.create(null)` records 0): JS
        // reads `null`, never the raw 0 word.
        Some(0) => PolyValue::null().raw(),
        Some(p) => p,
        None => {
            // A Registry-class instance (Date …) recorded at construction resolves
            // to `ClassName.prototype` — a real object, so `getPrototypeOf(new
            // Date())` is not `null`. (A Proxy is already handled by the
            // `proxy_parts` guard at the top of this fn; an explicit
            // `setPrototypeOf` matched `Some(..)` above.)
            if let Some(name) = registry_class_of(obj_word) {
                let name_w = super::abi_adapter::intern_poly(&name).raw();
                return __rtsadp_class_proto(name_w);
            }
            // No recorded link: an ARRAY's implicit prototype is the SAME
            // object `Array.prototype` resolves to (`__rtsadp_class_proto`),
            // so `Object.getPrototypeOf([]) === Array.prototype` holds.
            let v = PolyValue::from_raw(obj_word);
            if v.is_object() && !super::inspect::looks_like_object(v) {
                let name = super::abi_adapter::intern_poly("Array").raw();
                let undef = PolyValue::undefined().raw();
                return __rtsadp_class_proto_init(name, undef);
            }
            // A plain keyed object's implicit prototype is the SAME shared root
            // `Object.prototype` resolves to — except the ROOT itself, whose
            // chain ends in `null` (no extra "Object" loop).
            if v.is_object() && super::inspect::looks_like_object(v) {
                let root = object_proto_root();
                if obj_word == root {
                    return PolyValue::null().raw();
                }
                return root;
            }
            // A function's [[Prototype]] is the shared `Function.prototype` (whose
            // own [[Prototype]] is `Object.prototype`), so `getPrototypeOf(fn)` is
            // an object and a function's chain reaches Object and terminates.
            if v.is_function() {
                return function_proto_root();
            }
            PolyValue::null().raw()
        }
    }
}

/// `Object.setPrototypeOf(obj, proto)` / `Reflect.setPrototypeOf` — record (or
/// clear) `obj`'s prototype. A non-object `proto` (`null`) REMOVES the link (a null
/// prototype). Returns `obj` (Object.setPrototypeOf's contract). A non-object `obj`
/// is a no-op pass-through.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_obj_set_proto(obj_word: u64, proto_word: u64) -> u64 {
    // PROXY: same trap routing as `set_proto_check` (Object.setPrototypeOf's
    // contract returns the object either way).
    if super::objops::proxy_parts(obj_word).is_some() {
        __rtsadp_set_proto_check(obj_word, proto_word);
        return obj_word;
    }
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

/// `C.prototype` — the ONE shared prototype OBJECT of a class, created lazily
/// (an empty keyed object) and keyed by the class-name string. `new C()` links
/// each instance to it (`__rtsadp_obj_set_proto`), so `C.prototype.m = v`
/// reaches every instance through the dynamic-get prototype walk.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_class_proto(name_str_word: u64) -> u64 {
    let name = super::abi_adapter::resolve_poly(PolyValue::from_raw(name_str_word));
    // `Object.prototype` IS the shared root object — one identity everywhere
    // (`Object.getPrototypeOf({}) === Object.prototype` must hold).
    if name == "Object" {
        return object_proto_root();
    }
    let mut t = class_proto_table().lock().unwrap_or_else(|e| e.into_inner());
    if let Some(w) = t.get(&name) {
        return *w;
    }
    use rts_runtime::namespaces::collections::vec as rt_vec;
    use rts_runtime::namespaces::gc::handles as rt_handles;
    let h = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW();
    let shape = crate::shape::intern_global_shape(&[]);
    rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(h, PolyValue::from_i32(shape as i32).raw() as i64);
    // PIN: the proto word is cached for the whole program (like an interned
    // string constant) — it must never be swept.
    rt_handles::__RTS_FN_NS_GC_PIN_HANDLE(h);
    let w = PolyValue::from_object_handle(rt_handles::__RTS_FN_NS_GC_POLY_FROM_HANDLE(h)).raw();
    t.insert(name, w);
    w
}

/// name → the class's shared prototype object word (see [`__rtsadp_class_proto`]).
fn class_proto_table() -> &'static Mutex<HashMap<String, u64>> {
    static T: OnceLock<Mutex<HashMap<String, u64>>> = OnceLock::new();
    T.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Drop the per-program prototype state at a program boundary
/// ([`crate::state::reset_codegen_state`]). Both tables are keyed by / hold
/// HandleTable words from the current program's pool, which the reset drains —
/// a stale entry read by the next program points a recycled slot at the wrong
/// value. `class_proto_table` is keyed by class NAME but its VALUES are handles,
/// so it is drained too (the next program's methodtable prologue rebuilds it).
pub fn reset_state() {
    if let Ok(mut t) = proto_table().lock() {
        t.clear();
    }
    if let Ok(mut t) = class_proto_table().lock() {
        t.clear();
    }
    if let Ok(mut t) = registry_class_table().lock() {
        t.clear();
    }
}

/// `C.prototype = obj` / `F.prototype = obj` — REPLACE the shared prototype.
/// Instances created BEFORE keep the old proto (matching JS); instances created
/// after link to `obj`. A non-object clears to the lazy default.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_class_proto_set(name_word: u64, proto_word: u64) -> u64 {
    let name = super::abi_adapter::resolve_poly(PolyValue::from_raw(name_word));
    if let Ok(mut t) = class_proto_table().lock() {
        if PolyValue::from_raw(proto_word).is_object() {
            t.insert(name, proto_word);
        } else {
            t.remove(&name);
        }
    }
    proto_word
}

/// `__rtsadp_class_proto_init(name, parent_name_or_undefined)` — the ONE
/// idempotent per-class prototype setup, run on each `new C()` (cheap after the
/// first): ensures `C.prototype` exists (via [`__rtsadp_class_proto`]), gives it
/// a `constructor` object carrying `{ name: "C" }`, and links its
/// `[[Prototype]]` to the PARENT's prototype (`class C extends P`) or to the
/// shared `Object.prototype` root (whose own chain ends in `null` and whose
/// constructor name is `"Object"`). Returns the proto word for the instance
/// link. This is what makes `Object.getPrototypeOf(d).constructor.name` and the
/// `while (proto) { … getPrototypeOf(proto) }` chain walk behave like JS.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_class_proto_init(name_word: u64, parent_name_word: u64) -> u64 {
    use std::collections::HashSet;
    use std::sync::{Mutex, OnceLock};
    static DONE: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    let proto = __rtsadp_class_proto(name_word);
    let name = super::abi_adapter::resolve_poly(PolyValue::from_raw(name_word));
    {
        let done = DONE.get_or_init(|| Mutex::new(HashSet::new()));
        let mut d = done.lock().unwrap_or_else(|e| e.into_inner());
        if !d.insert(name) {
            return proto;
        }
    }
    // constructor = { name: "<C>" } (a data object — the callable-class link is
    // a later increment; `.constructor.name` is the observed surface).
    let ctor = empty_proto_object();
    let name_key = super::abi_adapter::intern_poly("name").raw();
    super::objops::__rtsadp_obj_set(ctor, name_key, name_word);
    let ctor_key = super::abi_adapter::intern_poly("constructor").raw();
    super::objops::__rtsadp_obj_set(proto, ctor_key, ctor);
    // Chain: parent's proto, or the Object.prototype root — but NEVER overwrite
    // a [[Prototype]] the user already wired (`F.prototype =
    // Object.create(Base.prototype)` runs before the first `new F()`).
    if proto_of(proto).is_none() {
        let parent = PolyValue::from_raw(parent_name_word);
        let up = if parent.is_string() {
            __rtsadp_class_proto(parent_name_word)
        } else {
            object_proto_root()
        };
        // Never self-link (the "Object" proto IS the root — a cycle would hang
        // every chain walk).
        if up != proto {
            __rtsadp_obj_set_proto(proto, up);
        }
    }
    proto
}

/// The shared `Object.prototype` root: constructor name `"Object"`, no
/// prototype above it (`getPrototypeOf` → null, terminating chain walks).
pub(crate) fn object_proto_root() -> u64 {
    use std::sync::OnceLock;
    static ROOT: OnceLock<u64> = OnceLock::new();
    *ROOT.get_or_init(|| {
        let root = empty_proto_object();
        let ctor = empty_proto_object();
        let name_key = super::abi_adapter::intern_poly("name").raw();
        let obj_name = super::abi_adapter::intern_poly("Object").raw();
        super::objops::__rtsadp_obj_set(ctor, name_key, obj_name);
        let ctor_key = super::abi_adapter::intern_poly("constructor").raw();
        super::objops::__rtsadp_obj_set(root, ctor_key, ctor);
        root
    })
}

/// The shared `Function.prototype` root: every function's `[[Prototype]]`. Its own
/// `[[Prototype]]` is `Object.prototype`, so a function's chain reaches Object and
/// terminates in `null`. Created lazily, PINned for the whole program.
fn function_proto_root() -> u64 {
    use std::sync::OnceLock;
    static ROOT: OnceLock<u64> = OnceLock::new();
    *ROOT.get_or_init(|| {
        let root = empty_proto_object();
        __rtsadp_obj_set_proto(root, object_proto_root());
        root
    })
}

/// A fresh empty keyed object word, PINned (proto-graph objects live for the
/// whole program).
fn empty_proto_object() -> u64 {
    use rts_runtime::namespaces::collections::vec as rt_vec;
    use rts_runtime::namespaces::gc::handles as rt_handles;
    let h = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW();
    let shape = crate::shape::intern_global_shape(&[]);
    rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(h, PolyValue::from_i32(shape as i32).raw() as i64);
    rt_handles::__RTS_FN_NS_GC_PIN_HANDLE(h);
    PolyValue::from_object_handle(rt_handles::__RTS_FN_NS_GC_POLY_FROM_HANDLE(h)).raw()
}

/// `Reflect.setPrototypeOf(obj, proto)` — like [`__rtsadp_obj_set_proto`] but
/// returns SUCCESS as an i64 0/1, and routes a PROXY receiver through its
/// `setPrototypeOf` trap (#218 phase 3): ToBoolean of the trap's return decides
/// (a `false` trap REJECTS — the target is not written); no trap → forward to
/// the target. A non-object receiver fails (`0`).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_set_proto_check(obj_word: u64, proto_word: u64) -> i64 {
    if let Some((target, handler)) = super::objops::proxy_parts(obj_word) {
        let trap_key = super::abi_adapter::intern_poly("setPrototypeOf").raw();
        let trap = super::objops::__rtsadp_obj_get(handler, trap_key);
        if PolyValue::from_raw(trap).is_function() {
            let undef = PolyValue::undefined().raw();
            let r = super::funcops::__rtsadp_fn_invoke(trap, target, proto_word, undef, undef, 0);
            return PolyValue::from_raw(r).is_truthy() as i64;
        }
        return __rtsadp_set_proto_check(target, proto_word);
    }
    if !PolyValue::from_raw(obj_word).is_object() {
        return 0;
    }
    __rtsadp_obj_set_proto(obj_word, proto_word);
    1
}

/// `proto.isPrototypeOf(obj)` → walk `obj`'s prototype chain; `true` iff `proto`
/// appears in it. A bounded walk (a cycle — which `Object.create` cannot build —
/// stops at the visited cap) returns `false`.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_is_prototype_of(proto_word: u64, obj_word: u64) -> u64 {
    // Spec: a non-object `V` is never a prototype child (and must NOT autobox).
    let v = PolyValue::from_raw(obj_word);
    if !(v.is_object() || v.is_function()) {
        return PolyValue::bool(false).raw();
    }
    // ONE unified chain walk — no per-class (`Object`/`Array`) hardcode. The walk's
    // `proto_of_value` already yields the primordial `Array.prototype` /
    // `Object.prototype` roots and routes a function to `Object.prototype`.
    let found = walk_proto_chain(obj_word, |p| p == proto_word);
    PolyValue::bool(found).raw()
}

/// `obj instanceof <named class>` via the UNIFIED walk: `true` iff
/// `ClassName.prototype` (`__rtsadp_class_proto(name)`) appears in `obj`'s
/// prototype chain. A non-object / non-function `obj` is never an instance (spec:
/// no autobox). The GENERAL instanceof path — the compile-time fast branches
/// (user-class shape-id set, `Object` tag check, fn-ctor ctor-table) handle their
/// cases first; this catches the rest (a registered class the fast branches don't
/// name) off the one `walk_proto_chain`, so `instanceof` shares the single
/// prototype mechanism instead of a per-class predicate.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_instanceof_walk(obj_word: u64, class_name_word: u64) -> u64 {
    let v = PolyValue::from_raw(obj_word);
    if !(v.is_object() || v.is_function()) {
        return PolyValue::bool(false).raw();
    }
    // A struct-backed `#[rtse::class]` instance (`Entry::Rtse`) carries its JS class
    // name. `x instanceof C` is true iff that class IS `C` or DESCENDS from it
    // (`class_registry::is_descendant_of`, populated from the Registry's `extends`).
    // Only a POSITIVE match short-circuits — on no match we FALL THROUGH to the
    // prototype walk so `rtseInstance instanceof Object` (and any real proto
    // relationship) still resolves.
    if let Some(h) = rts_engine::heap::poly::poly_handle_normalize(obj_word) {
        if let Some(cls) = rts_engine::heap::handles::rtse_class_of(h) {
            if let Some(target_name) = rts_engine::heap::handles::read_string_handle(
                rts_engine::heap::poly::poly_handle_normalize(class_name_word)
                    .unwrap_or(class_name_word),
            ) {
                if rts_engine::heap::class_registry::is_descendant_of(cls, &target_name) {
                    return PolyValue::bool(true).raw();
                }
            }
        }
    }
    let target = __rtsadp_class_proto(class_name_word);
    let found = walk_proto_chain(obj_word, |p| p == target);
    PolyValue::bool(found).raw()
}
