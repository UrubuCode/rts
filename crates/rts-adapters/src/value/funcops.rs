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

// ---- thenable-adoption SETTLERS (spec PromiseResolve, used by
// `__rtsadp_promise_resolve_w`) ----

/// Uniform-ABI thunk: RESOLVE the promise whose raw handle rides `env`
/// (the settler fn's bound env slot) with `a0`.
extern "C" fn settle_resolve_thunk(env: u64, a0: u64, _a1: u64, _a2: u64, _a3: u64, _r: u64) -> u64 {
    use rts_runtime::namespaces::promise_slot;
    rts_runtime::namespaces::gc::handles::with_entry(env, |e| {
        if let Some(Entry::PromiseAsync(slot)) = e {
            promise_slot::resolve(slot, a0 as i64);
        }
    });
    PolyValue::undefined().raw()
}

/// Uniform-ABI thunk: REJECT the promise whose raw handle rides `env` with `a0`.
extern "C" fn settle_reject_thunk(env: u64, a0: u64, _a1: u64, _a2: u64, _a3: u64, _r: u64) -> u64 {
    use rts_runtime::namespaces::promise_slot;
    rts_runtime::namespaces::gc::handles::with_entry(env, |e| {
        if let Some(Entry::PromiseAsync(slot)) = e {
            promise_slot::mark_handled(slot);
            promise_slot::reject(slot, a0 as i64);
        }
    });
    PolyValue::undefined().raw()
}

/// Uniform-ABI thunk for a `Math.max`/`Math.min` FUNCTION VALUE (env carries
/// the op: 1 = max, 0 = min). Reads `a0..a3` up to the argc the `rest` slot
/// carries (≤4-arg convention) or the overflow array (>4), reducing with the
/// same NaN-propagating fold the static path uses.
extern "C" fn math_minmax_thunk(env: u64, a0: u64, a1: u64, a2: u64, a3: u64, rest: u64) -> u64 {
    let is_max = env == 1;
    let mut acc = if is_max { f64::NEG_INFINITY } else { f64::INFINITY };
    let mut nan = false;
    let mut take = |w: u64| {
        let n = super::genops::dyn_to_number(w);
        if n.is_nan() {
            nan = true;
        } else if (is_max && n > acc) || (!is_max && n < acc) {
            acc = n;
        }
    };
    let rv = PolyValue::from_raw(rest);
    if rv.is_object() {
        // >4 args: a0..a3 + the overflow array.
        for w in [a0, a1, a2, a3] {
            take(w);
        }
        let h = rts_runtime::namespaces::gc::handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(rv.as_handle());
        let len = rts_runtime::namespaces::collections::vec::__RTS_FN_NS_COLLECTIONS_VEC_LEN(h)
            .max(0);
        for i in 0..len {
            take(rts_runtime::namespaces::collections::vec::__RTS_FN_NS_COLLECTIONS_VEC_GET(h, i)
                as u64);
        }
    } else {
        let argc = if rv.is_int32() { rv.as_i32().clamp(0, 4) } else { 4 };
        for (i, w) in [a0, a1, a2, a3].into_iter().enumerate() {
            if (i as i32) < argc {
                take(w);
            }
        }
    }
    if nan {
        return PolyValue::from_f64(f64::NAN).raw();
    }
    PolyValue::from_f64(acc).raw()
}

/// `Math.max` / `Math.min` read as a first-class FUNCTION VALUE
/// (`Reflect.apply(Math.max, null, xs)`, `arr.map(Math.max)` …): a real
/// callable whose env slot carries the op (1 = max, 0 = min).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_math_fn_value(op: u64) -> u64 {
    let data = FunctionData {
        fn_ptr: math_minmax_thunk as usize as u64,
        arity: 2,
        name: Box::<str>::from(if op == 1 { "max" } else { "min" }),
        bound_this: op as i64,
        has_bound_this: true,
        bound_args: Vec::new(),
        is_arrow: false,
        has_this_param: false,
        param_kinds: Vec::new(),
        return_kind: 0,
        packed_shim: 0,
        source: None,
        keep_alive: None,
        prototype_handle: 0,
        rest_param_idx: -1,
        uniform_thunk: true,
    };
    let h = alloc_entry(Entry::Function(Box::new(data)));
    PolyValue::from_function_handle(h & super::PAYLOAD_MASK).raw()
}

