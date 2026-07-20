//! `Headers` global class (#289).
//!
//! Multimap case-insensitive de header name -> lista de valores. Migrado do
//! `#[rts_class]` (macro) pro modelo builder hand-written do `rts-engine`
//! (rumo à remoção da `rts-macro`). Os externs `__RTS_FN_GL_HEADERS_*` +
//! `register_headers_class_spec()` são escritos à mão.

use indexmap::IndexMap;

use rts_engine::abi::ty::{Bool, Handle};
use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

use rts_engine::heap::handles::{alloc_entry, with_entry, with_entry_mut, Entry};

fn norm_name(name: &str) -> String {
    name.to_ascii_lowercase()
}

/// new Headers() — vazio.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_HEADERS_NEW() -> Handle {
    alloc_entry(Entry::Headers(Box::new(IndexMap::new())))
}

/// new Headers(arr) — array de pares [name, value].
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_HEADERS_NEW_FROM(arr_h: Handle) -> Handle {
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
    h: Handle,
    name_ptr: *const u8,
    name_len: i64,
    value_ptr: *const u8,
    value_len: i64,
) {
    let name = unsafe { rts_engine::abi::str_abi::from_abi(name_ptr, name_len) };
    let value = unsafe { rts_engine::abi::str_abi::from_abi(value_ptr, value_len) };
    let name = norm_name(name.unwrap_or(""));
    let val = value.unwrap_or("").to_string();
    with_entry_mut(h, |e| {
        if let Some(Entry::Headers(map)) = e {
            map.entry(name.clone()).or_default().push(val.clone());
        }
    });
}

/// h.set(name, value) — substitui todos os valores existentes.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_HEADERS_SET(
    h: Handle,
    name_ptr: *const u8,
    name_len: i64,
    value_ptr: *const u8,
    value_len: i64,
) {
    let name = unsafe { rts_engine::abi::str_abi::from_abi(name_ptr, name_len) };
    let value = unsafe { rts_engine::abi::str_abi::from_abi(value_ptr, value_len) };
    let name = norm_name(name.unwrap_or(""));
    let val = value.unwrap_or("").to_string();
    with_entry_mut(h, |e| {
        if let Some(Entry::Headers(map)) = e {
            map.insert(name.clone(), vec![val.clone()]);
        }
    });
}

/// h.get(name) — junta valores com ", ". 0 (null) se nao houver.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_HEADERS_GET(h: Handle, name_ptr: *const u8, name_len: i64) -> Handle {
    let name = match unsafe { rts_engine::abi::str_abi::from_abi(name_ptr, name_len) } {
        Some(s) => s,
        None => return 0,
    };
    let name = norm_name(name);
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
pub extern "C" fn __RTS_FN_GL_HEADERS_HAS(h: Handle, name_ptr: *const u8, name_len: i64) -> Bool {
    let name = match unsafe { rts_engine::abi::str_abi::from_abi(name_ptr, name_len) } {
        Some(s) => s,
        None => return 0,
    };
    let name = norm_name(name);
    with_entry(h, |e| match e {
        Some(Entry::Headers(map)) => {
            if map.contains_key(&name) {
                1
            } else {
                0
            }
        }
        _ => 0,
    })
}

/// h.delete(name)
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_HEADERS_DELETE(h: Handle, name_ptr: *const u8, name_len: i64) {
    let name = unsafe { rts_engine::abi::str_abi::from_abi(name_ptr, name_len) };
    let name = norm_name(name.unwrap_or(""));
    with_entry_mut(h, |e| {
        if let Some(Entry::Headers(map)) = e {
            map.shift_remove(&name);
        }
    });
}

/// h.getSetCookie() — Vec dos valores raw de "set-cookie", sem juntar.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_HEADERS_GET_SET_COOKIE(h: Handle) -> Handle {
    let vals: Vec<String> = with_entry(h, |e| match e {
        Some(Entry::Headers(map)) => map.get("set-cookie").cloned().unwrap_or_default(),
        _ => Vec::new(),
    });
    let handles: Vec<i64> = vals
        .into_iter()
        .map(|v| rts_engine::heap::shapes::string_word(v.as_bytes()) as i64)
        .collect();
    alloc_entry(Entry::Vec(Box::new(handles)))
}

/// h.entries() — Vec de [name_handle, joined_value_handle].
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_HEADERS_ENTRIES(h: Handle) -> Handle {
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
            use rts_engine::heap::shapes::{handle_word_auto, string_word};
            let kw = string_word(k.as_bytes()) as i64;
            let vw = string_word(v.as_bytes()) as i64;
            handle_word_auto(alloc_entry(Entry::Vec(Box::new(vec![kw, vw])))) as i64
        })
        .collect();
    alloc_entry(Entry::Vec(Box::new(out)))
}

/// h.keys() — Vec de string handles.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_HEADERS_KEYS(h: Handle) -> Handle {
    let keys: Vec<String> = with_entry(h, |e| match e {
        Some(Entry::Headers(map)) => map.keys().cloned().collect(),
        _ => Vec::new(),
    });
    let out: Vec<i64> = keys
        .into_iter()
        .map(|k| rts_engine::heap::shapes::string_word(k.as_bytes()) as i64)
        .collect();
    alloc_entry(Entry::Vec(Box::new(out)))
}

