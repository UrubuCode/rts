//! `parallel` namespace — Rayon-backed data parallelism (silent-parallelism
//! passes rewrite `arr.map/forEach/reduce/...` into these calls).
//!
//! The `*_BOUND` / `REDUCE_RIGHT_*` / `FIND_LAST_*` externs are NOT namespace
//! members — codegen calls them directly for callbacks that capture locals
//! (#195, Entry::Function with bound_args) — so they stay free externs below.
//!
//! Migrado do `#[rts_namespace]` pro modelo builder hand-written do `rts-engine`
//! (rumo à remoção da `rts-macro`; ver pilotos hint/hash/ptr/mem/runtime).

pub mod pool;

use rayon::prelude::*;

use rts_engine::abi::ty::{Bool, Handle, I64, U64};
use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

use self::pool::pool;
use rts_engine::heap::handles::{alloc_entry, with_entry, Entry};
use rts_shared::globals::function::ops::invoke_array_callback;

fn snapshot_vec(handle: u64) -> Option<Vec<i64>> {
    with_entry(handle, |entry| match entry {
        Some(Entry::Vec(v)) => Some(v.as_ref().clone()),
        _ => None,
    })
}

/// JS-spec truthy for a callback return (0/null/undefined/""/false are falsy).
fn cb_truthy(v: i64) -> bool {
    crate::gc_surface::__RTS_FN_RT_TRUTHY(v) != 0
}

/// Applies `fn_ptr(x, i, arr)` in parallel over the Vec<i64>. New Vec handle.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PARALLEL_MAP(vec_handle: Handle, fn_ptr: U64) -> Handle {
    let Some(items) = snapshot_vec(vec_handle) else {
        return 0;
    };
    if fn_ptr == 0 {
        return 0;
    }
    // SAFETY: codegen contract — `extern "C" fn(i64, i64, i64) -> i64`.
    let f: extern "C" fn(i64, i64, i64) -> i64 =
        unsafe { std::mem::transmute(fn_ptr as usize) };
    let arr = vec_handle as i64;
    let map_hole = |i: usize, x: i64| {
        if x == i64::MIN + 4 {
            x
        } else {
            f(x, i as i64, arr)
        }
    };
    // (#365) rayon workers aren't GC-registered; inside a tokio worker run
    // sequentially on the (registered) calling thread.
    let result: Vec<i64> = if crate::runtime::async_rt::in_async_worker()
        || tokio::runtime::Handle::try_current().is_ok()
    {
        items
            .iter()
            .enumerate()
            .map(|(i, &x)| map_hole(i, x))
            .collect()
    } else {
        pool().install(|| {
            items
                .par_iter()
                .enumerate()
                .map(|(i, &x)| map_hole(i, x))
                .collect()
        })
    };
    alloc_entry(Entry::Vec(Box::new(result)))
}

/// Runs `fn_ptr(x, i, arr)` for each element. Delegates to Map.forEach for maps.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PARALLEL_FOR_EACH(vec_handle: Handle, fn_ptr: U64) {
    if fn_ptr == 0 {
        return;
    }
    let is_map_or_set = with_entry(vec_handle, |e| matches!(e, Some(Entry::Map(_))));
    if is_map_or_set {
        rts_shared::collections::map::__RTS_FN_NS_COLLECTIONS_MAP_FOR_EACH(
            vec_handle, fn_ptr,
        );
        return;
    }
    let Some(items) = snapshot_vec(vec_handle) else {
        return;
    };
    let arr = vec_handle as i64;
    items.iter().enumerate().for_each(|(i, &x)| {
        if x != i64::MIN + 4 {
            invoke_array_callback(fn_ptr, &[x, i as i64, arr]);
        }
    });
}

/// `reduce(fn)` without an initial value (items[0] seeds the accumulator).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PARALLEL_REDUCE_NO_INIT(vec_handle: Handle, fn_ptr: U64) -> I64 {
    let Some(items) = snapshot_vec(vec_handle) else {
        return 0;
    };
    if fn_ptr == 0 || items.is_empty() {
        return 0;
    }
    let f: extern "C" fn(i64, i64, i64) -> i64 =
        unsafe { std::mem::transmute(fn_ptr as usize) };
    let mut acc = items[0];
    for (i, &x) in items.iter().enumerate().skip(1) {
        acc = f(acc, x, i as i64);
    }
    acc
}

