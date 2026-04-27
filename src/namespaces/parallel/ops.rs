//! parallel::map / for_each / reduce — Rayon-backed data parallelism.

use rayon::prelude::*;

use super::super::gc::handles::{Entry, alloc_entry, shard_for_handle};
use super::pool::pool;

fn snapshot_vec(handle: u64) -> Option<Vec<i64>> {
    let guard = shard_for_handle(handle).lock().unwrap();
    match guard.get(handle) {
        Some(Entry::Vec(v)) => Some(v.as_ref().clone()),
        _ => None,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PARALLEL_MAP(vec_handle: u64, fn_ptr: u64) -> u64 {
    let Some(items) = snapshot_vec(vec_handle) else {
        return 0;
    };
    if fn_ptr == 0 {
        return 0;
    }
    // SAFETY: fn_ptr is `extern "C" fn(i64) -> i64` — contract with codegen.
    // Each Rayon worker calls this independently; no shared mutable state.
    let f: extern "C" fn(i64) -> i64 = unsafe { std::mem::transmute(fn_ptr as usize) };
    let result: Vec<i64> = pool().install(|| items.par_iter().map(|&x| f(x)).collect());
    alloc_entry(Entry::Vec(Box::new(result)))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PARALLEL_FOR_EACH(vec_handle: u64, fn_ptr: u64) {
    let Some(items) = snapshot_vec(vec_handle) else {
        return;
    };
    if fn_ptr == 0 {
        return;
    }
    // SAFETY: fn_ptr is `extern "C" fn(i64)`.
    let f: extern "C" fn(i64) = unsafe { std::mem::transmute(fn_ptr as usize) };
    pool().install(|| items.par_iter().for_each(|&x| f(x)));
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PARALLEL_REDUCE(
    vec_handle: u64,
    identity: i64,
    fn_ptr: u64,
) -> i64 {
    let Some(items) = snapshot_vec(vec_handle) else {
        return identity;
    };
    if fn_ptr == 0 {
        return identity;
    }
    // SAFETY: fn_ptr is `extern "C" fn(i64, i64) -> i64` (associative, commutative).
    let f: extern "C" fn(i64, i64) -> i64 = unsafe { std::mem::transmute(fn_ptr as usize) };
    pool().install(|| {
        items
            .par_iter()
            .copied()
            .reduce(|| identity, |a, b| f(a, b))
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PARALLEL_NUM_THREADS() -> i64 {
    pool().current_num_threads() as i64
}
