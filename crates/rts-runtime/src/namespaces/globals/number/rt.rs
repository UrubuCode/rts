//! Runtime implementations for GlobalClassSpec Number.
//! Simbolos: __RTS_FN_GL_NUMBER_*.

unsafe extern "C" {
    fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64;
    fn __RTS_FN_NS_GC_STRING_PTR(handle: u64) -> *const u8;
    fn __RTS_FN_NS_GC_STRING_LEN(handle: u64) -> i64;
}

fn alloc_str(s: &str) -> u64 {
    unsafe { __RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64) }
}

// ── FROM_STR: parse string handle to f64 ─────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_NUMBER_FROM_STR(handle: u64) -> f64 {
    let ptr = unsafe { __RTS_FN_NS_GC_STRING_PTR(handle) };
    let len = unsafe { __RTS_FN_NS_GC_STRING_LEN(handle) };
    if ptr.is_null() || len < 0 {
        return f64::NAN;
    }
    // JS spec: Number("") === 0 e Number("   ") === 0 (string vazia ou
    // somente whitespace coage para zero, nao NaN).
    if len == 0 {
        return 0.0;
    }
    let bytes = unsafe { std::slice::from_raw_parts(ptr as *const u8, len as usize) };
    let s = match std::str::from_utf8(bytes) {
        Ok(s) => s,
        Err(_) => return f64::NAN,
    };
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return 0.0;
    }
    // JS aceita prefixos numericos: 0x.. (hex), 0o.. (oct), 0b.. (bin)
    // alem do parse f64 padrao.
    if let Some(hex) = trimmed.strip_prefix("0x").or_else(|| trimmed.strip_prefix("0X")) {
        if let Ok(v) = u64::from_str_radix(hex, 16) {
            return v as f64;
        }
        return f64::NAN;
    }
    if let Some(oct) = trimmed.strip_prefix("0o").or_else(|| trimmed.strip_prefix("0O")) {
        if let Ok(v) = u64::from_str_radix(oct, 8) {
            return v as f64;
        }
        return f64::NAN;
    }
    if let Some(bin) = trimmed.strip_prefix("0b").or_else(|| trimmed.strip_prefix("0B")) {
        if let Ok(v) = u64::from_str_radix(bin, 2) {
            return v as f64;
        }
        return f64::NAN;
    }
    match trimmed.parse::<f64>() {
        Ok(v) => v,
        Err(_) => f64::NAN,
    }
}

// ── Constants (exposed as f64 globals) ────────────────────────────────────────
// Cranelift emite `global_value` + `load` para MemberKind::Constant com symbol.
// Alternativa: codegen inline os valores como f64const. Por ora expoe via fn.

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_NUMBER_NEW_FROM(v: f64) -> f64 {
    v
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_NUMBER_NEW_EMPTY() -> f64 {
    0.0
}

// ── Static methods ────────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_NUMBER_IS_NAN(v: f64) -> i64 {
    v.is_nan() as i64
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_NUMBER_IS_FINITE(v: f64) -> i64 {
    v.is_finite() as i64
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_NUMBER_IS_INTEGER(v: f64) -> i64 {
    (v.is_finite() && v.fract() == 0.0) as i64
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_NUMBER_IS_SAFE_INT(v: f64) -> i64 {
    const MAX: f64 = 9_007_199_254_740_991.0; // 2^53 - 1
    (v.is_finite() && v.fract() == 0.0 && v.abs() <= MAX) as i64
}

// ── Instance methods ──────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_NUMBER_VALUE_OF(v: f64) -> f64 {
    v
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_NUMBER_TO_FIXED(v: f64, digits: i64) -> u64 {
    let d = digits.clamp(0, 100) as usize;
    alloc_str(&format!("{v:.prec$}", prec = d))
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_NUMBER_TO_PRECISION(v: f64, digits: i64) -> u64 {
    if digits <= 0 || !v.is_finite() {
        return alloc_str(&format!("{v}"));
    }
    let sig = digits.clamp(1, 100) as usize;
    // Determine magnitude to decide fixed vs exponential (mirrors JS spec).
    let mag = if v == 0.0 { 0i32 } else { v.abs().log10().floor() as i32 };
    if mag >= -(sig as i32 - 1) && mag < sig as i32 {
        // Fixed notation: fractional digits = sig - (mag + 1)
        let frac = (sig as i32 - 1 - mag).max(0) as usize;
        alloc_str(&format!("{v:.prec$}", prec = frac))
    } else {
        // Exponential notation with JS-style e+N / e-N
        let s = format!("{v:.prec$e}", prec = sig - 1);
        alloc_str(&js_exp_notation(&s))
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_NUMBER_TO_EXPONENTIAL(v: f64, digits: i64) -> u64 {
    // JS spec: sem arg (digits < 0) usa precisao minima necessaria.
    // Com arg, formata com precisao fixa.
    let s = if digits < 0 {
        format!("{v:e}")
    } else {
        let d = digits.clamp(0, 100) as usize;
        format!("{v:.prec$e}", prec = d)
    };
    alloc_str(&js_exp_notation(&s))
}

// Converts Rust's `1.5e10` / `1.5e-3` to JS-style `1.5e+10` / `1.5e-3`.
fn js_exp_notation(s: &str) -> String {
    if let Some(e_pos) = s.find('e') {
        let (mantissa, exp_part) = s.split_at(e_pos);
        let exp_str = &exp_part[1..]; // strip 'e'
        if exp_str.starts_with('-') {
            format!("{mantissa}e{exp_str}")
        } else {
            format!("{mantissa}e+{exp_str}")
        }
    } else {
        s.to_string()
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_NUMBER_TO_STRING_RADIX(v: f64, radix: i64) -> u64 {
    let r = radix.clamp(2, 36) as u32;
    if r == 10 {
        // Fast path: standard decimal representation.
        if v.fract() == 0.0 && v.is_finite() {
            return alloc_str(&format!("{}", v as i64));
        }
        return alloc_str(&format!("{v}"));
    }
    // Integer part in arbitrary radix.
    if v.fract() == 0.0 && v.is_finite() {
        let n = v as i64;
        if n == 0 {
            return alloc_str("0");
        }
        let negative = n < 0;
        let mut n = n.unsigned_abs();
        let mut digits = Vec::new();
        while n > 0 {
            let d = (n % r as u64) as u8;
            digits.push(if d < 10 { b'0' + d } else { b'a' + d - 10 });
            n /= r as u64;
        }
        if negative {
            digits.push(b'-');
        }
        digits.reverse();
        alloc_str(std::str::from_utf8(&digits).unwrap_or("0"))
    } else {
        // Fractional: radix 2/8/16 common, fallback to decimal for others.
        match r {
            16 => alloc_str(&format!("{:x}", v.to_bits())),
            _ => alloc_str(&format!("{v}")),
        }
    }
}
