//! Formatters de i64/f64/bool -> string handle.

unsafe extern "C" {
    fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64;
}

fn intern(s: &str) -> u64 {
    unsafe { __RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64) }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FMT_FMT_I64(value: i64) -> u64 {
    intern(&value.to_string())
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FMT_FMT_F64(value: f64) -> u64 {
    intern(&value.to_string())
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FMT_FMT_BOOL(value: i64) -> u64 {
    intern(if value != 0 { "true" } else { "false" })
}

/// `value` formatado com prefixo `0x` em hex lowercase. Para valores
/// negativos, usa os bits complemento-de-dois (ex: -1 → "0xffffffffffffffff").
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FMT_FMT_HEX(value: i64) -> u64 {
    intern(&format!("0x{:x}", value as u64))
}

/// Binario com prefixo `0b`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FMT_FMT_BIN(value: i64) -> u64 {
    intern(&format!("0b{:b}", value as u64))
}

/// Octal com prefixo `0o`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FMT_FMT_OCT(value: i64) -> u64 {
    intern(&format!("0o{:o}", value as u64))
}

/// Float com numero fixo de casas decimais. `precision` negativo e
/// tratado como 0.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FMT_FMT_F64_PREC(value: f64, precision: i32) -> u64 {
    let prec = precision.max(0) as usize;
    intern(&format!("{value:.prec$}"))
}
