//! IndexMap<String, i64> — mapa de chave string para valor i64.
//!
//! Usa `indexmap::IndexMap` para preservar ordem de inserção, necessário
//! para implementar a ordem de enumeração de propriedades do JS:
//! - integer-indexed keys (`"0"`, `"1"`, `"2"`, ...) em ordem numérica
//!   ascendente;
//! - demais string keys em ordem de inserção.

use indexmap::IndexMap;

use super::super::gc::handles::{Entry, alloc_entry, free_handle, with_entry, with_entry_mut};

/// Reconhece "array index" no sentido do ECMA-262: string que representa
/// um u32 canônico (sem leading zeros exceto "0"; máximo 2^32 - 2).
/// Retorna o valor numérico para ordenação. Strings como "01", "+1", "1.0",
/// " 1" não são consideradas índices.
fn parse_array_index(s: &str) -> Option<u32> {
    if s.is_empty() {
        return None;
    }
    if s.len() > 1 && s.starts_with('0') {
        return None;
    }
    if !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let n: u32 = s.parse().ok()?;
    if n == u32::MAX {
        return None;
    }
    Some(n)
}

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
    F: FnOnce(&IndexMap<String, i64>) -> R,
{
    with_entry(handle, |entry| match entry {
        Some(Entry::Map(m)) => f(m.as_ref()),
        _ => default,
    })
}

fn with_map_mut<F, R>(handle: u64, default: R, f: F) -> R
where
    F: FnOnce(&mut IndexMap<String, i64>) -> R,
{
    with_entry_mut(handle, |entry| match entry {
        Some(Entry::Map(m)) => f(m.as_mut()),
        _ => default,
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_MAP_NEW() -> u64 {
    alloc_entry(Entry::Map(Box::new(IndexMap::new())))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_MAP_FREE(handle: u64) {
    free_handle(handle);
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
    with_map_mut(handle, 0, |m| if m.shift_remove(key).is_some() { 1 } else { 0 })
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_MAP_CLEAR(handle: u64) {
    with_map_mut(handle, (), |m| m.clear());
}

/// Shallow clone do map — aloca novo handle com mesmas (key, value) pairs.
/// Usado pelo desugar de `const { a, ...rest } = obj` (#312): rest e'
/// inicializado como clone, e em seguida o codegen emite map_delete para
/// cada key explicita.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_MAP_CLONE(handle: u64) -> u64 {
    let cloned: Option<IndexMap<String, i64>> =
        with_map(handle, None, |m| Some(m.clone()));
    match cloned {
        Some(m) => alloc_entry(Entry::Map(Box::new(m))),
        None => 0,
    }
}

/// Retorna a key na posição `idx` na ordem de enumeração definida pelo JS:
/// 1. integer-indexed keys (string que parseia para u32 sem leading zero,
///    exceto "0") em ordem numérica ascendente;
/// 2. demais string keys em ordem de inserção (preservada pelo IndexMap).
///
/// Usado por for-in. Retorna handle de string ou 0 se idx fora de range.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_MAP_KEY_AT(handle: u64, idx: i64) -> u64 {
    if idx < 0 {
        return 0;
    }
    let key_opt: Option<String> = with_map(handle, None, |m| {
        let mut int_keys: Vec<(u32, &String)> = Vec::new();
        let mut str_keys: Vec<&String> = Vec::new();
        for k in m.keys() {
            match parse_array_index(k) {
                Some(n) => int_keys.push((n, k)),
                None => str_keys.push(k),
            }
        }
        int_keys.sort_by_key(|(n, _)| *n);
        let i = idx as usize;
        if i < int_keys.len() {
            Some(int_keys[i].1.clone())
        } else {
            str_keys.get(i - int_keys.len()).map(|s| (*s).clone())
        }
    });
    match key_opt {
        Some(s) => crate::namespaces::gc::string_pool::__RTS_FN_NS_GC_STRING_NEW(
            s.as_ptr(),
            s.len() as i64,
        ),
        None => 0,
    }
}

/// (#266) Object.keys(obj) — retorna Vec<i64> com handles de strings dos
/// keys. Ordem: sorted asc (mesmo criterio de KEY_AT).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_MAP_KEYS(handle: u64) -> u64 {
    let keys: Vec<String> = with_map(handle, Vec::new(), |m| {
        let mut ks: Vec<String> = m.keys().cloned().collect();
        ks.sort();
        ks
    });
    let mut vec: Vec<i64> = Vec::with_capacity(keys.len());
    for k in keys {
        let h = crate::namespaces::gc::string_pool::__RTS_FN_NS_GC_STRING_NEW(
            k.as_ptr(),
            k.len() as i64,
        );
        vec.push(h as i64);
    }
    crate::namespaces::gc::handles::alloc_entry(
        crate::namespaces::gc::handles::Entry::Vec(Box::new(vec)),
    )
}

/// (#266) Object.values(obj) — retorna Vec<i64> com valores. Ordem por
/// keys sorted asc.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_COLLECTIONS_MAP_VALUES(handle: u64) -> u64 {
    let vals: Vec<i64> = with_map(handle, Vec::new(), |m| {
        let mut entries: Vec<(&String, &i64)> = m.iter().collect();
        entries.sort_by(|a, b| a.0.cmp(b.0));
        entries.into_iter().map(|(_, v)| *v).collect()
    });
    crate::namespaces::gc::handles::alloc_entry(
        crate::namespaces::gc::handles::Entry::Vec(Box::new(vals)),
    )
}
