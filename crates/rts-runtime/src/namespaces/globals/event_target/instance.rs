//! Implementacao runtime de EventTarget e Event.
//!
//! EventTarget storage: Entry::Map com field "listeners" -> Map handle de
//! "<type>" -> Vec<i64> (fn handles).
//! Event storage: Entry::Map com field "type" -> string handle.

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

// ── EventTarget ───────────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_EVENT_TARGET_NEW() -> u64 {
    let listeners = alloc_entry(Entry::Map(Box::new(IndexMap::new())));
    let mut m: IndexMap<String, i64> = IndexMap::new();
    m.insert("listeners".to_string(), listeners as i64);
    m.insert("__rts_class".to_string(), {
        alloc_entry(Entry::String(b"EventTarget".to_vec())) as i64
    });
    alloc_entry(Entry::Map(Box::new(m)))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_EVENT_TARGET_ADD_LISTENER(
    h: u64,
    type_ptr: i64,
    type_len: i64,
    fn_h: u64,
) {
    let ty = str_from_parts(type_ptr, type_len).to_string();
    let listeners_h: u64 = with_entry(h, |e| match e {
        Some(Entry::Map(m)) => m.get("listeners").copied().unwrap_or(0) as u64,
        _ => 0,
    });
    if listeners_h == 0 {
        return;
    }
    // listeners[ty] = Vec<fn>; cria se nao existe.
    let mut vec_h: u64 = with_entry(listeners_h, |e| match e {
        Some(Entry::Map(m)) => m.get(&ty).copied().unwrap_or(0) as u64,
        _ => 0,
    });
    if vec_h == 0 {
        vec_h = alloc_entry(Entry::Vec(Box::new(Vec::new())));
        with_entry_mut(listeners_h, |e| {
            if let Some(Entry::Map(m)) = e {
                m.insert(ty.clone(), vec_h as i64);
            }
        });
    }
    with_entry_mut(vec_h, |e| {
        if let Some(Entry::Vec(v)) = e {
            v.push(fn_h as i64);
        }
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_EVENT_TARGET_REMOVE_LISTENER(
    h: u64,
    type_ptr: i64,
    type_len: i64,
    fn_h: u64,
) {
    let ty = str_from_parts(type_ptr, type_len).to_string();
    let listeners_h: u64 = with_entry(h, |e| match e {
        Some(Entry::Map(m)) => m.get("listeners").copied().unwrap_or(0) as u64,
        _ => 0,
    });
    if listeners_h == 0 {
        return;
    }
    let vec_h: u64 = with_entry(listeners_h, |e| match e {
        Some(Entry::Map(m)) => m.get(&ty).copied().unwrap_or(0) as u64,
        _ => 0,
    });
    if vec_h == 0 {
        return;
    }
    with_entry_mut(vec_h, |e| {
        if let Some(Entry::Vec(v)) = e {
            v.retain(|&x| x != fn_h as i64);
        }
    });
}

/// target.dispatchEvent(event) — chama todos listeners do type, retorna bool
/// (true se nao cancelado; em RTS sempre true por ora).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_EVENT_TARGET_DISPATCH(h: u64, event_h: u64) -> i64 {
    // Lookup do type do event.
    let ty: String = with_entry(event_h, |e| match e {
        Some(Entry::Map(m)) => {
            let t_h = m.get("type").copied().unwrap_or(0) as u64;
            with_entry(t_h, |te| match te {
                Some(Entry::String(b)) => String::from_utf8_lossy(b).into_owned(),
                _ => String::new(),
            })
        }
        _ => String::new(),
    });
    if ty.is_empty() {
        return 1;
    }
    // Coleta fn handles fora do lock.
    let listeners_h: u64 = with_entry(h, |e| match e {
        Some(Entry::Map(m)) => m.get("listeners").copied().unwrap_or(0) as u64,
        _ => 0,
    });
    let vec_h: u64 = with_entry(listeners_h, |e| match e {
        Some(Entry::Map(m)) => m.get(&ty).copied().unwrap_or(0) as u64,
        _ => 0,
    });
    let fps: Vec<u64> = with_entry(vec_h, |e| match e {
        Some(Entry::Vec(v)) => v.iter().map(|&x| x as u64).collect(),
        _ => Vec::new(),
    });
    // Invoca cada fn com (event_h) como arg.
    for fp in fps {
        if fp == 0 {
            continue;
        }
        let args = alloc_entry(Entry::Vec(Box::new(vec![event_h as i64])));
        unsafe extern "C" {
            fn __RTS_FN_RT_INVOKE_AUTO(fn_h: i64, this_h: i64, args_h: i64) -> i64;
        }
        let _ = unsafe { __RTS_FN_RT_INVOKE_AUTO(fp as i64, h as i64, args as i64) };
    }
    1
}

// ── Event ─────────────────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_EVENT_NEW(type_ptr: i64, type_len: i64) -> u64 {
    let ty = str_from_parts(type_ptr, type_len);
    let type_h = alloc_entry(Entry::String(ty.as_bytes().to_vec()));
    let mut m: IndexMap<String, i64> = IndexMap::new();
    m.insert("type".to_string(), type_h as i64);
    m.insert("__rts_class".to_string(), {
        alloc_entry(Entry::String(b"Event".to_vec())) as i64
    });
    alloc_entry(Entry::Map(Box::new(m)))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_EVENT_TYPE(h: u64) -> u64 {
    with_entry(h, |e| match e {
        Some(Entry::Map(m)) => m.get("type").copied().unwrap_or(0) as u64,
        _ => 0,
    })
}
