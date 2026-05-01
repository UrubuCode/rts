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
