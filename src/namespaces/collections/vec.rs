//! Vec<i64> — lista ordenada de valores i64.

use super::super::gc::handles::{Entry, alloc_entry, free_handle, shard_for_handle};

fn with_vec<F, R>(handle: u64, default: R, f: F) -> R
where
    F: FnOnce(&Vec<i64>) -> R,
{
    let guard = shard_for_handle(handle).lock().unwrap();
    match guard.get(handle) {
        Some(Entry::Vec(v)) => f(v.as_ref()),
        _ => default,
    }
}

fn with_vec_mut<F, R>(handle: u64, default: R, f: F) -> R
where
    F: FnOnce(&mut Vec<i64>) -> R,
{
    let mut guard = shard_for_handle(handle).lock().unwrap();
    match guard.get_mut(handle) {
        Some(Entry::Vec(v)) => f(v.as_mut()),
        _ => default,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_NEW() -> u64 {
    alloc_entry(Entry::Vec(Box::new(Vec::new())))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_FREE(handle: u64) {
    free_handle(handle);
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_LEN(handle: u64) -> i64 {
    with_vec(handle, -1, |v| v.len() as i64)
}

/// Limite duro de elementos por vec — protege contra OOM em cenarios
/// patologicos (ex: generator infinito desugared para buffer eager,
/// loop sem condicao de parada). 1M i64 = 8MiB por vec, suficiente
/// pro caso real e barato comparado aos GBs que um leak descontrolado
/// produz.
const VEC_MAX_LEN: usize = 1_000_000;

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_PUSH(handle: u64, value: i64) {
    let limit_hit = with_vec_mut(handle, false, |v| {
        if v.len() >= VEC_MAX_LEN {
            return true;
        }
        v.push(value);
        false
    });
    if limit_hit {
        eprintln!(
            "RTS runtime: vec push exceeded limit of {VEC_MAX_LEN} elements; aborting (likely infinite generator or unbounded loop)"
        );
        std::process::abort();
    }
}

/// Remove e retorna o ultimo valor, ou 0 se vazio.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_POP(handle: u64) -> i64 {
    with_vec_mut(handle, 0, |v| v.pop().unwrap_or(0))
}

/// Valor em `index`, ou 0 fora do range.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_GET(handle: u64, index: i64) -> i64 {
    if index < 0 {
        return 0;
    }
    with_vec(handle, 0, |v| v.get(index as usize).copied().unwrap_or(0))
}

/// Escreve `value` em `index`. No-op fora do range.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_SET(handle: u64, index: i64, value: i64) {
    if index < 0 {
        return;
    }
    with_vec_mut(handle, (), |v| {
        if let Some(slot) = v.get_mut(index as usize) {
            *slot = value;
        }
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_CLEAR(handle: u64) {
    with_vec_mut(handle, (), |v| v.clear());
}

/// Junta os elementos do Vec interpretando cada i64 como:
///   - string handle valido → conteudo da string
///   - caso contrario → representacao decimal do numero
/// Retorna handle de string nova com os elementos separados por `sep_h`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_JOIN(handle: u64, sep_h: u64) -> u64 {
    // Snapshot dos elementos sem segurar o lock — formatar pode tocar
    // outros shards (resolver string handles).
    let elems: Vec<i64> = {
        let guard = shard_for_handle(handle).lock().unwrap();
        match guard.get(handle) {
            Some(Entry::Vec(v)) => v.iter().copied().collect(),
            _ => return 0,
        }
    };

    // Resolve separador como bytes; vazio se handle invalido.
    let sep_bytes: Vec<u8> = {
        let guard = shard_for_handle(sep_h).lock().unwrap();
        match guard.get(sep_h) {
            Some(Entry::String(b)) => b.clone(),
            _ => Vec::new(),
        }
    };

    let mut out: Vec<u8> = Vec::new();
    for (i, e) in elems.iter().enumerate() {
        if i > 0 {
            out.extend_from_slice(&sep_bytes);
        }
        let h = *e as u64;
        // Tenta como string handle primeiro.
        let as_str: Option<Vec<u8>> = {
            let guard = shard_for_handle(h).lock().unwrap();
            match guard.get(h) {
                Some(Entry::String(b)) => Some(b.clone()),
                _ => None,
            }
        };
        if let Some(b) = as_str {
            out.extend_from_slice(&b);
        } else {
            // Fallback: formata como i64 decimal.
            out.extend_from_slice(e.to_string().as_bytes());
        }
    }

    alloc_entry(Entry::String(out))
}
