//! Busca em strings: contains / starts_with / ends_with / find.
//!
//! StrPtr no limite ABI ja entrega (ptr, len); codegen expande
//! automaticamente para dois slots i64. Retorno `Bool` vira i8/i64 na
//! convencao Cranelift; aqui retornamos i64 direto (0/1).

fn str_from_abi<'a>(ptr: *const u8, len: i64) -> Option<&'a str> {
    if ptr.is_null() || len < 0 {
        return None;
    }
    // SAFETY: caller contract — UTF-8 valido cobrindo `len` bytes.
    let slice = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    std::str::from_utf8(slice).ok()
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_STRING_CONTAINS(
    h_ptr: *const u8,
    h_len: i64,
    n_ptr: *const u8,
    n_len: i64,
) -> i64 {
    match (str_from_abi(h_ptr, h_len), str_from_abi(n_ptr, n_len)) {
        (Some(h), Some(n)) => h.contains(n) as i64,
        _ => 0,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_STRING_STARTS_WITH(
    s_ptr: *const u8,
    s_len: i64,
    p_ptr: *const u8,
    p_len: i64,
) -> i64 {
    match (str_from_abi(s_ptr, s_len), str_from_abi(p_ptr, p_len)) {
        (Some(s), Some(p)) => s.starts_with(p) as i64,
        _ => 0,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_STRING_ENDS_WITH(
    s_ptr: *const u8,
    s_len: i64,
    p_ptr: *const u8,
    p_len: i64,
) -> i64 {
    match (str_from_abi(s_ptr, s_len), str_from_abi(p_ptr, p_len)) {
        (Some(s), Some(p)) => s.ends_with(p) as i64,
        _ => 0,
    }
}

/// Indice byte da primeira ocorrencia, ou -1.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_STRING_FIND(
    s_ptr: *const u8,
    s_len: i64,
    n_ptr: *const u8,
    n_len: i64,
) -> i64 {
    match (str_from_abi(s_ptr, s_len), str_from_abi(n_ptr, n_len)) {
        (Some(s), Some(n)) => match s.find(n) {
            Some(idx) => idx as i64,
            None => -1,
        },
        _ => -1,
    }
}

/// (#208) `str.match(pattern)` — primeiro match, retorna handle de string
/// com o conteudo encontrado, ou 0 se nao acha. Pattern e' string regex.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_STRING_MATCH(
    s_ptr: *const u8,
    s_len: i64,
    p_ptr: *const u8,
    p_len: i64,
) -> u64 {
    use crate::namespaces::gc::handles::{Entry, alloc_entry};
    let (Some(s), Some(p)) = (str_from_abi(s_ptr, s_len), str_from_abi(p_ptr, p_len)) else {
        return 0;
    };
    let Ok(rx) = regex::Regex::new(p) else {
        return 0;
    };
    match rx.find(s) {
        Some(m) => alloc_entry(Entry::String(m.as_str().as_bytes().to_vec())),
        None => 0,
    }
}

/// `str.match(regex_handle)` — variante que aceita handle de
/// Entry::Regex (de literal /pat/ ou new RegExp). Se nao for regex
/// valido, retorna 0.
/// - Sem flag `g`: retorna Vec [fullMatch, ...grupos] (ou 0 se nao acha).
/// - Com flag `g`: retorna Vec flat de todos os fullMatches.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_STRING_MATCH_REGEX(
    s_ptr: *const u8,
    s_len: i64,
    regex_handle: u64,
) -> u64 {
    use crate::namespaces::gc::handles::{with_entry, Entry, alloc_entry};
    let Some(s) = str_from_abi(s_ptr, s_len) else {
        return 0;
    };
    let s_owned = s.to_string();
    enum MatchResultExt {
        First {
            items: Vec<Option<Vec<u8>>>,
            index: usize,
            names: Vec<Option<String>>,
        },
        All(Vec<Vec<u8>>),
        None,
    }
    let mr: MatchResultExt = with_entry(regex_handle, |e| match e {
        Some(Entry::Regex(rts_rx)) => {
            if rts_rx.global {
                let all: Vec<Vec<u8>> = rts_rx.regex
                    .find_iter(&s_owned)
                    .map(|m| m.as_str().as_bytes().to_vec())
                    .collect();
                MatchResultExt::All(all)
            } else {
                match rts_rx.regex.captures(&s_owned) {
                    Some(caps) => {
                        let index = caps.get(0).map(|m| m.start()).unwrap_or(0);
                        let items: Vec<Option<Vec<u8>>> = (0..caps.len())
                            .map(|i| caps.get(i).map(|m| m.as_str().as_bytes().to_vec()))
                            .collect();
                        let names: Vec<Option<String>> = rts_rx
                            .regex
                            .capture_names()
                            .map(|n| n.map(|s| s.to_string()))
                            .collect();
                        MatchResultExt::First { items, index, names }
                    }
                    None => MatchResultExt::None,
                }
            }
        }
        _ => MatchResultExt::None,
    });
    match mr {
        MatchResultExt::First { items, index, names } => {
            if items.is_empty() { return 0; }
            // Retorna Map com chaves "0","1",... + "index" + "input" + "length" + "groups".
            let mut map: indexmap::IndexMap<String, i64> = indexmap::IndexMap::new();
            for (i, opt) in items.iter().enumerate() {
                let h: i64 = match opt {
                    Some(bytes) => alloc_entry(Entry::String(bytes.clone())) as i64,
                    None => i64::MIN + 2,
                };
                map.insert(i.to_string(), h);
            }
            map.insert("length".to_string(), items.len() as i64);
            map.insert("index".to_string(), index as i64);
            let input_h = alloc_entry(Entry::String(s_owned.into_bytes())) as i64;
            map.insert("input".to_string(), input_h);
            // (#1086) groups: Map<name, string|undefined> ou undefined.
            let any_named = names.iter().any(|n| n.is_some());
            let groups_v: i64 = if !any_named {
                i64::MIN + 2
            } else {
                let mut gmap: indexmap::IndexMap<String, i64> = indexmap::IndexMap::new();
                for (i, n) in names.iter().enumerate() {
                    if let Some(name) = n {
                        let v: i64 = match items.get(i).and_then(|o| o.as_ref()) {
                            Some(bytes) => alloc_entry(Entry::String(bytes.clone())) as i64,
                            None => i64::MIN + 2,
                        };
                        gmap.insert(name.clone(), v);
                    }
                }
                alloc_entry(Entry::Map(Box::new(gmap))) as i64
            };
            map.insert("groups".to_string(), groups_v);
            alloc_entry(Entry::Map(Box::new(map)))
        }
        MatchResultExt::All(items) => {
            if items.is_empty() { return 0; }
            let slots: Vec<i64> = items
                .into_iter()
                .map(|bytes| alloc_entry(Entry::String(bytes)) as i64)
                .collect();
            alloc_entry(Entry::Vec(Box::new(slots)))
        }
        MatchResultExt::None => 0,
    }
}

/// `str.search(regex_handle)` — index do primeiro match, -1 se nao
/// acha. Aceita Entry::Regex direto.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_STRING_SEARCH_REGEX(
    s_ptr: *const u8,
    s_len: i64,
    regex_handle: u64,
) -> i64 {
    use crate::namespaces::gc::handles::{with_entry, Entry};
    let Some(s) = str_from_abi(s_ptr, s_len) else {
        return -1;
    };
    let s_owned = s.to_string();
    with_entry(regex_handle, |e| match e {
        Some(Entry::Regex(rx)) => rx.regex.find(&s_owned).map(|m| m.start() as i64).unwrap_or(-1),
        _ => -1,
    })
}

/// `str.replace(/pat/g, replacement)` — substitui todos (g flag) ou
/// primeiro match. Aceita handle Entry::Regex. Replacement eh string.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_STRING_REPLACE_REGEX(
    s_ptr: *const u8,
    s_len: i64,
    regex_handle: u64,
    replacement_h: u64,
) -> u64 {
    use crate::namespaces::gc::handles::{with_entry, Entry, alloc_entry};
    let Some(s) = str_from_abi(s_ptr, s_len) else {
        return 0;
    };
    let s_owned = s.to_string();

    // Le replacement string + flag global.
    let repl = with_entry(replacement_h, |e| match e {
        Some(Entry::String(b)) => Some(String::from_utf8_lossy(b).into_owned()),
        _ => None,
    }).unwrap_or_default();
    // (#1086) JS spec: `$<name>` interpola o named capture group; Rust
    // regex crate usa `${name}`. Converte sintaxe antes de chamar replace.
    fn js_to_rust_replacement(s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        let bytes = s.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'$' && i + 1 < bytes.len() && bytes[i + 1] == b'<' {
                if let Some(end) = s[i + 2..].find('>') {
                    out.push_str("${");
                    out.push_str(&s[i + 2..i + 2 + end]);
                    out.push('}');
                    i = i + 2 + end + 1;
                    continue;
                }
            }
            out.push(bytes[i] as char);
            i += 1;
        }
        out
    }
    let repl_rust = js_to_rust_replacement(&repl);

    let out = with_entry(regex_handle, |e| match e {
        Some(Entry::Regex(rx)) => {
            if rx.global {
                Some(rx.regex.replace_all(&s_owned, repl_rust.as_str()).into_owned())
            } else {
                Some(rx.regex.replace(&s_owned, repl_rust.as_str()).into_owned())
            }
        }
        _ => None,
    });
    match out {
        Some(s) => alloc_entry(Entry::String(s.into_bytes())),
        None => 0,
    }
}

