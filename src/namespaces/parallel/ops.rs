//! parallel::map / for_each / reduce — Rayon-backed data parallelism.

use rayon::prelude::*;

use super::super::gc::handles::{Entry, alloc_entry, with_entry};
use super::pool::pool;

fn snapshot_vec(handle: u64) -> Option<Vec<i64>> {
    with_entry(handle, |entry| match entry {
        Some(Entry::Vec(v)) => Some(v.as_ref().clone()),
        _ => None,
    })
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

// (#208) filter/find/findIndex/some/every — predicate fn `extern "C" fn(i64) -> i64`
// retorna 0/non-zero como bool. Sequenciais (não paralelos) porque
// preservar ordem de elementos e' importante; uma versao parallel
// poderia vir depois com Vec ordering preservation.

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PARALLEL_FILTER(vec_handle: u64, fn_ptr: u64) -> u64 {
    let Some(items) = snapshot_vec(vec_handle) else {
        return 0;
    };
    if fn_ptr == 0 {
        return 0;
    }
    let f: extern "C" fn(i64) -> i64 = unsafe { std::mem::transmute(fn_ptr as usize) };
    let result: Vec<i64> = items.into_iter().filter(|&x| f(x) != 0).collect();
    alloc_entry(Entry::Vec(Box::new(result)))
}

/// Retorna primeiro elemento que satisfaz predicate, ou 0 se nenhum.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PARALLEL_FIND(vec_handle: u64, fn_ptr: u64) -> i64 {
    let Some(items) = snapshot_vec(vec_handle) else {
        return 0;
    };
    if fn_ptr == 0 {
        return 0;
    }
    let f: extern "C" fn(i64) -> i64 = unsafe { std::mem::transmute(fn_ptr as usize) };
    items.into_iter().find(|&x| f(x) != 0).unwrap_or(0)
}

/// Retorna index do primeiro elemento que satisfaz predicate, ou -1.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PARALLEL_FIND_INDEX(vec_handle: u64, fn_ptr: u64) -> i64 {
    let Some(items) = snapshot_vec(vec_handle) else {
        return -1;
    };
    if fn_ptr == 0 {
        return -1;
    }
    let f: extern "C" fn(i64) -> i64 = unsafe { std::mem::transmute(fn_ptr as usize) };
    items
        .into_iter()
        .position(|x| f(x) != 0)
        .map(|i| i as i64)
        .unwrap_or(-1)
}

/// Retorna 1 se algum elemento satisfaz predicate, 0 caso contrario.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PARALLEL_SOME(vec_handle: u64, fn_ptr: u64) -> i64 {
    let Some(items) = snapshot_vec(vec_handle) else {
        return 0;
    };
    if fn_ptr == 0 {
        return 0;
    }
    let f: extern "C" fn(i64) -> i64 = unsafe { std::mem::transmute(fn_ptr as usize) };
    if items.into_iter().any(|x| f(x) != 0) { 1 } else { 0 }
}

/// Retorna 1 se todos os elementos satisfazem predicate, 0 caso contrario.
/// Vazio = true (vacuously).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PARALLEL_EVERY(vec_handle: u64, fn_ptr: u64) -> i64 {
    let Some(items) = snapshot_vec(vec_handle) else {
        return 1;
    };
    if fn_ptr == 0 {
        return 1;
    }
    let f: extern "C" fn(i64) -> i64 = unsafe { std::mem::transmute(fn_ptr as usize) };
    if items.into_iter().all(|x| f(x) != 0) { 1 } else { 0 }
}