/// `reduce(fn, identity)` (sequential — passes route any reduce here).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PARALLEL_REDUCE(vec_handle: Handle, identity: I64, fn_ptr: U64) -> I64 {
    let Some(items) = snapshot_vec(vec_handle) else {
        return identity;
    };
    if fn_ptr == 0 {
        return identity;
    }
    let f: extern "C" fn(i64, i64, i64) -> i64 =
        unsafe { std::mem::transmute(fn_ptr as usize) };
    let mut acc = identity;
    for (i, &x) in items.iter().enumerate() {
        acc = f(acc, x, i as i64);
    }
    acc
}

/// Number of threads in the global Rayon pool.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PARALLEL_NUM_THREADS() -> I64 {
    pool().current_num_threads() as i64
}

/// Vec of elements where `fn_ptr(x, i, arr)` is truthy. New Vec handle.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PARALLEL_FILTER(vec_handle: Handle, fn_ptr: U64) -> Handle {
    let Some(items) = snapshot_vec(vec_handle) else {
        return 0;
    };
    if fn_ptr == 0 {
        return 0;
    }
    let f: extern "C" fn(i64, i64, i64) -> i64 =
        unsafe { std::mem::transmute(fn_ptr as usize) };
    let arr = vec_handle as i64;
    let result: Vec<i64> = items
        .into_iter()
        .enumerate()
        .filter(|&(i, x)| x != i64::MIN + 4 && cb_truthy(f(x, i as i64, arr)))
        .map(|(_, x)| x)
        .collect();
    alloc_entry(Entry::Vec(Box::new(result)))
}

/// First element satisfying the predicate, or the `undefined` sentinel.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PARALLEL_FIND(vec_handle: Handle, fn_ptr: U64) -> I64 {
    let Some(items) = snapshot_vec(vec_handle) else {
        return 0;
    };
    if fn_ptr == 0 {
        return 0;
    }
    let f: extern "C" fn(i64, i64, i64) -> i64 =
        unsafe { std::mem::transmute(fn_ptr as usize) };
    let arr = vec_handle as i64;
    match items
        .into_iter()
        .enumerate()
        .find(|&(i, x)| cb_truthy(f(x, i as i64, arr)))
    {
        Some((_, v)) => v,
        None => i64::MIN + 2,
    }
}

/// Index of the first element satisfying the predicate, or -1.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PARALLEL_FIND_INDEX(vec_handle: Handle, fn_ptr: U64) -> I64 {
    let Some(items) = snapshot_vec(vec_handle) else {
        return -1;
    };
    if fn_ptr == 0 {
        return -1;
    }
    let f: extern "C" fn(i64, i64, i64) -> i64 =
        unsafe { std::mem::transmute(fn_ptr as usize) };
    let arr = vec_handle as i64;
    items
        .into_iter()
        .enumerate()
        .position(|(i, x)| cb_truthy(f(x, i as i64, arr)))
        .map(|i| i as i64)
        .unwrap_or(-1)
}

/// True if any element satisfies the predicate.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PARALLEL_SOME(vec_handle: Handle, fn_ptr: U64) -> Bool {
    let Some(items) = snapshot_vec(vec_handle) else {
        return 0;
    };
    if fn_ptr == 0 {
        return 0;
    }
    let f: extern "C" fn(i64, i64, i64) -> i64 =
        unsafe { std::mem::transmute(fn_ptr as usize) };
    let arr = vec_handle as i64;
    if items
        .into_iter()
        .enumerate()
        .any(|(i, x)| cb_truthy(f(x, i as i64, arr)))
    {
        1
    } else {
        0
    }
}

