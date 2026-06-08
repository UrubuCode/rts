//! `parallel` namespace — Rayon-backed data parallelism (silent-parallelism
//! passes rewrite `arr.map/forEach/reduce/...` into these calls).
//!
//! The `*_BOUND` / `REDUCE_RIGHT_*` / `FIND_LAST_*` externs are NOT namespace
//! members — codegen calls them directly for callbacks that capture locals
//! (#195, Entry::Function with bound_args) — so they stay free externs below.
//!
//! Migrated to the `#[rts_namespace]` single-declaration model (stage 2c,
//! `docs/specs/rts-core-engine.md`).

pub mod pool;

use rayon::prelude::*;

use rts_abi::ty::{Bool, Handle, I64, U64};
use rts_macro::rts_namespace;

use self::pool::pool;
use crate::namespaces::gc::handles::{alloc_entry, with_entry, Entry};
use crate::namespaces::globals::function::ops::invoke_array_callback;

fn snapshot_vec(handle: u64) -> Option<Vec<i64>> {
    with_entry(handle, |entry| match entry {
        Some(Entry::Vec(v)) => Some(v.as_ref().clone()),
        _ => None,
    })
}

/// JS-spec truthy for a callback return (0/null/undefined/""/false are falsy).
fn cb_truthy(v: i64) -> bool {
    crate::namespaces::gc::string_pool::__RTS_FN_RT_TRUTHY(v) != 0
}

/// Rayon-backed data parallelism (map/for_each/reduce + predicates over Vec<i64>).
#[rts_namespace(parallel)]
impl ParallelNs {
    /// Applies `fn_ptr(x, i, arr)` in parallel over the Vec<i64>. New Vec handle.
    #[rts_fn]
    pub fn map(vec_handle: Handle, fn_ptr: U64) -> Handle {
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
    #[rts_fn]
    pub fn for_each(vec_handle: Handle, fn_ptr: U64) {
        if fn_ptr == 0 {
            return;
        }
        let is_map_or_set = with_entry(vec_handle, |e| matches!(e, Some(Entry::Map(_))));
        if is_map_or_set {
            crate::namespaces::collections::map::__RTS_FN_NS_COLLECTIONS_MAP_FOR_EACH(
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
    #[rts_fn]
    pub fn reduce_no_init(vec_handle: Handle, fn_ptr: U64) -> I64 {
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
    #[rts_fn]
    pub fn reduce(vec_handle: Handle, identity: I64, fn_ptr: U64) -> I64 {
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
    #[rts_fn]
    pub fn num_threads() -> I64 {
        pool().current_num_threads() as i64
    }

    /// Vec of elements where `fn_ptr(x, i, arr)` is truthy. New Vec handle.
    #[rts_fn]
    pub fn filter(vec_handle: Handle, fn_ptr: U64) -> Handle {
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
    #[rts_fn]
    pub fn find(vec_handle: Handle, fn_ptr: U64) -> I64 {
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
    #[rts_fn]
    pub fn find_index(vec_handle: Handle, fn_ptr: U64) -> I64 {
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
    #[rts_fn]
    pub fn some(vec_handle: Handle, fn_ptr: U64) -> Bool {
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
    #[rts_fn]
    pub fn every(vec_handle: Handle, fn_ptr: U64) -> Bool {
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
