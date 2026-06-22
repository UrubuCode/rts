//! Codegen-owned first-class FUNCTION-value trampolines (P4.6).
//!
//! Function is a PRIMORDIAL the new engine OWNS. A function used as a VALUE
//! (passed as an arg, stored in a `let`, returned, called through a variable) is
//! a `TAG_FUNCTION` [`PolyValue`] whose 48-bit payload is a REAL `rts-engine`
//! heap handle to an [`Entry::Function`] storing the function's JIT address, its
//! declared param count, and a has-rest flag. Because it is a real heap handle
//! boxed as `TAG_FUNCTION`, the GC's `poly_handle_normalize` (already handling
//! `TAG_FUNCTION`) marks it — GC-safe, no codegen-side table.
//!
//! These are codegen-owned `__rtsadp_*` trampolines (NOT `__RTS_FN_*`) exactly
//! like [`super::genops`] / [`super::arrayops`]: the new engine does NOT use the
//! frozen old engine's `invoke_n` / FUNCTION_REIFY (those carry the old
//! i64-overloaded convention). The uniform indirect-call ABI is FIXED:
//!
//! ```text
//! extern "C" fn(env: u64, a0: u64, a1: u64, a2: u64, a3: u64, rest: u64) -> u64
//! ```
//!
//! A leading `env` PolyValue word (P5.7) holding the captured-value array (a real
//! `Entry::Vec`, `TAG_OBJECT`) for a CLOSURE, or `undefined` (0-payload singleton)
//! for a non-capturing function — every thunk takes it, a non-capturing thunk
//! IGNORES it. Then four positional PolyValue words in `a0..a3` + a `rest`
//! PolyValue holding an ARRAY of any remaining args (or `undefined` when ≤4 args).
//! The return is one PolyValue word. A callee with ≤4 params reads `a0..a3`
//! (ignoring unused); a `...rest` callee reads the overflow from the `rest` array.
//! The common ≤4-arg case never allocates `rest`. This is the THUNK signature
//! `func_addr` points at — never a variable-arity call.
//!
//! ## Where the env comes from (P5.7)
//!
//! A function VALUE's [`Entry::Function`] stores BOTH the thunk address (`fn_ptr`)
//! AND the env PolyValue word (reusing the `bound_this` slot — a captured-value
//! array word for a closure, `0` for a non-capturing function). `__rtsadp_fn_invoke`
//! reads the stored env from the function handle and passes it as the FIRST arg,
//! so the call sites of `__rtsadp_fn_invoke` are unchanged — the env travels with
//! the function value itself.

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::OnceLock;

use rts_runtime::namespaces::gc::handles::{Entry, FunctionData, alloc_entry, with_entry};

use super::PolyValue;

/// Side-table mapping an INSTANCE object word → the CONSTRUCTOR function's
/// identity (its `FunctionData.fn_ptr`, i.e. the THUNK address), recorded by
/// `new F()` and read by `x instanceof F`. This is the codegen-owned analogue of
/// the Map/Set kind-set: a small global table keyed by the boxed `TAG_OBJECT`
/// PolyValue word (stable + unique per instance, since it encodes slot+shard).
///
/// Function identity (the value compared) is `FunctionData.fn_ptr`, which is the
/// SAME whether `F` is reached as a direct call (`new F()` marks with the thunk
/// address) or as a function VALUE (`__rtsadp_fn_ptr` reads the same field) — so
/// `instanceof` agrees across both reach paths.
fn ctor_table() -> &'static Mutex<HashMap<u64, u64>> {
    static T: OnceLock<Mutex<HashMap<u64, u64>>> = OnceLock::new();
    T.get_or_init(|| Mutex::new(HashMap::new()))
}