/// (#208) `str.search(pattern)` — index do primeiro match, ou -1.
/// Pattern e' string regex.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_STRING_SEARCH(
    s_ptr: *const u8,
    s_len: i64,
    p_ptr: *const u8,
    p_len: i64,
) -> i64 {
    let (Some(s), Some(p)) = (str_from_abi(s_ptr, s_len), str_from_abi(p_ptr, p_len)) else {
        return -1;
    };
    let Ok(rx) = regex::Regex::new(p) else {
        return -1;
    };
    rx.find(s).map(|m| m.start() as i64).unwrap_or(-1)
}

/// (#208) `str.matchAll(pattern)` — retorna handle de Vec<u64> com handles
/// de strings, um por match. Em JS retorna iterator de RegExpExecArray; em
/// RTS v0 retorna Vec eager (cada elemento e' o conteudo do match).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_STRING_MATCH_ALL(
    s_ptr: *const u8,
    s_len: i64,
    p_ptr: *const u8,
    p_len: i64,
) -> u64 {
    use crate::namespaces::gc::handles::{Entry, alloc_entry};
    let empty_vec = || alloc_entry(Entry::Vec(Box::new(Vec::new())));
    let (Some(s), Some(p)) = (str_from_abi(s_ptr, s_len), str_from_abi(p_ptr, p_len)) else {
        return empty_vec();
    };
    let Ok(rx) = regex::Regex::new(p) else {
        return empty_vec();
    };
    let mut handles: Vec<i64> = Vec::new();
    for m in rx.find_iter(s) {
        let h = alloc_entry(Entry::String(m.as_str().as_bytes().to_vec()));
        handles.push(h as i64);
    }
    alloc_entry(Entry::Vec(Box::new(handles)))
}

