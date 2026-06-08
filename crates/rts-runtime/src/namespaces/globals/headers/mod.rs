//! `Headers` global class (#289).
//!
//! Multimap case-insensitive de header name -> lista de valores. Migrado ao
//! modelo `#[rts_class]` (stage 5).

use indexmap::IndexMap;

use rts_abi::ty::{Bool, Handle};
use rts_macro::rts_class;

use crate::namespaces::gc::handles::{alloc_entry, with_entry, with_entry_mut, Entry};

fn norm_name(name: &str) -> String {
    name.to_ascii_lowercase()
}

/// Headers — Fetch API multimap case-insensitive de header name -> valores.
#[rts_class(Headers)]
impl HeadersClass {
    /// new Headers() — vazio.
    #[rts_ctor(ts = "new Headers()", pure)]
    pub fn new() -> Handle {
        alloc_entry(Entry::Headers(Box::new(IndexMap::new())))
    }

    /// new Headers(arr) — array de pares [name, value].
    #[rts_ctor(
        symbol = "__RTS_FN_GL_HEADERS_NEW_FROM",
        ts = "new Headers(init: [string, string][])",
        pure
    )]
    pub fn new_from(arr_h: Handle) -> Handle {
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
    #[rts_method(ts = "append(name: string, value: string): void", opt_str)]
    pub fn append(h: Handle, name: Str, value: Str) {
        let name = norm_name(name.unwrap_or(""));
        let val = value.unwrap_or("").to_string();
        with_entry_mut(h, |e| {
            if let Some(Entry::Headers(map)) = e {
                map.entry(name.clone()).or_default().push(val.clone());
            }
        });
    }

    /// h.set(name, value) — substitui todos os valores existentes.
    #[rts_method(ts = "set(name: string, value: string): void", opt_str)]
    pub fn set(h: Handle, name: Str, value: Str) {
        let name = norm_name(name.unwrap_or(""));
        let val = value.unwrap_or("").to_string();
        with_entry_mut(h, |e| {
            if let Some(Entry::Headers(map)) = e {
                map.insert(name.clone(), vec![val.clone()]);
            }
        });
    }

    /// h.get(name) — junta valores com ", ". 0 (null) se nao houver.
    #[rts_method(ts = "get(name: string): string | null", pure)]
    pub fn get(h: Handle, name: Str) -> Handle {
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
    #[rts_method(ts = "has(name: string): boolean", pure)]
    pub fn has(h: Handle, name: Str) -> Bool {
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
    #[rts_method(ts = "delete(name: string): void", opt_str)]
    pub fn delete(h: Handle, name: Str) {
        let name = norm_name(name.unwrap_or(""));
        with_entry_mut(h, |e| {
            if let Some(Entry::Headers(map)) = e {
                map.shift_remove(&name);
            }
        });
    }

    /// h.getSetCookie() — Vec dos valores raw de "set-cookie", sem juntar.
    #[rts_method(name = "getSetCookie", ts = "getSetCookie(): string[]", pure)]
    pub fn get_set_cookie(h: Handle) -> Handle {
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

    /// h.entries() — Vec de [name_handle, joined_value_handle].
    #[rts_method(ts = "entries(): IterableIterator<[string, string]>", pure)]
    pub fn entries(h: Handle) -> Handle {
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
    #[rts_method(ts = "keys(): IterableIterator<string>", pure)]
    pub fn keys(h: Handle) -> Handle {
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
    #[rts_method(ts = "values(): IterableIterator<string>", pure)]
    pub fn values(h: Handle) -> Handle {
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
}