/// `__rtsadp_fn_ptr(fn_word)` — the CONSTRUCTOR identity of a function VALUE: the
/// stored `FunctionData.fn_ptr` (the uniform thunk address). Returns `0` when the
/// word is not a live function value (so a `new <non-fn>()` / `x instanceof
/// <non-fn>` decides `false`, never crashes).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_fn_ptr(fn_word: u64) -> u64 {
    let pv = PolyValue::from_raw(fn_word);
    if !pv.is_function() {
        return 0;
    }
    let real = rts_runtime::namespaces::gc::handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(pv.as_handle());
    with_entry(real, |e| match e {
        Some(Entry::Function(d)) => d.fn_ptr,
        _ => 0,
    })
}

/// `__rtsadp_ctor_mark(obj_word, fn_ptr)` — record that the instance `obj_word`
/// was constructed by the function whose identity is `fn_ptr`. A non-object word
/// or a `0` fn_ptr is ignored (no entry recorded). The key is the full boxed
/// `TAG_OBJECT` PolyValue word.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_ctor_mark(obj_word: u64, fn_ptr: u64) {
    let pv = PolyValue::from_raw(obj_word);
    if !pv.is_object() || fn_ptr == 0 {
        return;
    }
    if let Ok(mut t) = ctor_table().lock() {
        t.insert(obj_word, fn_ptr);
    }
}

/// `__rtsadp_instanceof_fn(obj_word, fn_ptr)` — PolyValue bool: `true` iff
/// `obj_word` is an object whose RECORDED constructor identity equals `fn_ptr`.
/// (Future: walk a prototype chain; exact match for now.) A non-object word or an
/// unrecorded instance yields `false`.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_instanceof_fn(obj_word: u64, fn_ptr: u64) -> u64 {
    let pv = PolyValue::from_raw(obj_word);
    if !pv.is_object() || fn_ptr == 0 {
        return PolyValue::bool(false).raw();
    }
    let matched = ctor_table()
        .lock()
        .map(|t| t.get(&obj_word).copied() == Some(fn_ptr))
        .unwrap_or(false);
    PolyValue::bool(matched).raw()
}

// ---- function-VALUE data properties (`F.foo = v` / `F.foo`) (Phase 4) ----

/// Side-table mapping `(fn identity, property name) → PolyValue word`, recording a
/// data property assigned on a FUNCTION value (`F.foo = v`) and read back by
/// `F.foo`. The `String` is OWNED (a copy of the compile-time-known property name
/// the lowering passes as a `(ptr,len)` slice). This is the analogue of the
/// `ctor_table` above — a small global table keyed by the function's STABLE
/// identity, so statics on a dual-callable function (`String.fromCharCode = …`)
/// and the array.ts pattern resolve through one path.
fn fn_prop_table() -> &'static Mutex<HashMap<(u64, String), u64>> {
    static T: OnceLock<Mutex<HashMap<(u64, String), u64>>> = OnceLock::new();
    T.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Resolve a FUNCTION value's STABLE identity from the word the lowering passed.
/// The identity is the uniform-ABI THUNK address (`FunctionData.fn_ptr`) — the
/// SAME value `new F()` records and `x instanceof F` compares.
///
/// Two reach paths converge here: a reified function VALUE (a `TAG_FUNCTION`
/// PolyValue, e.g. a non-`this` free function or arrow stored in a `let`) resolves
/// to its stored `fn_ptr`; a raw thunk-address word (what the lowering hands for a
/// `this`-using fn-ctor like `const F = function(this){…}`, which cannot reify as
/// a value) IS the `fn_ptr` already and passes straight through. Either way the
/// identity is the thunk address, so `F.foo` and `F.foo = v` agree across both.
fn fn_identity(fn_word: u64) -> u64 {
    let pv = PolyValue::from_raw(fn_word);
    if pv.is_function() {
        let real =
            rts_runtime::namespaces::gc::handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(pv.as_handle());
        return with_entry(real, |e| match e {
            Some(Entry::Function(d)) => d.fn_ptr,
            _ => 0,
        });
    }
    // Not a function PolyValue: the lowering passed the raw thunk address directly
    // (a `this`-using fn-ctor's identity). Use it as-is.
    fn_word
}