/// True if every element satisfies the predicate (vacuously true if empty).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PARALLEL_EVERY(vec_handle: Handle, fn_ptr: U64) -> Bool {
    let Some(items) = snapshot_vec(vec_handle) else {
        return 1;
    };
    if fn_ptr == 0 {
        return 1;
    }
    let f: extern "C" fn(i64, i64, i64) -> i64 =
        unsafe { std::mem::transmute(fn_ptr as usize) };
    let arr = vec_handle as i64;
    if items
        .into_iter()
        .enumerate()
        .all(|(i, x)| cb_truthy(f(x, i as i64, arr)))
    {
        1
    } else {
        0
    }
}

/// Função `parallel.f(args)` — helper de construção de membro.
#[allow(clippy::too_many_arguments)]
fn func(
    name: &str,
    symbol: &str,
    sig: Sig,
    ts: &str,
    doc: &str,
    fp: *const u8,
    flags: MemberFlags,
) -> Member {
    Member {
        name: name.to_string(),
        kind: MemberKind::Function,
        sig,
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp),
        flags,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: doc.to_string(),
        pure: false,
        intrinsic: None,
    }
}

/// Registra a namespace `parallel` no motor (Fase 2 — hand-written, sem macro).
pub fn register(e: &mut Engine) {
    e.ns("parallel")
        .doc("Rayon-backed data parallelism (map/for_each/reduce + predicates over Vec<i64>).")
        .member(func(
            "map",
            "__RTS_FN_NS_PARALLEL_MAP",
            Sig::new(vec![AbiType::Handle, AbiType::U64], AbiType::Handle),
            "map(vec_handle: number, fn_ptr: number): number",
            "Applies `fn_ptr(x, i, arr)` in parallel over the Vec<i64>. New Vec handle.",
            __RTS_FN_NS_PARALLEL_MAP as *const u8,
            MemberFlags::NONE,
        ))
        .member(func(
            "for_each",
            "__RTS_FN_NS_PARALLEL_FOR_EACH",
            Sig::new(vec![AbiType::Handle, AbiType::U64], AbiType::Void),
            "for_each(vec_handle: number, fn_ptr: number): void",
            "Runs `fn_ptr(x, i, arr)` for each element. Delegates to Map.forEach for maps.",
            __RTS_FN_NS_PARALLEL_FOR_EACH as *const u8,
            MemberFlags::UNDEF_RET,
        ))
        .member(func(
            "reduce_no_init",
            "__RTS_FN_NS_PARALLEL_REDUCE_NO_INIT",
            Sig::new(vec![AbiType::Handle, AbiType::U64], AbiType::I64),
            "reduce_no_init(vec_handle: number, fn_ptr: number): number",
            "`reduce(fn)` without an initial value (items[0] seeds the accumulator).",
            __RTS_FN_NS_PARALLEL_REDUCE_NO_INIT as *const u8,
            MemberFlags::AMBIGUOUS_RET,
        ))
        .member(func(
            "reduce",
            "__RTS_FN_NS_PARALLEL_REDUCE",
            Sig::new(vec![AbiType::Handle, AbiType::I64, AbiType::U64], AbiType::I64),
            "reduce(vec_handle: number, identity: number, fn_ptr: number): number",
            "`reduce(fn, identity)` (sequential — passes route any reduce here).",
            __RTS_FN_NS_PARALLEL_REDUCE as *const u8,
            MemberFlags::AMBIGUOUS_RET,
        ))
        .member(func(
            "num_threads",
            "__RTS_FN_NS_PARALLEL_NUM_THREADS",
            Sig::new(Vec::new(), AbiType::I64),
            "num_threads(): number",
            "Number of threads in the global Rayon pool.",
            __RTS_FN_NS_PARALLEL_NUM_THREADS as *const u8,
            MemberFlags::NONE,
        ))
        .member(func(
            "filter",
            "__RTS_FN_NS_PARALLEL_FILTER",
            Sig::new(vec![AbiType::Handle, AbiType::U64], AbiType::Handle),
            "filter(vec_handle: number, fn_ptr: number): number",
            "Vec of elements where `fn_ptr(x, i, arr)` is truthy. New Vec handle.",
            __RTS_FN_NS_PARALLEL_FILTER as *const u8,
            MemberFlags::NONE,
        ))
        .member(func(
            "find",
            "__RTS_FN_NS_PARALLEL_FIND",
            Sig::new(vec![AbiType::Handle, AbiType::U64], AbiType::I64),
            "find(vec_handle: number, fn_ptr: number): number",
            "First element satisfying the predicate, or the `undefined` sentinel.",
            __RTS_FN_NS_PARALLEL_FIND as *const u8,
            MemberFlags::AMBIGUOUS_RET,
        ))
        .member(func(
            "find_index",
            "__RTS_FN_NS_PARALLEL_FIND_INDEX",
            Sig::new(vec![AbiType::Handle, AbiType::U64], AbiType::I64),
            "find_index(vec_handle: number, fn_ptr: number): number",
            "Index of the first element satisfying the predicate, or -1.",
            __RTS_FN_NS_PARALLEL_FIND_INDEX as *const u8,
            MemberFlags::NONE,
        ))
        .member(func(
            "some",
            "__RTS_FN_NS_PARALLEL_SOME",
            Sig::new(vec![AbiType::Handle, AbiType::U64], AbiType::Bool),
            "some(vec_handle: number, fn_ptr: number): boolean",
            "True if any element satisfies the predicate.",
            __RTS_FN_NS_PARALLEL_SOME as *const u8,
            MemberFlags::NONE,
        ))
        .member(func(
            "every",
            "__RTS_FN_NS_PARALLEL_EVERY",
            Sig::new(vec![AbiType::Handle, AbiType::U64], AbiType::Bool),
            "every(vec_handle: number, fn_ptr: number): boolean",
            "True if every element satisfies the predicate (vacuously true if empty).",
            __RTS_FN_NS_PARALLEL_EVERY as *const u8,
            MemberFlags::NONE,
        ))
        .done();
}