/// h.values() — Vec de string handles juntados por ", ".
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_HEADERS_VALUES(h: Handle) -> Handle {
    let vals: Vec<String> = with_entry(h, |e| match e {
        Some(Entry::Headers(map)) => map.values().map(|vs| vs.join(", ")).collect(),
        _ => Vec::new(),
    });
    let out: Vec<i64> = vals
        .into_iter()
        .map(|v| rts_engine::heap::shapes::string_word(v.as_bytes()) as i64)
        .collect();
    alloc_entry(Entry::Vec(Box::new(out)))
}

/// Membro de classe global (helper hand-written, espelha `leak_class` da macro).
#[allow(clippy::too_many_arguments)]
fn m(
    name: &str,
    kind: MemberKind,
    sig: Sig,
    symbol: &str,
    ts: &str,
    doc: &str,
    fp: *const u8,
    pure: bool,
) -> Member {
    Member {
        name: name.to_string(),
        kind,
        sig,
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: doc.to_string(),
        pure,
        intrinsic: None,
        emit: None,
    }
}

/// Registra a classe global `Headers` no motor (Fase 2 — hand-written, sem macro).
pub fn register_headers_class_spec(e: &mut Engine) {
    e.class("Headers")
        .doc("Headers — Fetch API multimap case-insensitive de header name -> valores.")
        .member(m(
            "new",
            MemberKind::Constructor,
            Sig::new(Vec::new(), AbiType::Handle),
            "__RTS_FN_GL_HEADERS_NEW",
            "new Headers()",
            "new Headers() — vazio.",
            __RTS_FN_GL_HEADERS_NEW as *const u8,
            true,
        ))
        .member(m(
            "new",
            MemberKind::Constructor,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_HEADERS_NEW_FROM",
            "new Headers(init: [string, string][])",
            "new Headers(arr) — array de pares [name, value].",
            __RTS_FN_GL_HEADERS_NEW_FROM as *const u8,
            true,
        ))
        .member(m(
            "append",
            MemberKind::InstanceMethod,
            Sig::new(
                vec![AbiType::Handle, AbiType::StrPtr, AbiType::StrPtr],
                AbiType::Void,
            ),
            "__RTS_FN_GL_HEADERS_APPEND",
            "append(name: string, value: string): void",
            "h.append(name, value)",
            __RTS_FN_GL_HEADERS_APPEND as *const u8,
            false,
        ))
        .member(m(
            "set",
            MemberKind::InstanceMethod,
            Sig::new(
                vec![AbiType::Handle, AbiType::StrPtr, AbiType::StrPtr],
                AbiType::Void,
            ),
            "__RTS_FN_GL_HEADERS_SET",
            "set(name: string, value: string): void",
            "h.set(name, value) — substitui todos os valores existentes.",
            __RTS_FN_GL_HEADERS_SET as *const u8,
            false,
        ))
        .member(m(
            "get",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::StrPtr], AbiType::Handle),
            "__RTS_FN_GL_HEADERS_GET",
            "get(name: string): string | null",
            "h.get(name) — junta valores com \", \". 0 (null) se nao houver.",
            __RTS_FN_GL_HEADERS_GET as *const u8,
            true,
        ))
        .member(m(
            "has",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::StrPtr], AbiType::Bool),
            "__RTS_FN_GL_HEADERS_HAS",
            "has(name: string): boolean",
            "h.has(name) — bool.",
            __RTS_FN_GL_HEADERS_HAS as *const u8,
            true,
        ))
        .member(m(
            "delete",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::StrPtr], AbiType::Void),
            "__RTS_FN_GL_HEADERS_DELETE",
            "delete(name: string): void",
            "h.delete(name)",
            __RTS_FN_GL_HEADERS_DELETE as *const u8,
            false,
        ))
        .member(m(
            "getSetCookie",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_HEADERS_GET_SET_COOKIE",
            "getSetCookie(): string[]",
            "h.getSetCookie() — Vec dos valores raw de \"set-cookie\", sem juntar.",
            __RTS_FN_GL_HEADERS_GET_SET_COOKIE as *const u8,
            true,
        ))
        .member(m(
            "entries",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_HEADERS_ENTRIES",
            "entries(): IterableIterator<[string, string]>",
            "h.entries() — Vec de [name_handle, joined_value_handle].",
            __RTS_FN_GL_HEADERS_ENTRIES as *const u8,
            true,
        ))
        .member(m(
            "keys",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_HEADERS_KEYS",
            "keys(): IterableIterator<string>",
            "h.keys() — Vec de string handles.",
            __RTS_FN_GL_HEADERS_KEYS as *const u8,
            true,
        ))
        .member(m(
            "values",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_HEADERS_VALUES",
            "values(): IterableIterator<string>",
            "h.values() — Vec de string handles juntados por \", \".",
            __RTS_FN_GL_HEADERS_VALUES as *const u8,
            true,
        ))
        .done();
}
