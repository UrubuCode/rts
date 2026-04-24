//! Parsers de string -> i64 / f64 / bool.

fn str_from_abi<'a>(ptr: *const u8, len: i64) -> Option<&'a str> {
    if ptr.is_null() || len < 0 {
        return None;
    }
    // SAFETY: caller contract.
    let slice = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    std::str::from_utf8(slice).ok()
}

/// Retorna o inteiro parsed ou `i64::MIN` em caso de erro (valor
/// sentinel improvavel em input real).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FMT_PARSE_I64(ptr: *const u8, len: i64) -> i64 {
    match str_from_abi(ptr, len).and_then(|s| s.trim().parse::<i64>().ok()) {
        Some(v) => v,
        None => i64::MIN,
    }
}

/// Retorna o f64 parsed ou NaN em caso de erro.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FMT_PARSE_F64(ptr: *const u8, len: i64) -> f64 {
    match str_from_abi(ptr, len).and_then(|s| s.trim().parse::<f64>().ok()) {
        Some(v) => v,
        None => f64::NAN,
    }
}

/// "true" / "false" (case-insensitive) → 1 / 0. Qualquer outra coisa
/// retorna -1.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FMT_PARSE_BOOL(ptr: *const u8, len: i64) -> i64 {
    let Some(s) = str_from_abi(ptr, len) else {
        return -1;
    };
    match s.trim().to_ascii_lowercase().as_str() {
        "true" | "1" => 1,
        "false" | "0" => 0,
        _ => -1,
    }
}
