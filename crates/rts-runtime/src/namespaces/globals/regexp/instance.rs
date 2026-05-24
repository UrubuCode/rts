//! `RegExp` global class — constructor and instance method implementations.
//!
//! Constructors delegate to `__RTS_FN_NS_REGEX_COMPILE` (which accepts flags).
//! Instance methods delegate to the existing `regex` namespace ops.

use crate::namespaces::gc::handles::{Entry, alloc_entry, with_entry, with_entry_mut};

// ── Helpers ───────────────────────────────────────────────────────────────────

/// (cross-runtime #70/#1162) Side-table indices_vec_handle -> groups_map_handle.
/// `match.indices` eh um Vec (Array de [s,e]); a prop adicional `.groups`
/// (named capture indices) eh resolvida via lookup nesta tabela.
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

/// (#70/#1162) Lookup: dado um handle de indices Vec, retorna o handle do
/// Map de named groups indices. 0 se nao registrado (regex sem named groups).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_REGEXP_INDICES_GROUPS(vec_handle: u64) -> u64 {
    indices_groups_table()
        .lock()
        .ok()
        .and_then(|t| t.get(&vec_handle).copied())
        .unwrap_or(0)
}

// ── Constructors ──────────────────────────────────────────────────────────────

/// `new RegExp(pattern)` — no flags.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_REGEXP_NEW(pat_ptr: i64, pat_len: i64) -> u64 {
    crate::namespaces::regex::ops::__RTS_FN_NS_REGEX_COMPILE(
        pat_ptr as *const u8,
        pat_len,
        std::ptr::null(),
        0,
    )
}

/// `new RegExp(pattern, flags)` — with flags like "gi", "im", "s".
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_REGEXP_NEW_WITH_FLAGS(
    pat_ptr: i64,
    pat_len: i64,
    flag_ptr: i64,
    flag_len: i64,
) -> u64 {
    crate::namespaces::regex::ops::__RTS_FN_NS_REGEX_COMPILE(
        pat_ptr as *const u8,
        pat_len,
        flag_ptr as *const u8,
        flag_len,
    )
}

// ── Instance methods ──────────────────────────────────────────────────────────

/// `re.test(str)` — returns 1 if match, 0 otherwise.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_REGEXP_TEST(handle: u64, ptr: i64, len: i64) -> i64 {
    crate::namespaces::regex::ops::__RTS_FN_NS_REGEX_TEST(
        handle,
        ptr as *const u8,
        len,
    )
}