/// `str.matchAll(regex_handle)` — variante que aceita handle Entry::Regex.
/// Retorna Vec de Maps (JS spec): cada match e' um Map com slots numericos
/// "0".."N", "length", "index", "input", "groups" (Map de named captures
/// ou undefined sentinel). Compativel com `m[0]`/`m.groups.name`/`m.index`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_STRING_MATCH_ALL_REGEX(
    s_ptr: *const u8,
    s_len: i64,
    regex_handle: u64,
) -> u64 {
    use crate::namespaces::gc::handles::{Entry, alloc_entry, with_entry};
    use indexmap::IndexMap;
    let empty_vec = || alloc_entry(Entry::Vec(Box::new(Vec::new())));
    let Some(s) = str_from_abi(s_ptr, s_len) else { return empty_vec(); };
    let s_owned = s.to_string();
    type MatchInfo = (
        Vec<Option<String>>,
        Vec<(String, Option<String>)>,
        usize,
        Option<Vec<Option<(usize, usize)>>>,
        Vec<Option<String>>, // names em paralelo aos indices, para .groups
    );
    let infos: Vec<MatchInfo> = with_entry(regex_handle, |e| match e {
        Some(Entry::Regex(rts_rx)) => {
            let has_indices = rts_rx.flags.contains('d');
            let names: Vec<Option<String>> = rts_rx
                .regex
                .capture_names()
                .map(|n| n.map(|s| s.to_string()))
                .collect();
            rts_rx.regex.captures_iter(&s_owned).map(|caps| {
                let groups: Vec<Option<String>> = (0..caps.len())
                    .map(|i| caps.get(i).map(|m| m.as_str().to_string()))
                    .collect();
                let named: Vec<(String, Option<String>)> = names
                    .iter()
                    .enumerate()
                    .filter_map(|(i, n)| {
                        n.as_ref().map(|name| {
                            (name.clone(), caps.get(i).map(|m| m.as_str().to_string()))
                        })
                    })
                    .collect();
                let idx = caps.get(0).map(|m| m.start()).unwrap_or(0);
                let indices = if has_indices {
                    Some(
                        (0..caps.len())
                            .map(|i| caps.get(i).map(|m| (m.start(), m.end())))
                            .collect::<Vec<_>>(),
                    )
                } else { None };
                (groups, named, idx, indices, names.clone())
            }).collect()
        }
        _ => Vec::new(),
    });
    let outer: Vec<i64> = infos.into_iter().map(|(groups, named, idx, indices_opt, names)| {
        let mut map: IndexMap<String, i64> = IndexMap::new();
        for (i, opt) in groups.iter().enumerate() {
            let v = match opt {
                Some(s) => alloc_entry(Entry::String(s.clone().into_bytes())) as i64,
                None => i64::MIN + 2,
            };
            map.insert(i.to_string(), v);
        }
        map.insert("length".to_string(), groups.len() as i64);
        map.insert("index".to_string(), idx as i64);
        let input_h = alloc_entry(Entry::String(s_owned.as_bytes().to_vec())) as i64;
        map.insert("input".to_string(), input_h);
        let groups_v = if named.is_empty() {
            i64::MIN + 2
        } else {
            let mut gmap: IndexMap<String, i64> = IndexMap::new();
            for (n, opt) in named {
                let v = match opt {
                    Some(s) => alloc_entry(Entry::String(s.into_bytes())) as i64,
                    None => i64::MIN + 2,
                };
                gmap.insert(n, v);
            }
            alloc_entry(Entry::Map(Box::new(gmap))) as i64
        };
        map.insert("groups".to_string(), groups_v);
        // (#1087) indices Map quando flag 'd' setada.
        let indices_v = match indices_opt {
            Some(vec) => {
                let mut imap: IndexMap<String, i64> = IndexMap::new();
                for (i, opt) in vec.iter().enumerate() {
                    let pair_h: i64 = match opt {
                        Some((s, e)) => alloc_entry(Entry::Vec(Box::new(vec![*s as i64, *e as i64]))) as i64,
                        None => i64::MIN + 2,
                    };
                    imap.insert(i.to_string(), pair_h);
                }
                imap.insert("length".to_string(), vec.len() as i64);
                let any_named = names.iter().any(|n| n.is_some());
                let g_v: i64 = if any_named {
                    let mut gmap: IndexMap<String, i64> = IndexMap::new();
                    for (i, name_opt) in names.iter().enumerate() {
                        if let Some(name) = name_opt {
                            let pair_h: i64 = match vec.get(i).and_then(|o| *o) {
                                Some((s, e)) => alloc_entry(Entry::Vec(Box::new(vec![s as i64, e as i64]))) as i64,
                                None => i64::MIN + 2,
                            };
                            gmap.insert(name.clone(), pair_h);
                        }
                    }
                    alloc_entry(Entry::Map(Box::new(gmap))) as i64
                } else { i64::MIN + 2 };
                imap.insert("groups".to_string(), g_v);
                alloc_entry(Entry::Map(Box::new(imap))) as i64
            }
            None => i64::MIN + 2,
        };
        map.insert("indices".to_string(), indices_v);
        alloc_entry(Entry::Map(Box::new(map))) as i64
    }).collect();
    alloc_entry(Entry::Vec(Box::new(outer)))
}