/// Build the `(resolve, reject)` settler FUNCTION-value pair over the promise
/// `out_handle` (raw) — real callable `TAG_FUNCTION` words whose env carries
/// the promise handle, exactly what a thenable's `then(res, rej)` expects.
pub(crate) fn settler_pair(out_handle: u64) -> (u64, u64) {
    let mk = |addr: u64| -> u64 {
        let data = FunctionData {
            fn_ptr: addr,
            arity: 1,
            name: Box::<str>::from(""),
            bound_this: out_handle as i64,
            has_bound_this: true,
            bound_args: Vec::new(),
            is_arrow: false,
            has_this_param: false,
            param_kinds: Vec::new(),
            return_kind: 0,
            packed_shim: 0,
            source: None,
            keep_alive: None,
            prototype_handle: 0,
            rest_param_idx: -1,
            uniform_thunk: true,
        };
        let h = alloc_entry(Entry::Function(Box::new(data)));
        PolyValue::from_function_handle(h & super::PAYLOAD_MASK).raw()
    };
    (
        mk(settle_resolve_thunk as usize as u64),
        mk(settle_reject_thunk as usize as u64),
    )
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
    fn_reify_core(addr, nparams, has_rest, env_word, false)
}

/// fn_ptr → the BOX kind of an async fn's RESOLVE value (see
/// [`__rtsadp_promise_spawn`]): 0 = raw int (→ INT32/f64 number word),
/// 1 = f64 BITS (→ inline double word), 2 = bool, 3 = already a WORD (a
/// Tagged-return body — verbatim). Distinct from the INVOKE return kind (which
/// tells `invoke_typed` which register to read).
fn async_box_kinds() -> &'static Mutex<HashMap<u64, u8>> {
    static T: OnceLock<Mutex<HashMap<u64, u8>>> = OnceLock::new();
    T.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The resolve-value BOXER installed on the async spawn: converts the raw
/// invoke return into the correct PolyValue WORD by the registered box kind.
/// This is what makes a NEGATIVE i64 return survive — its raw bits fall inside
/// the NaN-box quadrant and would read back as a boxed garbage object.
extern "C" fn box_async_result(fn_ptr: u64, raw: i64) -> i64 {
    let kind = async_box_kinds()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&fn_ptr)
        .copied()
        .unwrap_or(3);
    let w = match kind {
        1 => PolyValue::from_f64(f64::from_bits(raw as u64)).raw(),
        2 => PolyValue::bool(raw != 0).raw(),
        3 => raw as u64,
        _ => match i32::try_from(raw) {
            Ok(i) => PolyValue::from_i32(i).raw(),
            // An integer beyond i32 rides as a JS double (exact ≤ 2^53).
            Err(_) => PolyValue::from_f64(raw as f64).raw(),
        },
    };
    w as i64
}

/// `__rtsadp_invoke_auto_word(callee_word, this_word, args_vec)` — invoke a
/// function VALUE word through the FULL `INVOKE_AUTO` machinery (bound args,
/// param kinds, uniform-thunk env, VARIADIC tail packing — a `(...args)` rest
/// callee gets its overflow packed into ONE array). The raw 6-slot
/// `__rtsadp_fn_invoke` cannot pack (it carries no arg COUNT); call sites that
/// know the exact argc (the singleton-override dispatch) route here with a
/// counted args vec instead.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_invoke_auto_word(callee_word: u64, this_word: u64, args_vec: u64) -> u64 {
    use rts_engine::heap::handles::{alloc_entry, with_entry, Entry as E};
    let h = rts_engine::heap::poly::poly_handle_normalize(callee_word).unwrap_or(callee_word);
    // A VARIADIC uniform-thunk callee needs NO pre-packing here: the THUNK
    // packs the positional tail into the rest array itself (`__rtsadp_pack_rest`)
    // — pre-packing double-wrapped the tail.
    let vec_arg = args_vec;
    rts_runtime::namespaces::globals::function::ops::__RTS_FN_RT_INVOKE_AUTO(
        h as i64,
        this_word as i64,
        vec_arg,
    ) as u64
}

