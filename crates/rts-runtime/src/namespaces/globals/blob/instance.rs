//! Runtime extern "C" para Blob/File.

use crate::namespaces::gc::handles::{Entry, alloc_entry, with_entry};
use indexmap::IndexMap;

fn str_from_parts(ptr: i64, len: i64) -> String {
    if ptr == 0 || len <= 0 {
        return String::new();
    }
    let slice = unsafe { std::slice::from_raw_parts(ptr as *const u8, len as usize) };
    std::str::from_utf8(slice).unwrap_or("").to_owned()
}

/// Concatena os bytes de cada parte (Vec de handles de elemento). Cada elemento
/// vira bytes: String -> utf8; Buffer -> raw; Vec -> cada i64 truncado p/ u8;
/// Blob/File (Map com "bytes") -> bytes internos.
fn concat_parts(parts_h: u64) -> Vec<u8> {
    if parts_h == 0 {
        return Vec::new();
    }
    let elems: Vec<i64> = with_entry(parts_h, |e| match e {
        Some(Entry::Vec(v)) => v.as_ref().clone(),
        _ => Vec::new(),
    });
    let mut out = Vec::new();
    for el in elems {
        let h = el as u64;
        let bytes = with_entry(h, |e| match e {
            Some(Entry::String(b)) => b.clone(),
            Some(Entry::Buffer(b)) => b.clone(),
            Some(Entry::Vec(v)) => v.iter().map(|&x| x as u8).collect(),
            // parte aninhada Blob/File: pega o buffer interno.
            Some(Entry::Map(m)) => {
                let bytes_h = m.get("bytes").copied().unwrap_or(0) as u64;
                with_entry(bytes_h, |be| match be {
                    Some(Entry::Buffer(b)) => b.clone(),
                    _ => Vec::new(),
                })
            }
            _ => Vec::new(),
        });
        out.extend(bytes);
    }
    out
}

fn make_blob(bytes: Vec<u8>, name: Option<String>, last_modified: i64, class: &str) -> u64 {
    let size = bytes.len() as i64;
    let bytes_h = alloc_entry(Entry::Buffer(bytes)) as i64;
    let mut m: IndexMap<String, i64> = IndexMap::new();
    m.insert("bytes".to_string(), bytes_h);
    m.insert("size".to_string(), size);
    if let Some(n) = name {
        let name_h = alloc_entry(Entry::String(n.into_bytes())) as i64;
        m.insert("name".to_string(), name_h);
        m.insert("lastModified".to_string(), last_modified);
    }
    m.insert(
        "__rts_class".to_string(),
        alloc_entry(Entry::String(class.as_bytes().to_vec())) as i64,
    );
    alloc_entry(Entry::Map(Box::new(m)))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_BLOB_NEW(parts: u64) -> u64 {
    make_blob(concat_parts(parts), None, 0, "Blob")
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_BLOB_NEW_EMPTY() -> u64 {
    make_blob(Vec::new(), None, 0, "Blob")
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_BLOB_SIZE(h: u64) -> i64 {
    with_entry(h, |e| match e {
        Some(Entry::Map(m)) => m.get("size").copied().unwrap_or(0),
        _ => 0,
    })
}

/// blob.text() — devolve a string UTF-8 dos bytes, envolvida num Promise
/// resolvido (await unwrappa; sem await, eh um handle de Promise).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_BLOB_TEXT(h: u64) -> u64 {
    let bytes = with_entry(h, |e| match e {
        Some(Entry::Map(m)) => {
            let bytes_h = m.get("bytes").copied().unwrap_or(0) as u64;
            with_entry(bytes_h, |be| match be {
                Some(Entry::Buffer(b)) => b.clone(),
                _ => Vec::new(),
            })
        }
        _ => Vec::new(),
    });
    let s = String::from_utf8_lossy(&bytes).into_owned();
    let str_h = alloc_entry(Entry::String(s.into_bytes()));
    let slot = crate::namespaces::gc::promise_slot::new_fulfilled(str_h as i64);
    alloc_entry(Entry::PromiseAsync(slot))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_FILE_NEW(
    parts: u64,
    name_ptr: i64,
    name_len: i64,
    opts: u64,
) -> u64 {
    let name = str_from_parts(name_ptr, name_len);
    let last_modified = if opts == 0 {
        0
    } else {
        with_entry(opts, |e| match e {
            Some(Entry::Map(m)) => m.get("lastModified").copied().unwrap_or(0),
            _ => 0,
        })
    };
    make_blob(concat_parts(parts), Some(name), last_modified, "File")
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_FILE_NAME(h: u64) -> u64 {
    with_entry(h, |e| match e {
        Some(Entry::Map(m)) => m.get("name").copied().unwrap_or(0) as u64,
        _ => 0,
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_FILE_LAST_MODIFIED(h: u64) -> i64 {
    with_entry(h, |e| match e {
        Some(Entry::Map(m)) => m.get("lastModified").copied().unwrap_or(0),
        _ => 0,
    })
}
