//! `FormData` global class (#72).
//!
//! Multimap case-sensitive de form fields, preserva ordem + duplicatas.
//! Migrado do `#[rts_class]` (macro) pro modelo builder hand-written do
//! `rts-engine` (rumo à remoção da `rts-macro`). Os externs
//! `__RTS_FN_GL_FORM_DATA_*` + `register_form_data_class_spec()` são escritos
//! à mão. Storage: Map com `__pairs` -> Vec<i64> alternando key_h, val_h.

use indexmap::IndexMap;

use rts_engine::abi::ty::{Bool, Handle};
use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

use rts_engine::heap::handles::{alloc_entry, with_entry, with_entry_mut, Entry};

/// Storage interno: handle do Vec<i64> de pares (key_h, val_h).
fn pairs_handle(h: u64) -> u64 {
    with_entry(h, |e| match e {
        Some(Entry::Map(m)) => m.get("__pairs").copied().unwrap_or(0) as u64,
        _ => 0,
    })
}

/// new FormData()
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_FORM_DATA_NEW() -> Handle {
    let pairs = alloc_entry(Entry::Vec(Box::new(Vec::new())));
    let mut m: IndexMap<String, i64> = IndexMap::new();
    m.insert("__pairs".to_string(), pairs as i64);
    m.insert("__rts_class".to_string(), {
        alloc_entry(Entry::String(b"FormData".to_vec())) as i64
    });
    alloc_entry(Entry::Map(Box::new(m)))
}

/// fd.append(name, value)
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_FORM_DATA_APPEND(
    h: Handle,
    name_ptr: *const u8,
    name_len: i64,
    value_ptr: *const u8,
    value_len: i64,
) {
    let name = unsafe { rts_engine::abi::str_abi::from_abi(name_ptr, name_len) };
    let value = unsafe { rts_engine::abi::str_abi::from_abi(value_ptr, value_len) };
    let name = name.unwrap_or("");
    let val = value.unwrap_or("");
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
    h: Handle,
    name_ptr: *const u8,
    name_len: i64,
    value_ptr: *const u8,
    value_len: i64,
) {
    let name = unsafe { rts_engine::abi::str_abi::from_abi(name_ptr, name_len) };
    let value = unsafe { rts_engine::abi::str_abi::from_abi(value_ptr, value_len) };
    let name = name.unwrap_or("").to_string();
    let val = value.unwrap_or("").to_string();
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
                let key_str: String = with_entry(k_h, |ke| match ke {
                    Some(Entry::String(b)) => Some(String::from_utf8_lossy(b).into_owned()),
                    _ => None,
                })
                .unwrap_or_default();
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

/// fd.delete(name)
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_FORM_DATA_DELETE(h: Handle, name_ptr: *const u8, name_len: i64) {
    let name = unsafe { rts_engine::abi::str_abi::from_abi(name_ptr, name_len) };
    let name = name.unwrap_or("").to_string();
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
                let key_str: String = with_entry(k_h, |ke| match ke {
                    Some(Entry::String(b)) => Some(String::from_utf8_lossy(b).into_owned()),
                    _ => None,
                })
                .unwrap_or_default();
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
pub extern "C" fn __RTS_FN_GL_FORM_DATA_GET(h: Handle, name_ptr: *const u8, name_len: i64) -> Handle {
    let name = unsafe { rts_engine::abi::str_abi::from_abi(name_ptr, name_len) };
    let name = name.unwrap_or("").to_string();
    let pairs = pairs_handle(h);
    let v: Vec<i64> = with_entry(pairs, |e| match e {
        Some(Entry::Vec(v)) => (**v).clone(),
        _ => Vec::new(),
    });
    let mut i = 0;
    while i + 1 < v.len() {
        let k_h = v[i] as u64;
        let key_str: String = with_entry(k_h, |ke| match ke {
            Some(Entry::String(b)) => Some(String::from_utf8_lossy(b).into_owned()),
            _ => None,
        })
        .unwrap_or_default();
        if key_str == name {
            return v[i + 1] as u64;
        }
        i += 2;
    }
    0
}

/// fd.getAll(name) — Vec de string handles.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_FORM_DATA_GET_ALL(
    h: Handle,
    name_ptr: *const u8,
    name_len: i64,
) -> Handle {
    let name = unsafe { rts_engine::abi::str_abi::from_abi(name_ptr, name_len) };
    let name = name.unwrap_or("").to_string();
    let pairs = pairs_handle(h);
    let v: Vec<i64> = with_entry(pairs, |e| match e {
        Some(Entry::Vec(v)) => (**v).clone(),
        _ => Vec::new(),
    });
    let mut out: Vec<i64> = Vec::new();
    let mut i = 0;
    while i + 1 < v.len() {
        let k_h = v[i] as u64;
        let key_str: String = with_entry(k_h, |ke| match ke {
            Some(Entry::String(b)) => Some(String::from_utf8_lossy(b).into_owned()),
            _ => None,
        })
        .unwrap_or_default();
        if key_str == name {
            // Elemento como WORD (motor novo), não handle cru.
            out.push(rts_engine::heap::shapes::handle_word_auto(v[i + 1] as u64) as i64);
        }
        i += 2;
    }
    alloc_entry(Entry::Vec(Box::new(out)))
}

/// fd.has(name) — bool.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_FORM_DATA_HAS(h: Handle, name_ptr: *const u8, name_len: i64) -> Bool {
    let name = match unsafe { rts_engine::abi::str_abi::from_abi(name_ptr, name_len) } {
        Some(s) => s,
        None => return 0,
    };
    if __RTS_FN_GL_FORM_DATA_GET(h, name.as_ptr(), name.len() as i64) != 0 {
        1
    } else {
        0
    }
}