/// `str.replace(regex_handle, fn_handle)` — substitui matches chamando fn
/// para cada match. fn recebe (match, ...grupos, offset, input) — RTS v0
/// passa so o match como primeiro arg via invoke_n.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_STRING_REPLACE_REGEX_FN(
    s_ptr: *const u8,
    s_len: i64,
    regex_handle: u64,
    fn_handle: u64,
) -> u64 {
    use crate::namespaces::gc::handles::{Entry, alloc_entry, with_entry};
    let Some(s) = str_from_abi(s_ptr, s_len) else { return 0; };
    let s_owned = s.to_string();

    // Coleta matches primeiro para nao ter borrow mutavel simultaneo.
    let matches: Vec<(usize, usize, Vec<String>)> = with_entry(regex_handle, |e| match e {
        Some(Entry::Regex(rts_rx)) => {
            if rts_rx.global {
                rts_rx.regex.captures_iter(&s_owned).map(|caps| {
                    let start = caps.get(0).unwrap().start();
                    let end = caps.get(0).unwrap().end();
                    let groups: Vec<String> = (0..caps.len())
                        .map(|i| caps.get(i).map(|m| m.as_str().to_owned()).unwrap_or_default())
                        .collect();
                    (start, end, groups)
                }).collect()
            } else {
                match rts_rx.regex.captures(&s_owned) {
                    Some(caps) => {
                        let start = caps.get(0).unwrap().start();
                        let end = caps.get(0).unwrap().end();
                        let groups: Vec<String> = (0..caps.len())
                            .map(|i| caps.get(i).map(|m| m.as_str().to_owned()).unwrap_or_default())
                            .collect();
                        vec![(start, end, groups)]
                    }
                    None => Vec::new(),
                }
            }
        }
        _ => Vec::new(),
    });

    if matches.is_empty() {
        return alloc_entry(Entry::String(s_owned.into_bytes()));
    }

    let mut result = String::new();
    let mut last_end = 0usize;
    let bytes = s_owned.as_bytes();

    // Resolve raw fn_ptr: either a Function handle or a direct function pointer.
    let raw_fn_ptr: u64 = crate::namespaces::gc::handles::with_entry(fn_handle, |e| match e {
        Some(Entry::Function(f)) => f.fn_ptr as u64,
        _ => fn_handle,
    });

    for (start, end, groups) in &matches {
        // parte antes do match
        result.push_str(std::str::from_utf8(&bytes[last_end..*start]).unwrap_or(""));

        // Aloca handles para todos os grupos (match, p1, p2, ...) + offset + input.
        let group_handles: Vec<i64> = groups.iter().map(|g| {
            alloc_entry(Entry::String(g.as_bytes().to_vec())) as i64
        }).collect();

        // Chama callback(match, ...captureGroups, offset, inputString).
        // JS spec: fn(match, p1, p2, ..., offset, string).
        let offset_i64 = *start as i64;
        let input_h = alloc_entry(Entry::String(s_owned.as_bytes().to_vec())) as i64;

        let repl_h = unsafe {
            match group_handles.len() {
                1 => {
                    // sem grupos: fn(match, offset, string)
                    let f: extern "C" fn(i64, i64, i64) -> i64 = std::mem::transmute(raw_fn_ptr as usize);
                    f(group_handles[0], offset_i64, input_h)
                }
                2 => {
                    let f: extern "C" fn(i64, i64, i64, i64) -> i64 = std::mem::transmute(raw_fn_ptr as usize);
                    f(group_handles[0], group_handles[1], offset_i64, input_h)
                }
                3 => {
                    let f: extern "C" fn(i64, i64, i64, i64, i64) -> i64 = std::mem::transmute(raw_fn_ptr as usize);
                    f(group_handles[0], group_handles[1], group_handles[2], offset_i64, input_h)
                }
                _ => {
                    // fallback: just match
                    let f: extern "C" fn(i64) -> i64 = std::mem::transmute(raw_fn_ptr as usize);
                    f(group_handles[0])
                }
            }
        } as u64;

        // converte o resultado da funcao para string
        let repl_str: String = with_entry(repl_h, |e| match e {
            Some(Entry::String(b)) => String::from_utf8_lossy(b).into_owned(),
            _ => String::new(),
        });
        result.push_str(&repl_str);
        last_end = *end;
    }
    // parte restante
    result.push_str(std::str::from_utf8(&bytes[last_end..]).unwrap_or(""));
    alloc_entry(Entry::String(result.into_bytes()))
}

