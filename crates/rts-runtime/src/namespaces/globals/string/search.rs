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
    let result = with_entry(regex_handle, |e| match e {
        Some(Entry::Regex(rx)) => rx.find(&s_owned).map(|m| m.as_str().as_bytes().to_vec()),
        _ => None,
    });
    match result {
        Some(bytes) => alloc_entry(Entry::String(bytes)),
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
