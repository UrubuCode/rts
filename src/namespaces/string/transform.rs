//! Transformacoes que produzem novas strings: case, trim, repeat.

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
pub extern "C" fn __RTS_FN_NS_STRING_TO_UPPER(ptr: *const u8, len: i64) -> u64 {
    let Some(s) = str_from_abi(ptr, len) else { return 0 };
    intern(&s.to_uppercase())
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_STRING_TO_LOWER(ptr: *const u8, len: i64) -> u64 {
    let Some(s) = str_from_abi(ptr, len) else { return 0 };
    intern(&s.to_lowercase())
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_STRING_TRIM(ptr: *const u8, len: i64) -> u64 {
    let Some(s) = str_from_abi(ptr, len) else { return 0 };
    intern(s.trim())
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_STRING_TRIM_START(ptr: *const u8, len: i64) -> u64 {
    let Some(s) = str_from_abi(ptr, len) else { return 0 };
    intern(s.trim_start())
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_STRING_TRIM_END(ptr: *const u8, len: i64) -> u64 {
    let Some(s) = str_from_abi(ptr, len) else { return 0 };
    intern(s.trim_end())
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_STRING_REPEAT(ptr: *const u8, len: i64, n: i64) -> u64 {
    let Some(s) = str_from_abi(ptr, len) else { return 0 };
    if n < 0 {
        return 0;
    }
    let repeated = s.repeat(n as usize);
    intern(&repeated)
}