/// fd.entries() — Vec de [name_h, val_h] preservando ordem + duplicatas.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_FORM_DATA_ENTRIES(h: Handle) -> Handle {
    let pairs = pairs_handle(h);
    let v: Vec<i64> = with_entry(pairs, |e| match e {
        Some(Entry::Vec(v)) => (**v).clone(),
        _ => Vec::new(),
    });
    let mut out: Vec<i64> = Vec::with_capacity(v.len() / 2);
    let mut i = 0;
    while i + 1 < v.len() {
        use rts_engine::heap::shapes::handle_word_auto as w;
        let pair = alloc_entry(Entry::Vec(Box::new(vec![
            w(v[i] as u64) as i64,
            w(v[i + 1] as u64) as i64,
        ])));
        out.push(w(pair) as i64);
        i += 2;
    }
    alloc_entry(Entry::Vec(Box::new(out)))
}

/// fd.keys() — Vec de string handles.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_FORM_DATA_KEYS(h: Handle) -> Handle {
    let pairs = pairs_handle(h);
    let v: Vec<i64> = with_entry(pairs, |e| match e {
        Some(Entry::Vec(v)) => (**v).clone(),
        _ => Vec::new(),
    });
    let mut out: Vec<i64> = Vec::with_capacity(v.len() / 2);
    let mut i = 0;
    while i + 1 < v.len() {
        out.push(rts_engine::heap::shapes::handle_word_auto(v[i] as u64) as i64);
        i += 2;
    }
    alloc_entry(Entry::Vec(Box::new(out)))
}

/// fd.values() — Vec de string handles.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_FORM_DATA_VALUES(h: Handle) -> Handle {
    let pairs = pairs_handle(h);
    let v: Vec<i64> = with_entry(pairs, |e| match e {
        Some(Entry::Vec(v)) => (**v).clone(),
        _ => Vec::new(),
    });
    let mut out: Vec<i64> = Vec::with_capacity(v.len() / 2);
    let mut i = 0;
    while i + 1 < v.len() {
        out.push(rts_engine::heap::shapes::handle_word_auto(v[i + 1] as u64) as i64);
        i += 2;
    }
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
        emit: None,
    }
}