/// `str.split(regex_handle)` — divide string usando regex.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_SPLIT_REGEX(
    recv: u64,
    regex_handle: u64,
) -> u64 {
    __RTS_FN_GL_STRING_SPLIT_REGEX_LIMIT(recv, regex_handle, i64::MAX)
}

/// `str.split(regex_handle, limit)` — divide string usando regex com limite.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_STRING_SPLIT_REGEX_LIMIT(
    recv: u64,
    regex_handle: u64,
    limit: i64,
) -> u64 {
    use crate::namespaces::gc::handles::{Entry, alloc_entry, with_entry};
    use crate::namespaces::globals::string::rt::alloc_str;
    unsafe extern "C" {
        fn __RTS_FN_NS_COLLECTIONS_VEC_NEW() -> u64;
        fn __RTS_FN_NS_COLLECTIONS_VEC_PUSH(handle: u64, value: i64);
    }
    let s_opt = with_entry(recv, |e| match e {
        Some(Entry::String(b)) => Some(String::from_utf8_lossy(b).into_owned()),
        _ => None,
    });
    let Some(s) = s_opt else { return alloc_entry(Entry::Vec(Box::new(Vec::new()))); };
    let lim = if limit < 0 { usize::MAX } else { limit as usize };
    // (#1086) JS spec: split com regex que tem capture groups inclui os
    // matches dos grupos no resultado, intercalados com as partes. Ex:
    // \"a1b2c3\".split(/(\\d)/) -> [\"a\", \"1\", \"b\", \"2\", \"c\", \"3\", \"\"].
    let parts: Vec<Option<String>> = with_entry(regex_handle, |e| match e {
        Some(Entry::Regex(rts_rx)) => {
            let has_groups = rts_rx.regex.captures_len() > 1;
            if !has_groups {
                return rts_rx.regex.split(&s).take(lim).map(|p| Some(p.to_owned())).collect();
            }
            // Manual split com injecao de capture groups.
            let mut out: Vec<Option<String>> = Vec::new();
            let mut last_end = 0usize;
            for caps in rts_rx.regex.captures_iter(&s) {
                if out.len() >= lim { break; }
                let full = caps.get(0).unwrap();
                // segmento antes do match
                out.push(Some(s[last_end..full.start()].to_owned()));
                if out.len() >= lim { break; }
                // capture groups (1..N) intercalados
                for i in 1..caps.len() {
                    if out.len() >= lim { break; }
                    out.push(caps.get(i).map(|m| m.as_str().to_owned()));
                }
                last_end = full.end();
            }
            if out.len() < lim {
                out.push(Some(s[last_end..].to_owned()));
            }
            out
        }
        _ => vec![Some(s.clone())],
    });
    let vec_h = unsafe { __RTS_FN_NS_COLLECTIONS_VEC_NEW() };
    for part in parts {
        let h: i64 = match part {
            Some(p) => alloc_str(&p) as i64,
            None => i64::MIN + 2, // undefined sentinel para grupos nao-participantes
        };
        unsafe { __RTS_FN_NS_COLLECTIONS_VEC_PUSH(vec_h, h) };
    }
    vec_h
}
