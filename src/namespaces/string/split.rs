//! Contagens, acesso por indice e indexing utilitario.
//!
//! `split_at` da issue #25 original exigiria handle composto (par);
//! fica como follow-up quando houver objects (#53). Aqui mantemos:
//! - char_count / byte_len: metricas
//! - char_at: byte i64 no indice dado (indice de char Unicode)
//! - char_code_at: code point Unicode como i64

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

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_STRING_CHAR_COUNT(ptr: *const u8, len: i64) -> i64 {
    match str_from_abi(ptr, len) {
        Some(s) => s.chars().count() as i64,
        None => 0,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_STRING_BYTE_LEN(ptr: *const u8, len: i64) -> i64 {
    match str_from_abi(ptr, len) {
        Some(s) => s.len() as i64,
        None => 0,
    }
}

/// Retorna o char Unicode no indice dado como string handle de 1 char.
/// Handle 0 se indice fora do range.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_STRING_CHAR_AT(ptr: *const u8, len: i64, idx: i64) -> u64 {
    let Some(s) = str_from_abi(ptr, len) else {
        return 0;
    };
    if idx < 0 {
        return 0;
    }
    match s.chars().nth(idx as usize) {
        Some(ch) => {
            let mut buf = [0u8; 4];
            let encoded = ch.encode_utf8(&mut buf);
            unsafe { __RTS_FN_NS_GC_STRING_NEW(encoded.as_ptr(), encoded.len() as i64) }
        }
        None => 0,
    }
}

/// Code point do char no indice; -1 se fora do range.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_STRING_CHAR_CODE_AT(ptr: *const u8, len: i64, idx: i64) -> i64 {
    let Some(s) = str_from_abi(ptr, len) else {
        return -1;
    };
    if idx < 0 {
        return -1;
    }
    match s.chars().nth(idx as usize) {
        Some(ch) => ch as i64,
        None => -1,
    }
}