// ── Non-member externs (#195): BOUND callbacks carry per-activation captures
// via Entry::Function; codegen calls these by symbol. ─────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PARALLEL_MAP_BOUND(vec_handle: u64, fn_handle: u64) -> u64 {
    let Some(items) = snapshot_vec(vec_handle) else {
        return 0;
    };
    if fn_handle == 0 {
        return 0;
    }
    let result: Vec<i64> = items
        .iter()
        .enumerate()
        .map(|(i, &x)| {
            if x == i64::MIN + 4 {
                x
            } else {
                invoke_array_callback(fn_handle, &[x, i as i64])
            }
        })
        .collect();
    alloc_entry(Entry::Vec(Box::new(result)))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PARALLEL_FILTER_BOUND(vec_handle: u64, fn_handle: u64) -> u64 {
    let Some(items) = snapshot_vec(vec_handle) else {
        return 0;
    };
    if fn_handle == 0 {
        return 0;
    }
    let result: Vec<i64> = items
        .into_iter()
        .enumerate()
        .filter(|&(i, x)| {
            x != i64::MIN + 4 && cb_truthy(invoke_array_callback(fn_handle, &[x, i as i64]))
        })
        .map(|(_, x)| x)
        .collect();
    alloc_entry(Entry::Vec(Box::new(result)))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PARALLEL_FOR_EACH_BOUND(vec_handle: u64, fn_handle: u64) {
    if fn_handle == 0 {
        return;
    }
    let Some(items) = snapshot_vec(vec_handle) else {
        return;
    };
    items.iter().enumerate().for_each(|(i, &x)| {
        if x != i64::MIN + 4 {
            invoke_array_callback(fn_handle, &[x, i as i64]);
        }
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PARALLEL_REDUCE_BOUND(
    vec_handle: u64,
    identity: i64,
    fn_handle: u64,
) -> i64 {
    let Some(items) = snapshot_vec(vec_handle) else {
        return identity;
    };
    if fn_handle == 0 {
        return identity;
    }
    let mut acc = identity;
    for (i, &x) in items.iter().enumerate() {
        acc = invoke_array_callback(fn_handle, &[acc, x, i as i64]);
    }
    acc
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PARALLEL_REDUCE_NO_INIT_BOUND(
    vec_handle: u64,
    fn_handle: u64,
) -> i64 {
    let Some(items) = snapshot_vec(vec_handle) else {
        return 0;
    };
    if fn_handle == 0 || items.is_empty() {
        return 0;
    }
    let mut acc = items[0];
    for (i, &x) in items.iter().enumerate().skip(1) {
        acc = invoke_array_callback(fn_handle, &[acc, x, i as i64]);
    }
    acc
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PARALLEL_FIND_BOUND(vec_handle: u64, fn_handle: u64) -> i64 {
    let Some(items) = snapshot_vec(vec_handle) else {
        return 0;
    };
    if fn_handle == 0 {
        return 0;
    }
    match items
        .into_iter()
        .enumerate()
        .find(|&(i, x)| cb_truthy(invoke_array_callback(fn_handle, &[x, i as i64])))
    {
        Some((_, v)) => v,
        None => i64::MIN + 2,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PARALLEL_FIND_INDEX_BOUND(vec_handle: u64, fn_handle: u64) -> i64 {
    let Some(items) = snapshot_vec(vec_handle) else {
        return -1;
    };
    if fn_handle == 0 {
        return -1;
    }
    items
        .into_iter()
        .enumerate()
        .position(|(i, x)| cb_truthy(invoke_array_callback(fn_handle, &[x, i as i64])))
        .map(|i| i as i64)
        .unwrap_or(-1)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PARALLEL_SOME_BOUND(vec_handle: u64, fn_handle: u64) -> i64 {
    let Some(items) = snapshot_vec(vec_handle) else {
        return 0;
    };
    if fn_handle == 0 {
        return 0;
    }
    if items
        .into_iter()
        .enumerate()
        .any(|(i, x)| cb_truthy(invoke_array_callback(fn_handle, &[x, i as i64])))
    {
        1
    } else {
        0
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PARALLEL_EVERY_BOUND(vec_handle: u64, fn_handle: u64) -> i64 {
    let Some(items) = snapshot_vec(vec_handle) else {
        return 1;
    };
    if fn_handle == 0 {
        return 1;
    }
    if items
        .into_iter()
        .enumerate()
        .all(|(i, x)| cb_truthy(invoke_array_callback(fn_handle, &[x, i as i64])))
    {
        1
    } else {
        0
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PARALLEL_REDUCE_RIGHT_BOUND(
    vec_handle: u64,
    identity: i64,
    fn_handle: u64,
) -> i64 {
    let Some(items) = snapshot_vec(vec_handle) else {
        return identity;
    };
    if fn_handle == 0 {
        return identity;
    }
    let mut acc = identity;
    for (i, &x) in items.iter().enumerate().rev() {
        acc = invoke_array_callback(fn_handle, &[acc, x, i as i64]);
    }
    acc
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PARALLEL_REDUCE_RIGHT_NO_INIT_BOUND(
    vec_handle: u64,
    fn_handle: u64,
) -> i64 {
    let Some(items) = snapshot_vec(vec_handle) else {
        return 0;
    };
    if fn_handle == 0 || items.is_empty() {
        return 0;
    }
    let mut acc = *items.last().unwrap();
    for (i, &x) in items.iter().enumerate().rev().skip(1) {
        acc = invoke_array_callback(fn_handle, &[acc, x, i as i64]);
    }
    acc
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PARALLEL_FIND_LAST_BOUND(vec_handle: u64, fn_handle: u64) -> i64 {
    let Some(items) = snapshot_vec(vec_handle) else {
        return 0;
    };
    if fn_handle == 0 {
        return 0;
    }
    match items
        .iter()
        .enumerate()
        .rev()
        .find(|&(i, &x)| cb_truthy(invoke_array_callback(fn_handle, &[x, i as i64])))
    {
        Some((_, &v)) => v,
        None => i64::MIN + 2,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PARALLEL_FIND_LAST_INDEX_BOUND(
    vec_handle: u64,
    fn_handle: u64,
) -> i64 {
    let Some(items) = snapshot_vec(vec_handle) else {
        return -1;
    };
    if fn_handle == 0 {
        return -1;
    }
    for (i, &x) in items.iter().enumerate().rev() {
        if cb_truthy(invoke_array_callback(fn_handle, &[x, i as i64])) {
            return i as i64;
        }
    }
    -1
}
