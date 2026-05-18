//! FormData runtime — storage como Vec<(String, String)> preservando ordem.

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

/// Storage interno: armazena pairs como Vec<i64> alternando key_h, val_h.
fn pairs_handle(h: u64) -> u64 {
    with_entry(h, |e| match e {
        Some(Entry::Map(m)) => m.get("__pairs").copied().unwrap_or(0) as u64,
        _ => 0,
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_FORM_DATA_NEW() -> u64 {
    let pairs = alloc_entry(Entry::Vec(Box::new(Vec::new())));
    let mut m: IndexMap<String, i64> = IndexMap::new();
    m.insert("__pairs".to_string(), pairs as i64);
    m.insert("__rts_class".to_string(), {
        alloc_entry(Entry::String(b"FormData".to_vec())) as i64
    });
    alloc_entry(Entry::Map(Box::new(m)))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_FORM_DATA_APPEND(
    h: u64,
    name_ptr: i64,
    name_len: i64,
    val_ptr: i64,
    val_len: i64,
) {
    let name = str_from_parts(name_ptr, name_len);
    let val = str_from_parts(val_ptr, val_len);
    let name_h = alloc_entry(Entry::String(name.as_bytes().to_vec())) as i64;
    let val_h = alloc_entry(Entry::String(val.as_bytes().to_vec())) as i64;
    let pairs = pairs_handle(h);
    if pairs == 0 {
        return;
    }
    with_entry_mut(pairs, |e| {
        if let Some(Entry::Vec(v)) = e {
            v.push(name_h);
            v.push(val_h);
        }
    });
}

/// fd.set(name, value) — remove existentes do name, append novo.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_FORM_DATA_SET(
    h: u64,
    name_ptr: i64,
    name_len: i64,
    val_ptr: i64,
    val_len: i64,
) {
    let name = str_from_parts(name_ptr, name_len).to_string();
    let val = str_from_parts(val_ptr, val_len).to_string();
    let pairs = pairs_handle(h);
    if pairs == 0 {
        return;
    }
    with_entry_mut(pairs, |e| {
        if let Some(Entry::Vec(v)) = e {
            let mut new_v: Vec<i64> = Vec::new();
            let mut i = 0;
            while i + 1 < v.len() {
                let k_h = v[i] as u64;
                let key_str: String = match crate::namespaces::gc::handles::with_entry(k_h, |ke| match ke {
                    Some(Entry::String(b)) => Some(String::from_utf8_lossy(b).into_owned()),
                    _ => None,
                }) {
                    Some(s) => s,
                    None => String::new(),
                };
                if key_str != name {
                    new_v.push(v[i]);
                    new_v.push(v[i + 1]);
                }
                i += 2;
            }
            let name_h = alloc_entry(Entry::String(name.clone().into_bytes())) as i64;
            let val_h = alloc_entry(Entry::String(val.clone().into_bytes())) as i64;
            new_v.push(name_h);
            new_v.push(val_h);
            **v = new_v;
        }
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_FORM_DATA_DELETE(h: u64, name_ptr: i64, name_len: i64) {
    let name = str_from_parts(name_ptr, name_len).to_string();
    let pairs = pairs_handle(h);
    if pairs == 0 {
        return;
    }
    with_entry_mut(pairs, |e| {
        if let Some(Entry::Vec(v)) = e {
            let mut new_v: Vec<i64> = Vec::new();
            let mut i = 0;
            while i + 1 < v.len() {
                let k_h = v[i] as u64;
                let key_str: String = match crate::namespaces::gc::handles::with_entry(k_h, |ke| match ke {
                    Some(Entry::String(b)) => Some(String::from_utf8_lossy(b).into_owned()),
                    _ => None,
                }) {
                    Some(s) => s,
                    None => String::new(),
                };
                if key_str != name {
                    new_v.push(v[i]);
                    new_v.push(v[i + 1]);
                }
                i += 2;
            }
            **v = new_v;
        }
    });
}

/// fd.get(name) — primeiro value, handle de string ou 0 (null).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_FORM_DATA_GET(h: u64, name_ptr: i64, name_len: i64) -> u64 {
    let name = str_from_parts(name_ptr, name_len).to_string();
    let pairs = pairs_handle(h);
    let v: Vec<i64> = with_entry(pairs, |e| match e {
        Some(Entry::Vec(v)) => (**v).clone(),
        _ => Vec::new(),
    });
    let mut i = 0;
    while i + 1 < v.len() {
        let k_h = v[i] as u64;
        let key_str: String = match with_entry(k_h, |ke| match ke {
            Some(Entry::String(b)) => Some(String::from_utf8_lossy(b).into_owned()),
            _ => None,
        }) {
            Some(s) => s,
            None => String::new(),
        };
        if key_str == name {
            return v[i + 1] as u64;
        }
        i += 2;
    }
    0
}

/// fd.getAll(name) — Vec de string handles.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_FORM_DATA_GET_ALL(h: u64, name_ptr: i64, name_len: i64) -> u64 {
    let name = str_from_parts(name_ptr, name_len).to_string();
    let pairs = pairs_handle(h);
    let v: Vec<i64> = with_entry(pairs, |e| match e {
        Some(Entry::Vec(v)) => (**v).clone(),
        _ => Vec::new(),
    });
    let mut out: Vec<i64> = Vec::new();
    let mut i = 0;
    while i + 1 < v.len() {
        let k_h = v[i] as u64;
        let key_str: String = match with_entry(k_h, |ke| match ke {
            Some(Entry::String(b)) => Some(String::from_utf8_lossy(b).into_owned()),
            _ => None,
        }) {
            Some(s) => s,
            None => String::new(),
        };
        if key_str == name {
            out.push(v[i + 1]);
        }
        i += 2;
    }
    alloc_entry(Entry::Vec(Box::new(out)))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_FORM_DATA_HAS(h: u64, name_ptr: i64, name_len: i64) -> i64 {
    if __RTS_FN_GL_FORM_DATA_GET(h, name_ptr, name_len) != 0 {
        1
    } else {
        0
    }
}

/// fd.entries() — Vec de Vec[[name_h, val_h], ...] preservando ordem +
/// duplicatas.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_FORM_DATA_ENTRIES(h: u64) -> u64 {
    let pairs = pairs_handle(h);
    let v: Vec<i64> = with_entry(pairs, |e| match e {
        Some(Entry::Vec(v)) => (**v).clone(),
        _ => Vec::new(),
    });
    let mut out: Vec<i64> = Vec::with_capacity(v.len() / 2);
    let mut i = 0;
    while i + 1 < v.len() {
        let pair = alloc_entry(Entry::Vec(Box::new(vec![v[i], v[i + 1]])));
        out.push(pair as i64);
        i += 2;
    }
    alloc_entry(Entry::Vec(Box::new(out)))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_FORM_DATA_KEYS(h: u64) -> u64 {
    let pairs = pairs_handle(h);
    let v: Vec<i64> = with_entry(pairs, |e| match e {
        Some(Entry::Vec(v)) => (**v).clone(),
        _ => Vec::new(),
    });
    let mut out: Vec<i64> = Vec::with_capacity(v.len() / 2);
    let mut i = 0;
    while i + 1 < v.len() {
        out.push(v[i]);
        i += 2;
    }
    alloc_entry(Entry::Vec(Box::new(out)))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_FORM_DATA_VALUES(h: u64) -> u64 {
    let pairs = pairs_handle(h);
    let v: Vec<i64> = with_entry(pairs, |e| match e {
        Some(Entry::Vec(v)) => (**v).clone(),
        _ => Vec::new(),
    });
    let mut out: Vec<i64> = Vec::with_capacity(v.len() / 2);
    let mut i = 0;
    while i + 1 < v.len() {
        out.push(v[i + 1]);
        i += 2;
    }
    alloc_entry(Entry::Vec(Box::new(out)))
}
