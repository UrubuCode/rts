//! `RegExp` global class — constructor and instance method implementations.
//!
//! Constructors delegate to `__RTS_FN_NS_REGEX_COMPILE` (which accepts flags).
//! Instance methods delegate to the existing `regex` namespace ops.

use crate::namespaces::gc::handles::{Entry, alloc_entry, with_entry, with_entry_mut};

// ── Helpers ───────────────────────────────────────────────────────────────────

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
            let start = if rx.global { rx.last_index } else { 0 };
            if start > s_full.len() {
                rx.last_index = 0;
                return None;
            }
            if !s_full.is_char_boundary(start) {
                rx.last_index = 0;
                return None;
            }
            let caps_opt = rx.regex.captures(&s_full[start..]);
            let Some(caps) = caps_opt else {
                if rx.global { rx.last_index = 0; }
                return None;
            };
            let m0 = caps.get(0).unwrap();
            let abs_start = start + m0.start();
            let abs_end = start + m0.end();
            if rx.global {
                rx.last_index = abs_end;
            }
            // Coleta captures (0..N).
            let mut groups_vec: Vec<Option<String>> = Vec::with_capacity(caps.len());
            // (#regex-d) Coleta indices [start, end] absolutos por capture
            // quando flag 'd' (hasIndices) esta setada.
            let has_indices = rx.flags.contains('d');
            let mut indices_vec: Vec<Option<(usize, usize)>> = Vec::with_capacity(caps.len());
            for i in 0..caps.len() {
                let m = caps.get(i);
                groups_vec.push(m.map(|mm| mm.as_str().to_string()));
                indices_vec.push(m.map(|mm| (start + mm.start(), start + mm.end())));
            }
            // Coleta named groups.
            let mut named_groups: Vec<(String, Option<String>)> = Vec::new();
            for name in rx.regex.capture_names().flatten() {
                let m = caps.name(name).map(|m| m.as_str().to_string());
                named_groups.push((name.to_string(), m));
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
    // (#regex-d) indices: Vec<[start, end]> quando flag 'd' setada;
    // undefined caso contrario. JSON.stringify renderiza como
    // `[[s,e],[s,e],...]`.
    let indices_v = match indices {
        Some(vec) => {
            let mut outer: Vec<i64> = Vec::with_capacity(vec.len());
            for opt in vec {
                let pair_h: i64 = match opt {
                    Some((s, e)) => {
                        let pair = vec![s as i64, e as i64];
                        alloc_entry(Entry::Vec(Box::new(pair))) as i64
                    }
                    None => i64::MIN + 2,
                };
                outer.push(pair_h);
            }
            alloc_entry(Entry::Vec(Box::new(outer))) as i64
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
        Some(Entry::Regex(rx)) => Some(rx.regex.as_str().to_owned()),
        _ => None,
    });
    match source {
        Some(s) if s.is_empty() => alloc_entry(Entry::String(b"(?:)".to_vec())),
        Some(s) => alloc_entry(Entry::String(s.into_bytes())),
        None => 0,
    }
}