/// `__rtsadp_register_fn_abi(fn_addr, kinds_packed, meta)` — register a user
/// fn's packed ABI (param kinds + return kind) in the FN_KINDS registry, so any
/// RAW-pointer consumer that invokes through the registry path
/// (`invoke_fn_ptr_with_registry` — `thread.spawn`, dynamic HOFs, the async
/// spawn) reads f64 params/returns through the right registers. Emitted by
/// `getPointer(f)` when `f` carries an f64 in its signature (#247).
/// `meta`: bits 0..7 = return kind, bits 8..15 = argc.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_register_fn_abi(fn_addr: u64, kinds_packed: u64, meta: u64) {
    let argc = ((meta >> 8) & 0xFF) as usize;
    let ret_kind = (meta & 0xFF) as i32;
    let kinds: Vec<u8> = (0..argc)
        .map(|i| ((kinds_packed >> (8 * i)) & 0xFF) as u8)
        .collect();
    rts_runtime::namespaces::globals::function::ops::__RTS_FN_RT_REGISTER_FN_KINDS(
        fn_addr,
        kinds.as_ptr() as i64,
        kinds.len() as i64,
        ret_kind,
    );
}

/// `__rtsadp_promise_spawn(fn_addr, args_vec, kinds_packed, meta)` — the ASYNC
/// function CALL spawn (see `front/run/asyncspawn.rs`). Registers the callee's
/// packed ABI (one kind byte per param — 0 = i64-class, 1 = f64-bits, 2 = bool)
/// in the FN_KINDS registry so the spawned invoke goes through `invoke_typed`
/// with the REAL param kinds (an f64 param reads its bits back into xmm —
/// never the raw-i64 misload), records the BOX kind of the return (`meta`
/// bits 16..23), and spawns via `create_spawn_boxed` — the resolve value is
/// boxed into a proper PolyValue WORD (see [`box_async_result`]).
///
/// `meta`: bits 0..7 = invoke return kind, bits 8..15 = argc, bits 16..23 =
/// resolve box kind.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_promise_spawn(
    fn_addr: u64,
    args_vec: u64,
    kinds_packed: u64,
    meta: u64,
) -> u64 {
    let argc = ((meta >> 8) & 0xFF) as usize;
    let ret_kind = (meta & 0xFF) as i32;
    let box_kind = ((meta >> 16) & 0xFF) as u8;
    let kinds: Vec<u8> = (0..argc)
        .map(|i| ((kinds_packed >> (8 * i)) & 0xFF) as u8)
        .collect();
    rts_runtime::namespaces::globals::function::ops::__RTS_FN_RT_REGISTER_FN_KINDS(
        fn_addr,
        kinds.as_ptr() as i64,
        kinds.len() as i64,
        ret_kind,
    );
    async_box_kinds()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(fn_addr, box_kind);
    rts_runtime::namespaces::promise::create_spawn_boxed(fn_addr, args_vec, box_async_result)
}

/// Reify a THIS-FIRST function (a method or ES5 `function (this: any)` expr):
/// identical to `__rtsadp_fn_reify` but marks `has_this_param` - the
/// method-aware invoker binds `this` into the thunk a0; the plain invoker
/// shifts positional args past an `undefined` this.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_fn_reify_this(
    addr: u64,
    nparams: u64,
    has_rest: u64,
    env_word: u64,
) -> u64 {
    fn_reify_core(addr, nparams, has_rest, env_word, true)
}

