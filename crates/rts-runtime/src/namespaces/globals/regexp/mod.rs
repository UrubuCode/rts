//! `RegExp` global class — backed by the Rust `regex` crate (RE2 semantics).
//!
//! Migrado ao modelo `#[rts_class]` (stage 5, `docs/specs/rts-core-engine.md`):
//! um unico `impl` declara construtores, instance methods e getters; o macro
//! deriva os externs `__RTS_FN_GL_REGEXP_*` + o `REGEXP_CLASS_SPEC`. As fns
//! `__RTS_FN_GL_REGEXP_LAST_INDEX_SET` e `__RTS_FN_GL_REGEXP_INDICES_GROUPS`
//! NAO sao membros da classe (chamadas pelo codegen por simbolo) e ficam como
//! free fns abaixo, junto dos helpers da side-table.

use rts_abi::ty::{Bool, Handle, I64};
use rts_macro::rts_class;

use crate::namespaces::gc::handles::{alloc_entry, with_entry, with_entry_mut, Entry};

// ── Helpers (side-table indices_vec_handle -> groups_map_handle) ───────────────

/// (cross-runtime #70/#1162) Side-table indices_vec_handle -> groups_map_handle.
static INDICES_GROUPS: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<u64, u64>>> =
    std::sync::OnceLock::new();

fn indices_groups_table() -> &'static std::sync::Mutex<std::collections::HashMap<u64, u64>> {
    INDICES_GROUPS.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

fn register_indices_groups(indices_vec: u64, groups_map: u64) {
    if let Ok(mut t) = indices_groups_table().lock() {
        t.insert(indices_vec, groups_map);
    }
}

/// Built-in RegExp class. Backed by the Rust `regex` crate (RE2 semantics).
#[rts_class(RegExp)]
impl RegExpClass {
    /// `new RegExp(pattern)` — no flags.
    #[rts_ctor(ts = "new RegExp(pattern: string): RegExp")]
    pub fn new(pattern: Str) -> Handle {
        crate::namespaces::regex::__RTS_FN_NS_REGEX_COMPILE(
            pattern.as_ptr(),
            pattern.len() as i64,
            "".as_ptr(),
            0,
        )
    }

