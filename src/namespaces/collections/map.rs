//! HashMap<String, i64> — mapa de chave string para valor i64.

use std::collections::HashMap;

use super::super::gc::handles::{table, Entry};

fn str_from_abi<'a>(ptr: *const u8, len: i64) -> Option<&'a str> {
    if ptr.is_null() || len < 0 {
        return None;
    }
    // SAFETY: caller contract.
    let slice = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    std::str::from_utf8(slice).ok()
}

fn with_map<F, R>(handle: u64, default: R, f: F) -> R
where
    F: FnOnce(&HashMap<String, i64>) -> R,
{
    let guard = table().lock().unwrap();
    match guard.get(handle) {
        Some(Entry::Map(m)) => f(m.as_ref()),
        _ => default,
    }
}

fn with_map_mut<F, R>(handle: u64, default: R, f: F) -> R
where
    F: FnOnce(&mut HashMap<String, i64>) -> R,
{
    let t = table();
    let mut guard = t.lock().unwrap();
    match guard.get_mut(handle) {
        Some(Entry::Map(m)) => f(m.as_mut()),
        _ => default,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_MAP_NEW() -> u64 {
    table()
        .lock()
        .unwrap()
        .alloc(Entry::Map(Box::new(HashMap::new())))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_MAP_FREE(handle: u64) {
    table().lock().unwrap().free(handle);
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_MAP_LEN(handle: u64) -> i64 {
    with_map(handle, -1, |m| m.len() as i64)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_MAP_HAS(
    handle: u64,
    key_ptr: *const u8,
    key_len: i64,
) -> i64 {
    let Some(key) = str_from_abi(key_ptr, key_len) else {
        return 0;
    };
    with_map(handle, 0, |m| if m.contains_key(key) { 1 } else { 0 })
}

/// Retorna o valor associado a `key`, ou 0 se ausente.
/// (0 tambem e valor valido — use map_has para distinguir.)
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_MAP_GET(
    handle: u64,
    key_ptr: *const u8,
    key_len: i64,
) -> i64 {
    let Some(key) = str_from_abi(key_ptr, key_len) else {
        return 0;
    };
    with_map(handle, 0, |m| m.get(key).copied().unwrap_or(0))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_MAP_SET(
    handle: u64,
    key_ptr: *const u8,
    key_len: i64,
    value: i64,
) {
    let Some(key) = str_from_abi(key_ptr, key_len) else {
        return;
    };
    let key_owned = key.to_string();
    with_map_mut(handle, (), |m| {
        m.insert(key_owned, value);
    });
}

/// Remove a chave. Retorna 1 se removida, 0 se ausente.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_MAP_DELETE(
    handle: u64,
    key_ptr: *const u8,
    key_len: i64,
) -> i64 {
    let Some(key) = str_from_abi(key_ptr, key_len) else {
        return 0;
    };
    with_map_mut(handle, 0, |m| if m.remove(key).is_some() { 1 } else { 0 })
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_MAP_CLEAR(handle: u64) {
    with_map_mut(handle, (), |m| m.clear());
}

/// Retorna a key na posição `idx` (em ordem de iteração estável dentro
/// de uma chamada). Usado por for-in. Coleta keys em snapshot Vec
/// ordenado pra garantir ordem reproduzível em runs distintos. Retorna
/// handle de string ou 0 se idx fora de range.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_MAP_KEY_AT(handle: u64, idx: i64) -> u64 {
    if idx < 0 {
        return 0;
    }
    let key_opt: Option<String> = with_map(handle, None, |m| {
        let mut keys: Vec<&String> = m.keys().collect();
        keys.sort();
        keys.get(idx as usize).map(|s| (*s).clone())
    });
    match key_opt {
        Some(s) => crate::namespaces::gc::string_pool::__RTS_FN_NS_GC_STRING_NEW(
            s.as_ptr(),
            s.len() as i64,
        ),
        None => 0,
    }
}