fn fn_reify_core(addr: u64, nparams: u64, has_rest: u64, env_word: u64, has_this: bool) -> u64 {
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
        has_this_param: has_this,
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
        // The NEW-engine reify: `fn_ptr` is the uniform 6-slot thunk. INVOKE_AUTO
        // (the legacy invoker the timer/microtask pumps use) dispatches on this.
        uniform_thunk: true,
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
/// JS `TypeError: … is not a function` — calling a NON-function value. Records
/// the pending error (a STRING word for now; a real `TypeError` instance is a
/// later increment) and returns `undefined` — the caller's post-call
/// `err_pending` check unwinds past the sentinel, so the wrong value is never
/// observed (matching JS, which throws instead of yielding `undefined`).
fn throw_not_a_function() -> u64 {
    let msg = super::abi_adapter::intern_poly("TypeError: not a function");
    super::errslot::__rtsadp_throw_set(msg.raw());
    PolyValue::undefined().raw()
}

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
        // PROXY over a callable (#218): route the call through the `apply` trap —
        // `handler.apply(target, thisArg, argsList)`. The uniform 5-slot invoke
        // does not carry an arg COUNT, so `argsList` rides as `undefined` (traps
        // that ignore it — the observed pattern — work; reading it is a later
        // increment). No trap → forward the call to the target verbatim.
        if let Some((target, handler)) = super::objops::proxy_parts(fn_word) {
            let apply_key = super::abi_adapter::intern_poly("apply").raw();
            let trap = super::objops::__rtsadp_obj_get(handler, apply_key);
            let undef = PolyValue::undefined().raw();
            if PolyValue::from_raw(trap).is_function() {
                return __rtsadp_fn_invoke(trap, target, undef, undef, undef, 0);
            }
            return __rtsadp_fn_invoke(target, a0, a1, a2, a3, rest);
        }
        return throw_not_a_function();
    }
    // Reconstruct the full real handle (generation read from the live slot) from
    // the bare 48-bit payload, then read the stored thunk address AND env word.
    let real = rts_runtime::namespaces::gc::handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(pv.as_handle());
    let (addr, env, has_this, rest_idx, uniform, arity, bound) = with_entry(real, |e| match e {
        Some(Entry::Function(d)) => (
            d.fn_ptr,
            d.bound_this as u64,
            d.has_this_param,
            d.rest_param_idx,
            d.uniform_thunk,
            d.arity,
            d.bound_args.clone(),
        ),
        _ => (0, 0, false, -1, true, 0, Vec::new()),
    });
    if addr == 0 {
        return PolyValue::undefined().raw();
    }
    // A LEGACY-convention callee (`new Function` compiles a raw all-i64 extern —
    // `uniform_thunk: false`): unbox the arg words to raw i64s and route through
    // the runtime's own typed invoker (`FUNCTION_CALL`, which also applies bound
    // args), then box the raw i64 result back as a number word.
    if !uniform {
        return invoke_legacy_fn(real, arity, bound.len(), [a0, a1, a2, a3]);
    }
    // BOUND partial-application args on a UNIFORM callee (`f.bind(null, x)`):
    // prepend the stored WORDS — the call-site args shift right, and an
    // INT32-argc `rest` word grows by the bound count (args past slot 4 drop —
    // the ≤4-slot subset).
    let (a0, a1, a2, a3, rest) = if bound.is_empty() {
        (a0, a1, a2, a3, rest)
    } else {
        let mut all: Vec<u64> = bound.iter().map(|&w| w as u64).collect();
        all.extend([a0, a1, a2, a3]);
        let undef = PolyValue::undefined().raw();
        let get = |i: usize| all.get(i).copied().unwrap_or(undef);
        let rest_pv = PolyValue::from_raw(rest);
        let new_rest = if rest_pv.is_int32() {
            PolyValue::from_i32(rest_pv.as_i32().saturating_add(bound.len() as i32)).raw()
        } else {
            rest
        };
        (get(0), get(1), get(2), get(3), new_rest)
    };
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
    // A VARIADIC callee: the THUNK is the single packer (`__rtsadp_pack_rest`
    // packs the positional tail + overflow at the rest index, capture-aware).
    // Packing HERE TOO double-wrapped the tail (`(...args)` read `[[2,3]]`).
    if has_this && bound.is_empty() {
        // A this-first function called WITHOUT a receiver (plain `f(x)`): JS
        // binds `this = undefined`; positional args shift past it (a3 drops -
        // a 4-arg this-method through the plain invoker is out of this subset).
        // A BOUND this-first fn (`f.bind(obj)` prepended the this word into
        // `bound_args[0]`) already carries `this` in a0 — no shift.
        return f(env_word, PolyValue::undefined().raw(), a0, a1, a2, rest);
    }
    f(env_word, a0, a1, a2, a3, rest)
}