/// Registra a classe global `FormData` no motor (Fase 2 — hand-written, sem macro).
pub fn register_form_data_class_spec(e: &mut Engine) {
    e.class("FormData")
        .doc("FormData — multimap case-sensitive de form fields (ordem + duplicatas).")
        .member(m(
            "new",
            MemberKind::Constructor,
            Sig::new(Vec::new(), AbiType::Handle),
            "__RTS_FN_GL_FORM_DATA_NEW",
            "new FormData()",
            "new FormData()",
            __RTS_FN_GL_FORM_DATA_NEW as *const u8,
            true,
        ))
        .member(m(
            "append",
            MemberKind::InstanceMethod,
            Sig::new(
                vec![AbiType::Handle, AbiType::StrPtr, AbiType::StrPtr],
                AbiType::Void,
            ),
            "__RTS_FN_GL_FORM_DATA_APPEND",
            "append(name: string, value: string): void",
            "fd.append(name, value)",
            __RTS_FN_GL_FORM_DATA_APPEND as *const u8,
            false,
        ))
        .member(m(
            "set",
            MemberKind::InstanceMethod,
            Sig::new(
                vec![AbiType::Handle, AbiType::StrPtr, AbiType::StrPtr],
                AbiType::Void,
            ),
            "__RTS_FN_GL_FORM_DATA_SET",
            "set(name: string, value: string): void",
            "fd.set(name, value) — remove existentes do name, append novo.",
            __RTS_FN_GL_FORM_DATA_SET as *const u8,
            false,
        ))
        .member(m(
            "delete",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::StrPtr], AbiType::Void),
            "__RTS_FN_GL_FORM_DATA_DELETE",
            "delete(name: string): void",
            "fd.delete(name)",
            __RTS_FN_GL_FORM_DATA_DELETE as *const u8,
            false,
        ))
        .member(m(
            "get",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::StrPtr], AbiType::Handle),
            "__RTS_FN_GL_FORM_DATA_GET",
            "get(name: string): string | null",
            "fd.get(name) — primeiro value, handle de string ou 0 (null).",
            __RTS_FN_GL_FORM_DATA_GET as *const u8,
            true,
        ))
        .member(m(
            "getAll",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::StrPtr], AbiType::Handle),
            "__RTS_FN_GL_FORM_DATA_GET_ALL",
            "getAll(name: string): string[]",
            "fd.getAll(name) — Vec de string handles.",
            __RTS_FN_GL_FORM_DATA_GET_ALL as *const u8,
            true,
        ))
        .member(m(
            "has",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::StrPtr], AbiType::Bool),
            "__RTS_FN_GL_FORM_DATA_HAS",
            "has(name: string): boolean",
            "fd.has(name) — bool.",
            __RTS_FN_GL_FORM_DATA_HAS as *const u8,
            true,
        ))
        .member(m(
            "entries",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_FORM_DATA_ENTRIES",
            "entries(): IterableIterator<[string, string]>",
            "fd.entries() — Vec de [name_h, val_h] preservando ordem + duplicatas.",
            __RTS_FN_GL_FORM_DATA_ENTRIES as *const u8,
            true,
        ))
        .member(m(
            "keys",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_FORM_DATA_KEYS",
            "keys(): IterableIterator<string>",
            "fd.keys() — Vec de string handles.",
            __RTS_FN_GL_FORM_DATA_KEYS as *const u8,
            true,
        ))
        .member(m(
            "values",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_FORM_DATA_VALUES",
            "values(): IterableIterator<string>",
            "fd.values() — Vec de string handles.",
            __RTS_FN_GL_FORM_DATA_VALUES as *const u8,
            true,
        ))
        .done();
}
