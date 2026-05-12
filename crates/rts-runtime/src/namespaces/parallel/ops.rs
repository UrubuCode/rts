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

/// JS-spec truthy do retorno de callback: chama __RTS_FN_RT_TRUTHY que
/// trata 0/null/undefined/""/false como falsy. Sem isso, filter com
/// `x => x` em array `[0, false, "", null]` so' descartava 0 e null
/// (bool/string handles passavam como nao-zero).
fn cb_truthy(v: i64) -> bool {
    crate::namespaces::gc::string_pool::__RTS_FN_RT_TRUTHY(v) != 0
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PARALLEL_MAP(vec_handle: u64, fn_ptr: u64) -> u64 {
    let Some(items) = snapshot_vec(vec_handle) else {
        return 0;
    };
    if fn_ptr == 0 {
        return 0;
    }
    // SAFETY: fn_ptr is `extern "C" fn(i64) -> i64` — contract com
    // codegen. Paralelo via rayon: callers que reescrevem `arr.map(fn)`
    // pra parallel.map (silent_parallelism) sao responsaveis por
    // garantir que `fn` eh pura (sem side effects). Hoje o pass
    // `array_methods_pass` so reescreve quando fn esta whitelisted
    // em `purity_pass` (math/string/num/etc puras).
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
    // JS Array.prototype.forEach garante ordem de iteracao (insertion
    // order). Sem isso, codigo como `entries.forEach(([k,v]) => print(...))`
    // produz saida fora de ordem (test object_builtins). Mantemos sequencial
    // — a vantagem de parallelism vem em map/reduce que sao puros.
    items.iter().for_each(|&x| f(x));
}

/// (cross-runtime #254) `arr.reduce(fn)` sem initial value. JS spec:
/// primeiro elemento vira acc, loop comeca do index 1. Array vazio
/// joga TypeError; aqui retornamos 0 (caller pode adicionar guard se
/// quiser distinguir). Sequencial — sem identity, paralelizar exige
/// fn associativa garantida.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PARALLEL_REDUCE_NO_INIT(
    vec_handle: u64,
    fn_ptr: u64,
) -> i64 {
    let Some(items) = snapshot_vec(vec_handle) else { return 0; };
    if fn_ptr == 0 || items.is_empty() { return 0; }
    let f: extern "C" fn(i64, i64) -> i64 = unsafe { std::mem::transmute(fn_ptr as usize) };
    let mut acc = items[0];
    for &x in &items[1..] {
        acc = f(acc, x);
    }
    acc
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
    // SAFETY: fn_ptr is `extern "C" fn(i64, i64) -> i64` (assumida
    // associativa/comutativa pelo silent reduce_pass — so ops + e *
    // qualificam). Paralelo via rayon.
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
    let result: Vec<i64> = items.into_iter().filter(|&x| cb_truthy(f(x))).collect();
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
    match items.into_iter().find(|&x| cb_truthy(f(x))) {
        Some(v) => v,
        None => {
            // JS spec: find sem match retorna `undefined`. Aloca handle
            // de string "undefined" — codegen marca .find(...) como
            // ambiguo (var_member_call_values), template literal/console
            // formata corretamente via TPL_COERCE_AUTO/INSPECT.
            alloc_entry(Entry::String(b"undefined".to_vec())) as i64
        }
    }
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
        .position(|x| cb_truthy(f(x)))
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
    if items.into_iter().any(|x| cb_truthy(f(x))) { 1 } else { 0 }
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
    if items.into_iter().all(|x| cb_truthy(f(x))) { 1 } else { 0 }
}