/// Invoke a LEGACY-convention function handle (a `new Function` compile: raw
/// all-i64 `extern "C"`, `uniform_thunk: false`) with up to 4 PolyValue-word
/// args: unbox each to its raw i64, route through the runtime's own
/// `FUNCTION_CALL` (which applies bound args / kinds), box the i64 result back
/// as the tightest number word.
fn invoke_legacy_fn(real_handle: u64, arity: u8, bound_len: usize, slots: [u64; 4]) -> u64 {
    use rts_engine::heap::handles::{Entry as E, alloc_entry};
    // `FUNCTION_CALL` prepends the stored bound args itself — pass only the
    // REMAINING positional count so the total matches the callee's arity.
    let n = (arity as usize).saturating_sub(bound_len).min(4);
    let raw: Vec<i64> = slots[..n].iter().map(|&w| word_to_raw_i64(w)).collect();
    let args_h = alloc_entry(E::Vec(Box::new(raw)));
    let r = rts_runtime::namespaces::globals::function::ops::__RTS_FN_GL_FUNCTION_CALL(
        real_handle,
        0,
        args_h,
    );
    if let Ok(i) = i32::try_from(r) {
        PolyValue::from_i32(i).raw()
    } else {
        PolyValue::from_f64(r as f64).raw()
    }
}

/// A PolyValue word → the raw i64 a legacy all-i64 callee reads: int32 → its
/// value, inline double → truncated, bool → 0/1, anything else → 0.
fn word_to_raw_i64(w: u64) -> i64 {
    let v = PolyValue::from_raw(w);
    if v.is_int32() {
        v.as_i32() as i64
    } else if v.is_double() {
        v.as_f64() as i64
    } else if v.is_bool() {
        v.as_bool() as i64
    } else if let Some(h) = rts_engine::heap::poly::poly_handle_normalize(w) {
        // A heap value (string/object/function word) crosses the raw-i64 ABI
        // as its REAL handle (the legacy surface convention).
        h as i64
    } else {
        0
    }
}

/// `__rtsadp_fn_bind(fn_word, args_vec)` — `f.bind(thisArg, …partial)` with
/// PARTIAL-APPLICATION args: clone the function value with the extra args
/// appended to `bound_args` (`.length` reads `arity - bound_args.len()`, and
/// both invoke paths prepend them). A UNIFORM-thunk callee stores the raw
/// PolyValue WORDS (its invoke shifts them into `a0…`); a legacy callee
/// (`new Function`) stores raw i64s (its `invoke_typed` path reads them
/// verbatim). Non-function → `undefined`.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_fn_bind(fn_word: u64, this_word: u64, args_vec: u64) -> u64 {
    use rts_engine::heap::handles::{Entry as E, FunctionData, alloc_entry};
    let pv = PolyValue::from_raw(fn_word);
    if !pv.is_function() {
        return PolyValue::undefined().raw();
    }
    let real = rts_runtime::namespaces::gc::handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(pv.as_handle());
    let items: Vec<i64> = with_entry(args_vec, |e| match e {
        Some(E::Vec(v)) => v.as_ref().clone(),
        _ => Vec::new(),
    });
    let cloned: Option<Box<FunctionData>> = with_entry(real, |e| match e {
        Some(E::Function(d)) => {
            let extra: Vec<i64> = if d.uniform_thunk {
                items.clone()
            } else {
                items.iter().map(|&w| word_to_raw_i64(w as u64)).collect()
            };
            let mut bound = d.bound_args.clone();
            // JS `f.bind(thisArg)` on a THIS-FIRST fn (`function m(this,…)`):
            // the receiver rides `bound_args[0]` (the thunk's this-first real
            // param slot); the plain invoker then skips its this=undefined
            // shift. A REBIND (`bound.bind(other)`) does NOT replace it — JS
            // keeps the first bound this. A plain fn ignores thisArg (no this
            // slot in the uniform ABI).
            if d.has_this_param && d.uniform_thunk && bound.is_empty() {
                bound.push(this_word as i64);
            }
            bound.extend(extra);
            Some(Box::new(FunctionData {
                fn_ptr: d.fn_ptr,
                arity: d.arity,
                name: d.name.clone(),
                bound_this: d.bound_this,
                has_bound_this: d.has_bound_this,
                bound_args: bound,
                is_arrow: d.is_arrow,
                has_this_param: d.has_this_param,
                param_kinds: d.param_kinds.clone(),
                return_kind: d.return_kind,
                packed_shim: d.packed_shim,
                source: d.source.clone(),
                keep_alive: d.keep_alive.clone(),
                prototype_handle: 0,
                rest_param_idx: d.rest_param_idx,
                uniform_thunk: d.uniform_thunk,
            }))
        }
        _ => None,
    });
    let Some(data) = cloned else {
        return PolyValue::undefined().raw();
    };
    let h = alloc_entry(E::Function(data));
    PolyValue::from_function_handle(
        rts_runtime::namespaces::gc::handles::__RTS_FN_NS_GC_POLY_FROM_HANDLE(h),
    )
    .raw()
}

