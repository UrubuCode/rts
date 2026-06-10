//! `FormData` global class (#72).
//!
//! Multimap case-sensitive de form fields, preserva ordem + duplicatas.
//! Migrado ao modelo `#[rts_class]` (stage 5). Storage: Map com `__pairs` ->
//! Vec<i64> alternando key_h, val_h.

use indexmap::IndexMap;

use rts_engine::abi::ty::{Bool, Handle};
use rts_macro::rts_class;

use crate::namespaces::gc::handles::{alloc_entry, with_entry, with_entry_mut, Entry};

/// Storage interno: handle do Vec<i64> de pares (key_h, val_h).
fn pairs_handle(h: u64) -> u64 {
    with_entry(h, |e| match e {
        Some(Entry::Map(m)) => m.get("__pairs").copied().unwrap_or(0) as u64,
        _ => 0,
    })
}

/// FormData — multimap case-sensitive de form fields (ordem + duplicatas).
#[rts_class(FormData, prefix = "FORM_DATA", spec = "FORM_DATA_CLASS_SPEC")]
impl FormDataClass {
    /// new FormData()
    #[rts_ctor(ts = "new FormData()", pure)]
    pub fn new() -> Handle {
        let pairs = alloc_entry(Entry::Vec(Box::new(Vec::new())));
        let mut m: IndexMap<String, i64> = IndexMap::new();
        m.insert("__pairs".to_string(), pairs as i64);
        m.insert("__rts_class".to_string(), {
            alloc_entry(Entry::String(b"FormData".to_vec())) as i64
        });
        alloc_entry(Entry::Map(Box::new(m)))
    }

    /// fd.append(name, value)
    #[rts_method(ts = "append(name: string, value: string): void", opt_str)]
    pub fn append(h: Handle, name: Str, value: Str) {
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
    #[rts_method(ts = "set(name: string, value: string): void", opt_str)]
    pub fn set(h: Handle, name: Str, value: Str) {
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
    #[rts_method(ts = "delete(name: string): void", opt_str)]
    pub fn delete(h: Handle, name: Str) {
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
    #[rts_method(ts = "get(name: string): string | null", opt_str, pure)]
    pub fn get(h: Handle, name: Str) -> Handle {
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
    #[rts_method(name = "getAll", ts = "getAll(name: string): string[]", opt_str, pure)]
    pub fn get_all(h: Handle, name: Str) -> Handle {
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
                out.push(v[i + 1]);
            }
            i += 2;
        }
        alloc_entry(Entry::Vec(Box::new(out)))
    }

    /// fd.has(name) — bool.
    #[rts_method(ts = "has(name: string): boolean", pure)]
    pub fn has(h: Handle, name: Str) -> Bool {
        if __RTS_FN_GL_FORM_DATA_GET(h, name.as_ptr(), name.len() as i64) != 0 {
            1
        } else {
            0
        }
    }

    /// fd.entries() — Vec de [name_h, val_h] preservando ordem + duplicatas.
    #[rts_method(ts = "entries(): IterableIterator<[string, string]>", pure)]
    pub fn entries(h: Handle) -> Handle {
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

    /// fd.keys() — Vec de string handles.
    #[rts_method(ts = "keys(): IterableIterator<string>", pure)]
    pub fn keys(h: Handle) -> Handle {
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

    /// fd.values() — Vec de string handles.
    #[rts_method(ts = "values(): IterableIterator<string>", pure)]
    pub fn values(h: Handle) -> Handle {
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
}