/// Read the `key`-named property recorded on the function `fn_word`, or the
/// `undefined` singleton word when absent. `key_word` is the property NAME as an
/// interned string PolyValue word (the same convention `__rtsadp_obj_get` uses for
/// a static key). A non-function / unrecorded key reads `undefined` — the
/// JS-correct missing-property result, never a crash.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_fn_get_prop(fn_word: u64, key_word: u64) -> u64 {
    let id = fn_identity(fn_word);
    if id == 0 {
        return PolyValue::undefined().raw();
    }
    let key = key_text(key_word);
    fn_prop_table()
        .lock()
        .ok()
        .and_then(|t| t.get(&(id, key)).copied())
        .unwrap_or_else(|| PolyValue::undefined().raw())
}

/// Record `fn_word.<key> = value_word` in the function-property side-table,
/// returning `value_word` (JS assignment evaluates to the assigned value). A
/// non-function `fn_word` (identity 0) records nothing but still returns the value.
/// `key_word` is the property NAME as an interned string PolyValue word.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_fn_set_prop(fn_word: u64, key_word: u64, value_word: u64) -> u64 {
    let id = fn_identity(fn_word);
    if id != 0 {
        let key = key_text(key_word);
        if let Ok(mut t) = fn_prop_table().lock() {
            t.insert((id, key), value_word);
        }
    }
    value_word
}

/// The UTF-8 text of a property-key PolyValue: a string PolyValue is read from the
/// real pool; a non-string defensively coerces through the engine ToString (the
/// lowering always interns a static string name, so the string path is the norm).
fn key_text(key_word: u64) -> String {
    let k = PolyValue::from_raw(key_word);
    if k.is_string() {
        super::abi_adapter::resolve_poly(k)
    } else {
        let s_word = super::genops::__rtsadp_to_string(key_word);
        super::abi_adapter::resolve_poly(PolyValue::from_raw(s_word))
    }
}

/// Drop every entry of the two runtime side-tables (`ctor_table`,
/// `fn_prop_table`). Called by [`crate::state::reset_codegen_state`] at the start
/// of each program compile so a fresh run does not inherit the previous run's
/// instance→ctor and function-property records. NOTE: this BOUNDS growth across
/// runs (eval / hot-reload / the test suite); it does NOT bound the WITHIN-a-run
/// growth of `ctor_table` (one entry per `new` executed) — that needs GC-tied
/// pruning / inline storage, a separate increment.
pub fn reset_state() {
    if let Ok(mut t) = ctor_table().lock() {
        t.clear();
        t.shrink_to_fit();
    }
    if let Ok(mut t) = fn_prop_table().lock() {
        t.clear();
        t.shrink_to_fit();
    }
}

/// `(ctor_table.len(), fn_prop_table.len())` — the live entry counts of the two
/// runtime side-tables. Used by the leak tests to assert the tables do not grow
/// unbounded across repeated program runs (each run resets them).
pub fn table_lens() -> (usize, usize) {
    let ctor = ctor_table().lock().map(|t| t.len()).unwrap_or(0);
    let prop = fn_prop_table().lock().map(|t| t.len()).unwrap_or(0);
    (ctor, prop)
}

/// The fixed uniform indirect-call signature every function VALUE is invoked
/// through: a leading `env` word + 4 positional PolyValue words + a `rest`
/// PolyValue (an array word or `undefined`), returning one PolyValue word.
type UniformFn = extern "C" fn(u64, u64, u64, u64, u64, u64) -> u64;