/// `re.exec(str)` — JS spec: retorna Array com matched + captures, e
/// `groups` quando ha named captures. Para reproduzir Array com props
/// adicionais, alocamos Map com slots "0", "1", ..., "length", "index",
/// "input", "groups". Codegen Vec acessa `arr[0]` via INDEX_GET_AUTO
/// que ja' faz fallback para Map keys quando handle nao eh Vec.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_REGEXP_EXEC(handle: u64, ptr: i64, len: i64) -> u64 {
    use indexmap::IndexMap;
    if ptr == 0 || len < 0 { return 0; }
    let s_full = unsafe {
        let slice = std::slice::from_raw_parts(ptr as *const u8, len as usize);
        match std::str::from_utf8(slice) {
            Ok(s) => s.to_string(),
            Err(_) => return 0,
        }
    };
    // Acessa Regex sob mut (para atualizar last_index em global) e captura
    // groups.
    let result = with_entry_mut(handle, |entry| {
        if let Some(Entry::Regex(rx)) = entry {
            // (#1086) Sticky flag (y) tambem usa lastIndex; alem disso,
            // sticky exige que o match aconteca EXATAMENTE em lastIndex
            // (sem skip de chars).
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
                if use_last_idx { rx.last_index = 0; }
                return None;
            };
            let m0 = match caps.groups.first().and_then(|o| o.clone()) {
                Some(m) => m,
                None => {
                    if use_last_idx { rx.last_index = 0; }
                    return None;
                }
            };
            // Sticky: match precisa comecar em pos=0 (relativo ao start absoluto).
            if sticky && m0.start != 0 {
                rx.last_index = 0;
                return None;
            }
            let abs_start = start + m0.start;
            let abs_end = start + m0.end;
            if use_last_idx {
                rx.last_index = abs_end;
            }
            // Coleta captures (0..N).
            let has_indices = rx.flags.contains('d');
            let mut groups_vec: Vec<Option<String>> = Vec::with_capacity(caps.groups.len());
            let mut indices_vec: Vec<Option<(usize, usize)>> = Vec::with_capacity(caps.groups.len());
            for opt in &caps.groups {
                groups_vec.push(opt.as_ref().map(|m| m.text.clone()));
                indices_vec.push(opt.as_ref().map(|m| (start + m.start, start + m.end)));
            }
            // Coleta named groups.
            let names = rx.engine.capture_names();
            let mut named_groups: Vec<(String, Option<String>)> = Vec::new();
            for (i, name_opt) in names.iter().enumerate() {
                if let Some(name) = name_opt {
                    let text = caps.groups.get(i).and_then(|o| o.as_ref()).map(|m| m.text.clone());
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
    // Monta Map com slots "0".."N", "length", "index", "input", "groups".
    let mut map: IndexMap<String, i64> = IndexMap::new();
    for (i, opt) in groups_vec.iter().enumerate() {
        let v = match opt {
            Some(s) => alloc_entry(Entry::String(s.clone().into_bytes())) as i64,
            None => i64::MIN + 2, // undefined sentinel
        };
        map.insert(i.to_string(), v);
    }
    map.insert("length".to_string(), groups_vec.len() as i64);
    map.insert("index".to_string(), idx as i64);
    let input_h = alloc_entry(Entry::String(input.into_bytes())) as i64;
    map.insert("input".to_string(), input_h);
    // groups: Map<name, string|undefined> ou undefined se sem named.
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
    // (#regex-d / #1087) indices: Map-like com slots "0".."N",
    // "length" e "groups" quando flag 'd' setada; undefined caso
    // contrario. Slots [start, end] como Vec inner. JS spec espelha
    // a forma do match (Array-like com prop extra "groups": Map<name,
    // [start,end]>).
    // (cross-runtime #70/#1162) JS spec: `match.indices` eh Array de
    // [start,end] | undefined COM prop adicional `.groups` (Map de named
    // capture indices). Usamos Entry::Vec + side-table de "groups por
    // vec handle" para suportar ambos os patterns:
    //   - `m.indices[i]` e `m.indices.length` => Vec direto
    //   - `m.indices.groups.name` => lookup via REGEXP_INDICES_GROUPS_GET
    //   - `JSON.stringify(m.indices)` => Vec serializa como array
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
            // Registra named groups indices em side-table.
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
                                let pair: Option<(usize, usize)> =
                                    vec.get(i).and_then(|o| *o);
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

/// (#781) `re.flags` — string canonica das flags (ex: "gi").
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_REGEXP_FLAGS(handle: u64) -> u64 {
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
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_REGEXP_GLOBAL(handle: u64) -> i64 {
    with_entry(handle, |entry| match entry {
        Some(Entry::Regex(rx)) => if rx.flags.contains('g') { 1 } else { 0 },
        _ => 0,
    })
}

/// `re.ignoreCase` — flag 'i' setada?
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_REGEXP_IGNORE_CASE(handle: u64) -> i64 {
    with_entry(handle, |entry| match entry {
        Some(Entry::Regex(rx)) => if rx.flags.contains('i') { 1 } else { 0 },
        _ => 0,
    })
}

/// `re.multiline` — flag 'm' setada?
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_REGEXP_MULTILINE(handle: u64) -> i64 {
    with_entry(handle, |entry| match entry {
        Some(Entry::Regex(rx)) => if rx.flags.contains('m') { 1 } else { 0 },
        _ => 0,
    })
}

/// (#782) `re.lastIndex` — getter retorna posicao do proximo `exec`/`test`
/// em regex global (ou 0 em regex nao-global).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_REGEXP_LAST_INDEX_GET(handle: u64) -> i64 {
    with_entry(handle, |entry| match entry {
        Some(Entry::Regex(rx)) => rx.last_index as i64,
        _ => 0,
    })
}

/// (#782) `re.lastIndex = N` — setter direto. JS spec aceita qualquer
/// numero; clamps para >= 0 e armazena como usize.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_REGEXP_LAST_INDEX_SET(handle: u64, n: i64) {
    let v = if n < 0 { 0 } else { n as usize };
    with_entry_mut(handle, |entry| {
        if let Some(Entry::Regex(rx)) = entry {
            rx.last_index = v;
        }
    });
}

/// `re.source` — returns pattern string as a handle.
/// JS spec: empty pattern returns `"(?:)"` (RegExp.prototype.source default).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_REGEXP_SOURCE(handle: u64) -> u64 {
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