/// `__rtsadp_fn_new(args_vec)` — the PRIMORDIAL `new Function(p0…pN-1, body)`:
/// ToString every arg word, join the leading N-1 as the params CSV, hand
/// `(params_handle, body_handle)` to the runtime ctor (which compiles the body
/// through the engine-installed COMPILE_FN_HOOK — see `front::run::dynfn`), and
/// box the resulting `Entry::Function` handle as a `TAG_FUNCTION` word. A
/// failed compile yields `undefined` (the runtime already eprintln'd why).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_fn_new(args_vec: u64) -> u64 {
    use rts_engine::heap::handles::Entry as E;
    let items: Vec<i64> = with_entry(args_vec, |e| match e {
        Some(E::Vec(v)) => v.as_ref().clone(),
        _ => Vec::new(),
    });
    let text = |w: i64| super::genops::to_string(PolyValue::from_raw(w as u64));
    let (params, body) = match items.split_last() {
        Some((last, init)) => (
            init.iter().map(|&w| text(w)).collect::<Vec<_>>().join(","),
            text(*last),
        ),
        None => (String::new(), String::new()),
    };
    let intern = |s: &str| {
        rts_runtime::namespaces::collector::string_pool::__RTS_FN_NS_GC_STRING_NEW(
            s.as_ptr(),
            s.len() as i64,
        )
    };
    let h = rts_runtime::namespaces::globals::function::ops::__RTS_FN_GL_FUNCTION_NEW(
        intern(&params),
        intern(&body),
    );
    if h == 0 {
        return PolyValue::undefined().raw();
    }
    PolyValue::from_function_handle(
        rts_runtime::namespaces::gc::handles::__RTS_FN_NS_GC_POLY_FROM_HANDLE(h),
    )
    .raw()
}

/// METHOD-AWARE invoke: `recv.m(args)` where `m` resolved to a FUNCTION word.
/// A `has_this_param` callee receives `this_word` in the thunk a0 slot (its
/// this-first real param); a plain function ignores the receiver (JS: `this`
/// unused) and gets the args unshifted.
/// `f(...arr)` — spread-invoke a function VALUE with a runtime ARRAY of args:
/// unpack `a0..a3`, overflow rides the rest ARRAY (>4) or the argc INT32 word
/// (≤4), exactly the compile-time call convention. Non-array `arr` → 0 args.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_fn_apply_arr(fn_word: u64, arr_word: u64) -> u64 {
    use rts_runtime::namespaces::collections::vec as rt_vec;
    use rts_runtime::namespaces::gc::handles as rt_handles;
    let undef = PolyValue::undefined().raw();
    let av = PolyValue::from_raw(arr_word);
    let (h, len) = if av.is_object() {
        let h = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(av.as_handle());
        (h, rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_LEN(h).max(0))
    } else {
        (0, 0)
    };
    let elem = |i: i64| -> u64 {
        if i < len {
            rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_GET(h, i) as u64
        } else {
            undef
        }
    };
    let rest = if len > 4 {
        let out = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW();
        for i in 4..len {
            rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(out, elem(i) as i64);
        }
        PolyValue::from_object_handle(rt_handles::__RTS_FN_NS_GC_POLY_FROM_HANDLE(out)).raw()
    } else {
        PolyValue::from_i32(len as i32).raw()
    };
    __rtsadp_fn_invoke(fn_word, elem(0), elem(1), elem(2), elem(3), rest)
}