/// `__rtsadp_fn_reify(addr, nparams, has_rest, env_word)` — allocate a heap
/// [`Entry::Function`] storing the THUNK address `addr`, the declared param count
/// `nparams`, the `has_rest` flag, and the captured-environment PolyValue word
/// `env_word` (a `TAG_OBJECT` captured-value array for a CLOSURE, or `0` for a
/// non-capturing function), and return the bare 48-bit slot+shard payload (the
/// caller boxes it as a `TAG_FUNCTION` PolyValue). Reuses `Entry::Function` so NO
/// new `Entry` variant is added; the env rides the otherwise-unused `bound_this`
/// slot.
///
/// The returned value is the PolyValue PAYLOAD (slot+shard), matching the
/// `POLY_FROM_HANDLE` convention — the lowering then OR-s in the `TAG_FUNCTION`
/// header. The full real handle (with generation) is reconstructed on demand by
/// `POLY_TO_HANDLE` when the value is invoked / marked.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_fn_reify(addr: u64, nparams: u64, has_rest: u64, env_word: u64) -> u64 {
    let data = FunctionData {
        fn_ptr: addr,
        arity: nparams.min(u8::MAX as u64) as u8,
        name: Box::<str>::from(""),
        // The captured-env PolyValue word rides `bound_this` (a closure's env
        // array word, or 0 for a non-capturing fn). `has_bound_this` is set when
        // there IS an env, so the GC `poly_handle_normalize` over the entry can
        // mark the captured array reachable through the live function value.
        bound_this: env_word as i64,
        has_bound_this: env_word != 0,
        bound_args: Vec::new(),
        is_arrow: false,
        has_this_param: false,
        param_kinds: Vec::new(),
        return_kind: 0,
        packed_shim: 0,
        source: None,
        keep_alive: None,
        prototype_handle: 0,
        // Reuse the existing rest marker: >= 0 means "has a rest param at this
        // declared index"; the thunk is the only invoker, so the exact index is
        // not read back here — we only need the live/non-live distinction.
        rest_param_idx: if has_rest != 0 {
            (nparams.max(1) as i32) - 1
        } else {
            -1
        },
    };
    let handle = alloc_entry(Entry::Function(Box::new(data)));
    // Drop the 16-bit generation → bare 48-bit slot+shard payload.
    handle & super::PAYLOAD_MASK
}

/// `__rtsadp_fn_invoke(fn_word, a0, a1, a2, a3, rest)` — read the THUNK address
/// from the function value `fn_word` (a `TAG_FUNCTION` PolyValue word), transmute
/// it to the uniform 5-slot signature, call it with the marshaled args, and
/// return the resulting PolyValue word.
///
/// `fn_word` is the full PolyValue word; its 48-bit payload is the heap slot. We
/// reconstruct the live generation via `with_entry` keyed by the full real
/// handle. A non-function / dead handle yields `undefined` (never a crash).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_fn_invoke(
    fn_word: u64,
    a0: u64,
    a1: u64,
    a2: u64,
    a3: u64,
    rest: u64,
) -> u64 {
    let pv = PolyValue::from_raw(fn_word);
    if !pv.is_function() {
        return PolyValue::undefined().raw();
    }
    // Reconstruct the full real handle (generation read from the live slot) from
    // the bare 48-bit payload, then read the stored thunk address AND env word.
    let real = rts_runtime::namespaces::gc::handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(pv.as_handle());
    let (addr, env) = with_entry(real, |e| match e {
        Some(Entry::Function(d)) => (d.fn_ptr, d.bound_this as u64),
        _ => (0, 0),
    });
    if addr == 0 {
        return PolyValue::undefined().raw();
    }
    // A non-capturing function stored env = 0; normalize that to the `undefined`
    // singleton word the thunk's env-read path expects (it never reads it anyway).
    let env_word = if env == 0 {
        PolyValue::undefined().raw()
    } else {
        env
    };
    // SAFETY: `addr` is the address of a THUNK the new engine emitted with EXACTLY
    // the uniform `extern "C" fn(env,u64,u64,u64,u64,u64) -> u64` signature (see
    // `front::run::thunk`). The JIT module that defined it is kept mapped for the
    // whole run (`Program` owns it). Transmuting a code address to its true
    // declared signature is sound.
    let f: UniformFn = unsafe { std::mem::transmute::<u64, UniformFn>(addr) };
    f(env_word, a0, a1, a2, a3, rest)
}
