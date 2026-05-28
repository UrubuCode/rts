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
    let Some(s) = str_from_abi(ptr, len) else {
        return f64::NAN;
    };
    let trimmed = s.trim_start();
    // Parse direto primeiro (numero puro, caso mais comum/rapido).
    if let Ok(v) = trimmed.trim_end().parse::<f64>() {
        return v;
    }
    // (cross-runtime) JS parseFloat: parseia o MAIOR prefixo numerico
    // valido e ignora o resto (`"3.14abc"` -> 3.14, `"42px"` -> 42,
    // `"1e3xyz"` -> 1000). Varre: sinal, digitos, ponto decimal, expoente.
    let bytes = trimmed.as_bytes();
    let mut i = 0usize;
    if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
        i += 1;
    }
    // Infinity literal (JS aceita "Infinity"/"-Infinity").
    let rest = &trimmed[i..];
    if rest.starts_with("Infinity") {
        let v = f64::INFINITY;
        return if trimmed.as_bytes().first() == Some(&b'-') { -v } else { v };
    }
    let mut seen_digit = false;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
        seen_digit = true;
    }
    if i < bytes.len() && bytes[i] == b'.' {
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
            seen_digit = true;
        }
    }
    if !seen_digit {
        return f64::NAN;
    }
    // Expoente: e/E seguido de sinal opcional e ao menos 1 digito.
    if i < bytes.len() && (bytes[i] == b'e' || bytes[i] == b'E') {
        let mut j = i + 1;
        if j < bytes.len() && (bytes[j] == b'+' || bytes[j] == b'-') {
            j += 1;
        }
        let mut exp_digit = false;
        while j < bytes.len() && bytes[j].is_ascii_digit() {
            j += 1;
            exp_digit = true;
        }
        if exp_digit {
            i = j;
        }
        // Sem digito de expoente: para antes do `e` (i fica no prefixo).
    }
    trimmed[..i].parse::<f64>().unwrap_or(f64::NAN)
}

/// (#208) `parseInt(s, radix?)` — JS spec:
/// - Skip leading whitespace.
/// - Optional `+`/`-` sign.
/// - radix=0 ou ausente: detecta `0x`/`0X` (16) ou default 10.
/// - Para no primeiro char invalido (tolerante a trailing chars).
/// - Retorna i64::MIN sentinel em erro (codegen mapeia pra NaN).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_FMT_PARSE_INT_RADIX(
    ptr: *const u8,
    len: i64,
    radix: i64,
) -> i64 {
    let Some(s) = str_from_abi(ptr, len) else {
        return i64::MIN;
    };
    let bytes = s.as_bytes();
    let mut i = 0usize;
    // Skip whitespace.
    while i < bytes.len() && (bytes[i] as char).is_whitespace() {
        i += 1;
    }
    if i >= bytes.len() {
        return i64::MIN;
    }
    // Sign.
    let negative = match bytes[i] {
        b'+' => { i += 1; false }
        b'-' => { i += 1; true }
        _ => false,
    };
    if i >= bytes.len() {
        return i64::MIN;
    }
    // Detect radix.
    let mut effective_radix: u32 = if radix == 0 { 10 } else { radix as u32 };
    if (radix == 0 || radix == 16)
        && i + 1 < bytes.len()
        && bytes[i] == b'0'
        && (bytes[i + 1] == b'x' || bytes[i + 1] == b'X')
    {
        effective_radix = 16;
        i += 2;
    }
    if !(2..=36).contains(&effective_radix) {
        return i64::MIN;
    }
    // Parse chars enquanto validos no radix.
    let mut acc: i64 = 0;
    let mut any_digit = false;
    while i < bytes.len() {
        let c = bytes[i] as char;
        let Some(d) = c.to_digit(effective_radix) else {
            break;
        };
        // Check overflow.
        let Some(next) = acc.checked_mul(effective_radix as i64).and_then(|v| v.checked_add(d as i64)) else {
            return i64::MIN;
        };
        acc = next;
        any_digit = true;
        i += 1;
    }
    if !any_digit {
        return i64::MIN;
    }
    if negative { -acc } else { acc }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str, radix: i64) -> i64 {
        __RTS_FN_NS_FMT_PARSE_INT_RADIX(s.as_ptr(), s.len() as i64, radix)
    }

    #[test]
    fn radix_10_default() {
        assert_eq!(p("100", 0), 100);
        assert_eq!(p("100", 10), 100);
        assert_eq!(p("-42", 0), -42);
        assert_eq!(p("+42", 0), 42);
    }

    #[test]
    fn radix_16_hex() {
        assert_eq!(p("FF", 16), 255);
        assert_eq!(p("ff", 16), 255);
        assert_eq!(p("0xFF", 16), 255);
        assert_eq!(p("0xff", 0), 255);
    }

    #[test]
    fn radix_2_binary() {
        assert_eq!(p("101", 2), 5);
        assert_eq!(p("1111", 2), 15);
    }

    #[test]
    fn tolerant_trailing_chars() {
        assert_eq!(p("42abc", 10), 42);
        assert_eq!(p("100xyz", 0), 100);
    }

    #[test]
    fn whitespace_stripped() {
        assert_eq!(p("  42", 0), 42);
        assert_eq!(p("\t-7", 10), -7);
    }

    #[test]
    fn empty_or_invalid_returns_sentinel() {
        assert_eq!(p("", 0), i64::MIN);
        assert_eq!(p("xyz", 10), i64::MIN);
        assert_eq!(p("   ", 10), i64::MIN);
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
