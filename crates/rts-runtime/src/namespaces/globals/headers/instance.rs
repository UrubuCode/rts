//! Implementacao runtime de Headers.

use crate::namespaces::gc::handles::{alloc_entry, with_entry, with_entry_mut, Entry};
use indexmap::IndexMap;

fn str_from_parts<'a>(ptr: i64, len: i64) -> &'a str {
    if ptr == 0 || len <= 0 {
        return "";
    }
    unsafe {
        let slice = std::slice::from_raw_parts(ptr as *const u8, len as usize);
        std::str::from_utf8_unchecked(slice)
    }
}

fn norm_name(name: &str) -> String {
    name.to_ascii_lowercase()
}

/// new Headers() — vazio.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_HEADERS_NEW() -> u64 {
    alloc_entry(Entry::Headers(Box::new(IndexMap::new())))
}

/// new Headers(arr) — array de pares [name, value]. Vec de Vec handles.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_HEADERS_NEW_FROM(arr_h: u64) -> u64 {
    let pairs: Vec<(String, String)> = with_entry(arr_h, |e| match e {
        Some(Entry::Vec(v)) => v
            .iter()
            .filter_map(|&pair_raw| {
                let pair_h = pair_raw as u64;
                let pair_data: Option<(u64, u64)> = with_entry(pair_h, |pe| match pe {
                    Some(Entry::Vec(pv)) if pv.len() >= 2 => Some((pv[0] as u64, pv[1] as u64)),
                    _ => None,
                });
                pair_data.and_then(|(kh, vh)| {
                    let k = with_entry(kh, |ke| match ke {
                        Some(Entry::String(b)) => Some(String::from_utf8_lossy(b).into_owned()),
                        _ => None,
                    })?;
                    let v = with_entry(vh, |ve| match ve {
                        Some(Entry::String(b)) => Some(String::from_utf8_lossy(b).into_owned()),
                        _ => None,
                    })?;
                    Some((k, v))
                })
            })
            .collect(),
        _ => Vec::new(),
    });
    let mut m: IndexMap<String, Vec<String>> = IndexMap::new();
    for (k, v) in pairs {
        m.entry(norm_name(&k)).or_default().push(v);
    }
    alloc_entry(Entry::Headers(Box::new(m)))
}

/// h.append(name, value)
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_HEADERS_APPEND(
    h: u64,
    name_ptr: i64,
    name_len: i64,
    val_ptr: i64,
    val_len: i64,
) {
    let name = norm_name(str_from_parts(name_ptr, name_len));
    let val = str_from_parts(val_ptr, val_len).to_string();
    with_entry_mut(h, |e| {
        if let Some(Entry::Headers(map)) = e {
            map.entry(name.clone()).or_default().push(val.clone());
        }
    });
}

/// h.set(name, value) — substitui todos os valores existentes.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_HEADERS_SET(
    h: u64,
    name_ptr: i64,
    name_len: i64,
    val_ptr: i64,
    val_len: i64,
) {
    let name = norm_name(str_from_parts(name_ptr, name_len));
    let val = str_from_parts(val_ptr, val_len).to_string();
    with_entry_mut(h, |e| {
        if let Some(Entry::Headers(map)) = e {
            map.insert(name.clone(), vec![val.clone()]);
        }
    });
}

/// h.get(name) — junta valores com ", ". Retorna handle de string ou 0
/// (null em JS) se nao houver.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_HEADERS_GET(h: u64, name_ptr: i64, name_len: i64) -> u64 {
    let name = norm_name(str_from_parts(name_ptr, name_len));
    let result: Option<String> = with_entry(h, |e| match e {
        Some(Entry::Headers(map)) => map.get(&name).map(|vals| vals.join(", ")),
        _ => None,
    });
    match result {
        Some(s) => alloc_entry(Entry::String(s.into_bytes())),
        None => 0,
    }
}

/// h.has(name) — bool.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_HEADERS_HAS(h: u64, name_ptr: i64, name_len: i64) -> i64 {
    let name = norm_name(str_from_parts(name_ptr, name_len));
    with_entry(h, |e| match e {
        Some(Entry::Headers(map)) => {
            if map.contains_key(&name) { 1 } else { 0 }
        }
        _ => 0,
    })
}

/// h.delete(name)
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_HEADERS_DELETE(h: u64, name_ptr: i64, name_len: i64) {
    let name = norm_name(str_from_parts(name_ptr, name_len));
    with_entry_mut(h, |e| {
        if let Some(Entry::Headers(map)) = e {
            map.shift_remove(&name);
        }
    });
}

/// h.getSetCookie() — retorna Vec dos valores raw de "set-cookie", sem juntar.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_HEADERS_GET_SET_COOKIE(h: u64) -> u64 {
    let vals: Vec<String> = with_entry(h, |e| match e {
        Some(Entry::Headers(map)) => map.get("set-cookie").cloned().unwrap_or_default(),
        _ => Vec::new(),
    });
    let handles: Vec<i64> = vals
        .into_iter()
        .map(|v| alloc_entry(Entry::String(v.into_bytes())) as i64)
        .collect();
    alloc_entry(Entry::Vec(Box::new(handles)))
}

/// h.entries() — Vec de Vec[[name_handle, joined_value_handle], ...].
/// Cada nome unico vira uma entry com valores juntados por ", " (JS spec
/// para iteracao normal — getSetCookie eh excecao).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_HEADERS_ENTRIES(h: u64) -> u64 {
    let pairs: Vec<(String, String)> = with_entry(h, |e| match e {
        Some(Entry::Headers(map)) => map
            .iter()
            .map(|(k, vs)| (k.clone(), vs.join(", ")))
            .collect(),
        _ => Vec::new(),
    });
    let out: Vec<i64> = pairs
        .into_iter()
        .map(|(k, v)| {
            let kh = alloc_entry(Entry::String(k.into_bytes())) as i64;
            let vh = alloc_entry(Entry::String(v.into_bytes())) as i64;
            alloc_entry(Entry::Vec(Box::new(vec![kh, vh]))) as i64
        })
        .collect();
    alloc_entry(Entry::Vec(Box::new(out)))
}

/// h.keys() — Vec de string handles.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_HEADERS_KEYS(h: u64) -> u64 {
    let keys: Vec<String> = with_entry(h, |e| match e {
        Some(Entry::Headers(map)) => map.keys().cloned().collect(),
        _ => Vec::new(),
    });
    let out: Vec<i64> = keys
        .into_iter()
        .map(|k| alloc_entry(Entry::String(k.into_bytes())) as i64)
        .collect();
    alloc_entry(Entry::Vec(Box::new(out)))
}

/// h.values() — Vec de string handles juntados por ", ".
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_HEADERS_VALUES(h: u64) -> u64 {
    let vals: Vec<String> = with_entry(h, |e| match e {
        Some(Entry::Headers(map)) => map.values().map(|vs| vs.join(", ")).collect(),
        _ => Vec::new(),
    });
    let out: Vec<i64> = vals
        .into_iter()
        .map(|v| alloc_entry(Entry::String(v.into_bytes())) as i64)
        .collect();
    alloc_entry(Entry::Vec(Box::new(out)))
}