    /// `new RegExp(pattern, flags)` — with flags like "gi", "im", "s".
    #[rts_ctor(
        symbol = "__RTS_FN_GL_REGEXP_NEW_WITH_FLAGS",
        ts = "new RegExp(pattern: string, flags: string): RegExp"
    )]
    pub fn new_with_flags(pattern: Str, flags: Str) -> Handle {
        crate::namespaces::regex::__RTS_FN_NS_REGEX_COMPILE(
            pattern.as_ptr(),
            pattern.len() as i64,
            flags.as_ptr(),
            flags.len() as i64,
        )
    }

    /// `re.test(str)` — returns 1 if match, 0 otherwise.
    #[rts_method(ts = "test(str: string): boolean", pure)]
    pub fn test(handle: Handle, s: Str) -> Bool {
        crate::namespaces::regex::__RTS_FN_NS_REGEX_TEST(handle, s.as_ptr(), s.len() as i64)
    }

    /// `re.exec(str)` — JS Array-like (Map) com matched + captures + groups.
    #[rts_method(ts = "exec(str: string): string | null", pure)]
    pub fn exec(handle: Handle, s: Str) -> Handle {
        use indexmap::IndexMap;
        let s_full = s.to_string();
        let result = with_entry_mut(handle, |entry| {
            if let Some(Entry::Regex(rx)) = entry {
                let sticky = rx.flags.contains('y');
                let use_last_idx = rx.global || sticky;
                let start = if use_last_idx { rx.last_index } else { 0 };
                if start > s_full.len() {
                    rx.last_index = 0;
                    return None;
                }
                if !s_full.is_char_boundary(start) {
                    rx.last_index = 0;
                    return None;
                }
                let caps_opt = rx.engine.captures(&s_full[start..]);
                let Some(caps) = caps_opt else {
                    if use_last_idx {
                        rx.last_index = 0;
                    }
                    return None;
                };
                let m0 = match caps.groups.first().and_then(|o| o.clone()) {
                    Some(m) => m,
                    None => {
                        if use_last_idx {
                            rx.last_index = 0;
                        }
                        return None;
                    }
                };
                if sticky && m0.start != 0 {
                    rx.last_index = 0;
                    return None;
                }
                let abs_start = start + m0.start;
                let abs_end = start + m0.end;
                if use_last_idx {
                    rx.last_index = abs_end;
                }
                let has_indices = rx.flags.contains('d');
                let mut groups_vec: Vec<Option<String>> = Vec::with_capacity(caps.groups.len());
                let mut indices_vec: Vec<Option<(usize, usize)>> =
                    Vec::with_capacity(caps.groups.len());
                for opt in &caps.groups {
                    groups_vec.push(opt.as_ref().map(|m| m.text.clone()));
                    indices_vec.push(opt.as_ref().map(|m| (start + m.start, start + m.end)));
                }
                let names = rx.engine.capture_names();
                let mut named_groups: Vec<(String, Option<String>)> = Vec::new();
                for (i, name_opt) in names.iter().enumerate() {
                    if let Some(name) = name_opt {
                        let text = caps
                            .groups
                            .get(i)
                            .and_then(|o| o.as_ref())
                            .map(|m| m.text.clone());
                        named_groups.push((name.clone(), text));
                    }
                }
                let indices = if has_indices { Some(indices_vec) } else { None };
                Some((groups_vec, named_groups, abs_start, s_full.clone(), indices))
            } else {
                None
            }
        });
        let Some((groups_vec, named_groups, idx, input, indices)) = result else {
            return 0;
        };
        let mut map: IndexMap<String, i64> = IndexMap::new();
        for (i, opt) in groups_vec.iter().enumerate() {
            let v = match opt {
                Some(s) => alloc_entry(Entry::String(s.clone().into_bytes())) as i64,
                None => i64::MIN + 2,
            };
            map.insert(i.to_string(), v);
        }
        map.insert("length".to_string(), groups_vec.len() as i64);
        map.insert("index".to_string(), idx as i64);
        let input_h = alloc_entry(Entry::String(input.into_bytes())) as i64;
        map.insert("input".to_string(), input_h);
        let groups_v = if named_groups.is_empty() {
            i64::MIN + 2
        } else {
            let mut gmap: IndexMap<String, i64> = IndexMap::new();
            for (name, opt) in named_groups {
                let v = match opt {
                    Some(s) => alloc_entry(Entry::String(s.into_bytes())) as i64,
                    None => i64::MIN + 2,
                };
                gmap.insert(name, v);
            }
            alloc_entry(Entry::Map(Box::new(gmap))) as i64
        };
        map.insert("groups".to_string(), groups_v);
        let indices_v = match indices.clone() {
            Some(vec) => {
                let mut ivec: Vec<i64> = Vec::with_capacity(vec.len());
                for opt in vec.iter() {
                    let pair_h: i64 = match opt {
                        Some((s, e)) => {
                            let pair = vec![*s as i64, *e as i64];
                            alloc_entry(Entry::Vec(Box::new(pair))) as i64
                        }
                        None => i64::MIN + 2,
                    };
                    ivec.push(pair_h);
                }
                let vec_h = alloc_entry(Entry::Vec(Box::new(ivec)));
                let g_vec_opt: Option<Vec<(String, Option<(usize, usize)>)>> =
                    with_entry(handle, |entry| match entry {
                        Some(Entry::Regex(rx)) => {
                            let names: Vec<Option<String>> = rx
                                .regex
                                .capture_names()
                                .map(|o| o.map(|s| s.to_string()))
                                .collect();
                            let mut out: Vec<(String, Option<(usize, usize)>)> = Vec::new();
                            for (i, name_opt) in names.iter().enumerate() {
                                if let Some(name) = name_opt {
                                    let pair: Option<(usize, usize)> = vec.get(i).and_then(|o| *o);
                                    out.push((name.clone(), pair));
                                }
                            }
                            Some(out)
                        }
                        _ => None,
                    });
                if let Some(g_pairs) = g_vec_opt {
                    if !g_pairs.is_empty() {
                        let mut gmap: IndexMap<String, i64> = IndexMap::new();
                        for (name, opt) in g_pairs {
                            let pair_h: i64 = match opt {
                                Some((s, e)) => {
                                    let pair = vec![s as i64, e as i64];
                                    alloc_entry(Entry::Vec(Box::new(pair))) as i64
                                }
                                None => i64::MIN + 2,
                            };
                            gmap.insert(name, pair_h);
                        }
                        let groups_h = alloc_entry(Entry::Map(Box::new(gmap)));
                        register_indices_groups(vec_h, groups_h);
                    }
                }
                vec_h as i64
            }
            None => i64::MIN + 2,
        };
        map.insert("indices".to_string(), indices_v);
        alloc_entry(Entry::Map(Box::new(map)))
    }

    /// `re.source` — returns pattern string as a handle.
    #[rts_getter(ts = "source: string", pure)]
    pub fn source(handle: Handle) -> Handle {
        let source: Option<String> = with_entry(handle, |entry| match entry {
            Some(Entry::Regex(rx)) => Some(rx.engine.source()),
            _ => None,
        });
        match source {
            Some(s) if s.is_empty() => alloc_entry(Entry::String(b"(?:)".to_vec())),
            Some(s) => alloc_entry(Entry::String(s.into_bytes())),
            None => 0,
        }
    }

    /// (#781) `re.flags` — string canonica das flags (ex: "gi").
    #[rts_getter(ts = "flags: string", pure)]
    pub fn flags(handle: Handle) -> Handle {
        let f: Option<String> = with_entry(handle, |entry| match entry {
            Some(Entry::Regex(rx)) => Some(rx.flags.clone()),
            _ => None,
        });
        match f {
            Some(s) => alloc_entry(Entry::String(s.into_bytes())),
            None => 0,
        }
    }

    /// `re.global` — flag 'g' setada?
    #[rts_getter(ts = "global: boolean", pure)]
    pub fn global(handle: Handle) -> Bool {
        with_entry(handle, |entry| match entry {
            Some(Entry::Regex(rx)) => {
                if rx.flags.contains('g') {
                    1
                } else {
                    0
                }
            }
            _ => 0,
        })
    }

    /// `re.ignoreCase` — flag 'i' setada?
    #[rts_getter(
        name = "ignoreCase",
        symbol = "__RTS_FN_GL_REGEXP_IGNORE_CASE",
        ts = "ignoreCase: boolean",
        pure
    )]
    pub fn ignore_case(handle: Handle) -> Bool {
        with_entry(handle, |entry| match entry {
            Some(Entry::Regex(rx)) => {
                if rx.flags.contains('i') {
                    1
                } else {
                    0
                }
            }
            _ => 0,
        })
    }

    /// (#782) `re.lastIndex` getter.
    #[rts_getter(
        name = "lastIndex",
        symbol = "__RTS_FN_GL_REGEXP_LAST_INDEX_GET",
        ts = "lastIndex: number",
        pure
    )]
    pub fn last_index_get(handle: Handle) -> I64 {
        with_entry(handle, |entry| match entry {
            Some(Entry::Regex(rx)) => rx.last_index as i64,
            _ => 0,
        })
    }

    /// `re.multiline` — flag 'm' setada?
    #[rts_getter(ts = "multiline: boolean", pure)]
    pub fn multiline(handle: Handle) -> Bool {
        with_entry(handle, |entry| match entry {
            Some(Entry::Regex(rx)) => {
                if rx.flags.contains('m') {
                    1
                } else {
                    0
                }
            }
            _ => 0,
        })
    }
}

// ── Non-member externs (codegen calls by symbol). ────────────────────────────

/// (#70/#1162) Lookup: dado handle de indices Vec, retorna handle do Map de
/// named groups indices. 0 se nao registrado.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_REGEXP_INDICES_GROUPS(vec_handle: u64) -> u64 {
    indices_groups_table()
        .lock()
        .ok()
        .and_then(|t| t.get(&vec_handle).copied())
        .unwrap_or(0)
}

/// (#782) `re.lastIndex = N` — setter direto.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_REGEXP_LAST_INDEX_SET(handle: u64, n: i64) {
    let v = if n < 0 { 0 } else { n as usize };
    with_entry_mut(handle, |entry| {
        if let Some(Entry::Regex(rx)) = entry {
            rx.last_index = v;
        }
    });
}
