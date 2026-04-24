//! Vec<i64> — lista ordenada de valores i64.

use super::super::gc::handles::{table, Entry};

fn with_vec<F, R>(handle: u64, default: R, f: F) -> R
where
    F: FnOnce(&Vec<i64>) -> R,
{
    let guard = table().lock().unwrap();
    match guard.get(handle) {
        Some(Entry::Vec(v)) => f(v.as_ref()),
        _ => default,
    }
}

fn with_vec_mut<F, R>(handle: u64, default: R, f: F) -> R
where
    F: FnOnce(&mut Vec<i64>) -> R,
{
    let t = table();
    let mut guard = t.lock().unwrap();
    match guard.get_mut(handle) {
        Some(Entry::Vec(v)) => f(v.as_mut()),
        _ => default,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_NEW() -> u64 {
    table()
        .lock()
        .unwrap()
        .alloc(Entry::Vec(Box::new(Vec::new())))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_FREE(handle: u64) {
    table().lock().unwrap().free(handle);
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_LEN(handle: u64) -> i64 {
    with_vec(handle, -1, |v| v.len() as i64)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_VEC_PUSH(handle: u64, value: i64) {
    with_vec_mut(handle, (), |v| v.push(value));
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
