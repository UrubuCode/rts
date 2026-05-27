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
    // SAFETY: fn_ptr is `extern "C" fn(i64, i64, i64) -> i64` — contract com
    // codegen. Callbacks de array methods (#776) recebem (val, idx, array_handle).
    // Lifted callbacks com menos params ignoram args extras (regs RDX/R8 em
    // x86_64 Win64, RSI/RDX em SysV). Paralelo via rayon — silent rewrite so
    // dispara quando fn esta whitelisted em `purity_pass`.
    let f: extern "C" fn(i64, i64, i64) -> i64 = unsafe { std::mem::transmute(fn_ptr as usize) };
    let arr = vec_handle as i64;
    // (cross-runtime #52) JS spec: map preserva holes (slot vazio
    // permanece vazio no resultado, callback nao eh invocado).
    let result: Vec<i64> = pool().install(|| {
        items
            .par_iter()
            .enumerate()
            .map(|(i, &x)| if x == i64::MIN + 4 { x } else { f(x, i as i64, arr) })
            .collect()
    });
    alloc_entry(Entry::Vec(Box::new(result)))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_PARALLEL_FOR_EACH(vec_handle: u64, fn_ptr: u64) {
    if fn_ptr == 0 {
        return;
    }
    // (engine) Quando o receiver eh Map/Set (codegen reescreveu Set.forEach
    // em parallel.for_each), delega pro MAP_FOR_EACH que usa INVOKE_AUTO e
    // sabe iterar storage Map<keyStr, val>.
    let is_map_or_set = crate::namespaces::gc::handles::with_entry(vec_handle, |e| {
        matches!(e, Some(crate::namespaces::gc::handles::Entry::Map(_)))
    });
    if is_map_or_set {
        crate::namespaces::collections::map::__RTS_FN_NS_COLLECTIONS_MAP_FOR_EACH(
            vec_handle, fn_ptr,
        );
        return;
    }
    let Some(items) = snapshot_vec(vec_handle) else {
        return;
    };
    // SAFETY: fn_ptr is `extern "C" fn(i64, i64, i64) -> i64`. Callbacks
    // de array methods (#776) recebem (val, idx, array_handle); lifted
    // callbacks com menos params ignoram args extras (calling convention).
    let f: extern "C" fn(i64, i64, i64) -> i64 = unsafe { std::mem::transmute(fn_ptr as usize) };
    let arr = vec_handle as i64;
    items.iter().enumerate().for_each(|(i, &x)| {
        if x != i64::MIN + 4 {
            f(x, i as i64, arr);
        }
    });
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
    // (cross-runtime #345) 3 params (acc, val, index); sem init o callback
    // roda a partir do indice 1 (items[0] eh o acc inicial).
    let f: extern "C" fn(i64, i64, i64) -> i64 = unsafe { std::mem::transmute(fn_ptr as usize) };
    let mut acc = items[0];
    for (i, &x) in items.iter().enumerate().skip(1) {
        acc = f(acc, x, i as i64);
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
    // (cross-runtime #761) Sequencial — `array_methods_pass` redireciona
    // qualquer `arr.reduce(arrow, init)` pra ca, inclusive callbacks
    // nao-associativas (ex: construir objeto via spread). Rayon's reduce
    // chama op com (identity, x) e depois combina, o que produz handles
    // intermediarios mortos / resultado errado pra fns nao-comutativas.
    // Reduce puro de inteiros via `reduce_pass` ainda funciona; perde so
    // o ganho de paralelismo em soma/produto.
    // (cross-runtime #345) callback liftado de reduce tem 3 params
    // (acc, val, index). Passamos index como 3o arg; callbacks de 2 params
    // ignoram o extra (calling convention).
    let f: extern "C" fn(i64, i64, i64) -> i64 = unsafe { std::mem::transmute(fn_ptr as usize) };
    let mut acc = identity;
    for (i, &x) in items.iter().enumerate() {
        acc = f(acc, x, i as i64);
    }
    acc
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
    // (#776) callback recebe (val, idx, array_handle); 1-param lifted ignora
    // args extras via calling convention.
    let f: extern "C" fn(i64, i64, i64) -> i64 = unsafe { std::mem::transmute(fn_ptr as usize) };
    let arr = vec_handle as i64;
    // (cross-runtime #52) JS spec: filter pula holes (sparse slot ==
    // i64::MIN + 4) sem invocar callback. Resultado sem holes.
    let result: Vec<i64> = items
        .into_iter()
        .enumerate()
        .filter(|&(i, x)| x != i64::MIN + 4 && cb_truthy(f(x, i as i64, arr)))
        .map(|(_, x)| x)
        .collect();
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
    let f: extern "C" fn(i64, i64, i64) -> i64 = unsafe { std::mem::transmute(fn_ptr as usize) };
    let arr = vec_handle as i64;
    match items.into_iter().enumerate().find(|&(i, x)| cb_truthy(f(x, i as i64, arr))) {
        Some((_, v)) => v,
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
    let f: extern "C" fn(i64, i64, i64) -> i64 = unsafe { std::mem::transmute(fn_ptr as usize) };
    let arr = vec_handle as i64;
    items
        .into_iter()
        .enumerate()
        .position(|(i, x)| cb_truthy(f(x, i as i64, arr)))
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
    let f: extern "C" fn(i64, i64, i64) -> i64 = unsafe { std::mem::transmute(fn_ptr as usize) };
    let arr = vec_handle as i64;
    if items.into_iter().enumerate().any(|(i, x)| cb_truthy(f(x, i as i64, arr))) { 1 } else { 0 }
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
    let f: extern "C" fn(i64, i64, i64) -> i64 = unsafe { std::mem::transmute(fn_ptr as usize) };
    let arr = vec_handle as i64;
    if items.into_iter().enumerate().all(|(i, x)| cb_truthy(f(x, i as i64, arr))) { 1 } else { 0 }
}
