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
        First { items: Vec<Vec<u8>>, index: usize },
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
                        let items: Vec<Vec<u8>> = (0..caps.len())
                            .map(|i| caps.get(i).map(|m| m.as_str().as_bytes().to_vec()).unwrap_or_default())
                            .collect();
                        MatchResultExt::First { items, index }
                    }
                    None => MatchResultExt::None,
                }
            }
        }
        _ => MatchResultExt::None,
    });
    match mr {
        MatchResultExt::First { items, index } => {
            if items.is_empty() { return 0; }
            // Retorna Map com chaves "0","1",... + "index" + "input" + "length".
            let mut map: indexmap::IndexMap<String, i64> = indexmap::IndexMap::new();
            for (i, bytes) in items.iter().enumerate() {
                let h = alloc_entry(Entry::String(bytes.clone())) as i64;
                map.insert(i.to_string(), h);
            }
            map.insert("length".to_string(), items.len() as i64);
            map.insert("index".to_string(), index as i64);
            let input_h = alloc_entry(Entry::String(s_owned.into_bytes())) as i64;
            map.insert("input".to_string(), input_h);
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

    let out = with_entry(regex_handle, |e| match e {
        Some(Entry::Regex(rx)) => {
            if rx.global {
                Some(rx.regex.replace_all(&s_owned, repl.as_str()).into_owned())
            } else {
                Some(rx.regex.replace(&s_owned, repl.as_str()).into_owned())
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
/// Retorna Vec de Vecs: cada elemento eh um Vec [fullMatch, ...grupos].
/// Cada elemento do Vec externo eh um handle para um Vec interno.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_STRING_MATCH_ALL_REGEX(
    s_ptr: *const u8,
    s_len: i64,
    regex_handle: u64,
) -> u64 {
    use crate::namespaces::gc::handles::{Entry, alloc_entry, with_entry};
    let empty_vec = || alloc_entry(Entry::Vec(Box::new(Vec::new())));
    let Some(s) = str_from_abi(s_ptr, s_len) else { return empty_vec(); };
    let s_owned = s.to_string();
    let outer: Vec<i64> = with_entry(regex_handle, |e| match e {
        Some(Entry::Regex(rts_rx)) => {
            rts_rx.regex.captures_iter(&s_owned).map(|caps| {
                let slots: Vec<i64> = (0..caps.len())
                    .map(|i| caps.get(i).map(|m| alloc_entry(Entry::String(m.as_str().as_bytes().to_vec())) as i64).unwrap_or(0))
                    .collect();
                alloc_entry(Entry::Vec(Box::new(slots))) as i64
            }).collect()
        }
        _ => Vec::new(),
    });
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
    let parts: Vec<String> = with_entry(regex_handle, |e| match e {
        Some(Entry::Regex(rts_rx)) => {
            rts_rx.regex.split(&s).take(lim).map(|p| p.to_owned()).collect()
        }
        _ => vec![s.clone()],
    });
    let vec_h = unsafe { __RTS_FN_NS_COLLECTIONS_VEC_NEW() };
    for part in parts {
        let h = alloc_str(&part) as i64;
        unsafe { __RTS_FN_NS_COLLECTIONS_VEC_PUSH(vec_h, h) };
    }
    vec_h
}
