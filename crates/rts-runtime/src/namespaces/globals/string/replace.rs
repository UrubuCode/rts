//! Substituicao de substring: replace (todas) / replacen (N primeiras).

unsafe extern "C" {
    fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64;
}

fn str_from_abi<'a>(ptr: *const u8, len: i64) -> Option<&'a str> {
    if ptr.is_null() || len < 0 {
        return None;
    }
    // SAFETY: caller contract.
    let slice = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    std::str::from_utf8(slice).ok()
}

fn intern(s: &str) -> u64 {
    unsafe { __RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64) }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_STRING_REPLACE(
    s_ptr: *const u8,
    s_len: i64,
    from_ptr: *const u8,
    from_len: i64,
    to_ptr: *const u8,
    to_len: i64,
) -> u64 {
    let Some(s) = str_from_abi(s_ptr, s_len) else {
        return 0;
    };
    let Some(from) = str_from_abi(from_ptr, from_len) else {
        return 0;
    };
    let Some(to) = str_from_abi(to_ptr, to_len) else {
        return 0;
    };
    intern(&s.replace(from, to))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_STRING_REPLACEN(
    s_ptr: *const u8,
    s_len: i64,
    from_ptr: *const u8,
    from_len: i64,
    to_ptr: *const u8,
    to_len: i64,
    n: i64,
) -> u64 {
    let Some(s) = str_from_abi(s_ptr, s_len) else {
        return 0;
    };
    let Some(from) = str_from_abi(from_ptr, from_len) else {
        return 0;
    };
    let Some(to) = str_from_abi(to_ptr, to_len) else {
        return 0;
    };
    if n < 0 {
        return 0;
    }
    intern(&s.replacen(from, to, n as usize))
}