/// `f.apply(thisArg, arr)` — the FULL runtime form: a NULLISH `thisArg` routes
/// the plain spread-invoke (exact argc/rest for any length — `Math.max` as a
/// value included); a real receiver routes the METHOD-aware invoke with
/// `a0..a2` + the overflow/argc rest word.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_fn_apply_this(fn_word: u64, this_word: u64, arr_word: u64) -> u64 {
    use rts_runtime::namespaces::collections::vec as rt_vec;
    use rts_runtime::namespaces::gc::handles as rt_handles;
    let tv = PolyValue::from_raw(this_word);
    if tv.is_undefined() || tv.is_null() {
        return __rtsadp_fn_apply_arr(fn_word, arr_word);
    }
    let undef = PolyValue::undefined().raw();
    let av = PolyValue::from_raw(arr_word);
    let (h, len) = if av.is_object() {
        let h = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(av.as_handle());
        (h, rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_LEN(h).max(0))
    } else {
        (0, 0)
    };
    let elem = |i: i64| -> u64 {
        if i < len {
            rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_GET(h, i) as u64
        } else {
            undef
        }
    };
    let rest = if len > 3 {
        let out = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW();
        for i in 3..len {
            rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(out, elem(i) as i64);
        }
        PolyValue::from_object_handle(rt_handles::__RTS_FN_NS_GC_POLY_FROM_HANDLE(out)).raw()
    } else {
        PolyValue::from_i32(len as i32).raw()
    };
    __rtsadp_fn_invoke_method(fn_word, this_word, elem(0), elem(1), elem(2), rest)
}

#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_fn_invoke_method(
    fn_word: u64,
    this_word: u64,
    a0: u64,
    a1: u64,
    a2: u64,
    rest: u64,
) -> u64 {
    let pv = PolyValue::from_raw(fn_word);
    if !pv.is_function() {
        // PROXY over a callable (#218): the `apply` trap (or forward to the
        // target) — mirror of the plain invoker's proxy branch, keeping the
        // receiver (`f.apply(thisArg, arr)` on a proxied fn).
        if let Some((target, handler)) = super::objops::proxy_parts(fn_word) {
            let apply_key = super::abi_adapter::intern_poly("apply").raw();
            let trap = super::objops::__rtsadp_obj_get(handler, apply_key);
            if PolyValue::from_raw(trap).is_function() {
                let undef = PolyValue::undefined().raw();
                return __rtsadp_fn_invoke(trap, target, this_word, undef, undef, 0);
            }
            return __rtsadp_fn_invoke_method(target, this_word, a0, a1, a2, rest);
        }
        // `recv.m()` where `m` resolved to a NON-function — JS TypeError.
        return throw_not_a_function();
    }
    let real = rts_runtime::namespaces::gc::handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(pv.as_handle());
    let (addr, env, has_this, uniform, arity, bound) = with_entry(real, |e| match e {
        Some(Entry::Function(d)) => (
            d.fn_ptr,
            d.bound_this as u64,
            d.has_this_param,
            d.uniform_thunk,
            d.arity,
            d.bound_args.clone(),
        ),
        _ => (0, 0, false, true, 0, Vec::new()),
    });
    if addr == 0 {
        return PolyValue::undefined().raw();
    }
    // LEGACY all-i64 callee (`new Function` / a raw-ptr synth fn): a THIS-FIRST
    // callee gets the receiver as its real first param (a literal setter/method
    // reified without the uniform thunk); a plain one keeps the no-this route.
    if !uniform {
        if has_this {
            return invoke_legacy_fn(real, arity, bound.len(), [this_word, a0, a1, a2]);
        }
        return invoke_legacy_fn(real, arity, bound.len(), [a0, a1, a2, PolyValue::undefined().raw()]);
    }
    let env_word = if env == 0 {
        PolyValue::undefined().raw()
    } else {
        env
    };
    let f: UniformFn = unsafe { std::mem::transmute::<u64, UniformFn>(addr) };
    // BOUND args prepend (a `.bind` clone): a bound THIS-FIRST fn already has
    // its receiver in bound[0] — the call-site `this_word` is ignored (JS:
    // `.call` on a bound fn cannot re-bind this).
    if !bound.is_empty() {
        let undef = PolyValue::undefined().raw();
        let mut all: Vec<u64> = bound.iter().map(|&w| w as u64).collect();
        for w in [a0, a1, a2] {
            all.push(w);
        }
        let g = |i: usize| all.get(i).copied().unwrap_or(undef);
        return f(env_word, g(0), g(1), g(2), g(3), rest);
    }
    if has_this {
        return f(env_word, this_word, a0, a1, a2, rest);
    }
    f(env_word, a0, a1, a2, PolyValue::undefined().raw(), rest)
}
