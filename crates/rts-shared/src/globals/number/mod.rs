//! `Number` global class — primitivo JS Number.
//!
//! Migrado do `#[rts_class]` (macro) pro modelo builder hand-written do
//! `rts-engine` (rumo à remoção da `rts-macro`). `parseInt`/`parseFloat`
//! (símbolos de `fmt`) e `toString` (símbolo de `gc`) sao membros `external` —
//! apontam p/ o extern de outro namespace sem reemitir (fn_ptr null, sem
//! `#[no_mangle]`). Os 3 non-member externs (NEW_EMPTY/BOX_VALUE_OF/
//! TO_STRING_RADIX), os helpers de arredondamento e `number_const_value` ficam
//! como free items abaixo.

use rts_engine::abi::ty::{Bool, Handle, F64, I64};
use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

unsafe extern "C" {
    fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64;
    fn __RTS_FN_NS_GC_STRING_PTR(handle: u64) -> *const u8;
    fn __RTS_FN_NS_GC_STRING_LEN(handle: u64) -> i64;
}

fn alloc_str(s: &str) -> u64 {
    unsafe { __RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64) }
}

// ── Class member externs (hand-written, espelham a macro). ───────────────────

/// new Number(value) — wraps value como NumberBox.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_NUMBER_NEW_BOXED(v: F64) -> Handle {
    rts_engine::heap::handles::alloc_entry(rts_engine::heap::handles::Entry::NumberBox(v))
}

/// new Number() — wraps 0 como NumberBox.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_NUMBER_NEW_BOXED_EMPTY() -> Handle {
    rts_engine::heap::handles::alloc_entry(rts_engine::heap::handles::Entry::NumberBox(
        0.0,
    ))
}

/// Number(value) — coerce value to number (no `new`, returns primitive).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_NUMBER_NEW_FROM(v: F64) -> F64 {
    v
}

/// Internal: parse string handle to f64 (used by Number() coercion).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_NUMBER_FROM_STR(handle: Handle) -> F64 {
    let ptr = unsafe { __RTS_FN_NS_GC_STRING_PTR(handle) };
    let len = unsafe { __RTS_FN_NS_GC_STRING_LEN(handle) };
    if ptr.is_null() || len < 0 {
        return f64::NAN;
    }
    if len == 0 {
        return 0.0;
    }
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
    let s = match std::str::from_utf8(bytes) {
        Ok(s) => s,
        Err(_) => return f64::NAN,
    };
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return 0.0;
    }
    if let Some(hex) = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
    {
        return u64::from_str_radix(hex, 16)
            .map(|v| v as f64)
            .unwrap_or(f64::NAN);
    }
    if let Some(oct) = trimmed
        .strip_prefix("0o")
        .or_else(|| trimmed.strip_prefix("0O"))
    {
        return u64::from_str_radix(oct, 8)
            .map(|v| v as f64)
            .unwrap_or(f64::NAN);
    }
    if let Some(bin) = trimmed
        .strip_prefix("0b")
        .or_else(|| trimmed.strip_prefix("0B"))
    {
        return u64::from_str_radix(bin, 2)
            .map(|v| v as f64)
            .unwrap_or(f64::NAN);
    }
    trimmed.parse::<f64>().unwrap_or(f64::NAN)
}

/// Number.isNaN(value).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_NUMBER_IS_NAN(v: F64) -> Bool {
    v.is_nan() as i64
}

/// Number.isFinite(value).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_NUMBER_IS_FINITE(v: F64) -> Bool {
    v.is_finite() as i64
}

/// Number.isInteger(value).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_NUMBER_IS_INTEGER(v: F64) -> Bool {
    (v.is_finite() && v.fract() == 0.0) as i64
}

/// Number.isSafeInteger(value).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_NUMBER_IS_SAFE_INT(v: F64) -> Bool {
    const MAX: f64 = 9_007_199_254_740_991.0;
    (v.is_finite() && v.fract() == 0.0 && v.abs() <= MAX) as i64
}

/// n.toFixed(digits) — fixed-point string.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_NUMBER_TO_FIXED(v: F64, digits: I64) -> Handle {
    if v.is_nan() {
        return alloc_str("NaN");
    }
    if v.is_infinite() {
        return alloc_str(if v > 0.0 { "Infinity" } else { "-Infinity" });
    }
    let d = digits.clamp(0, 100) as usize;
    let neg = v.is_sign_negative() && v != 0.0;
    let abs = v.abs();
    if abs < 1e21 {
        let high = format!("{abs:.25}");
        let out = round_decimal_str(&high, d);
        let out = if neg { format!("-{out}") } else { out };
        return alloc_str(&out);
    }
    alloc_str(&format!("{v:.prec$}", prec = d))
}

/// n.valueOf() — primitive identity ou unbox NumberBox.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_NUMBER_VALUE_OF(handle_or_bits: Handle) -> F64 {
    let h = handle_or_bits;
    let unboxed = rts_engine::heap::handles::with_entry(h, |e| match e {
        Some(rts_engine::heap::handles::Entry::NumberBox(inner)) => Some(*inner),
        _ => None,
    });
    match unboxed {
        Some(inner) => inner,
        None => f64::from_bits(h),
    }
}

/// n.toPrecision(digits) — significant digits string.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_NUMBER_TO_PRECISION(v: F64, digits: I64) -> Handle {
    if digits <= 0 || !v.is_finite() {
        return alloc_str(&format!("{v}"));
    }
    let sig = digits.clamp(1, 100) as usize;
    let mag = if v == 0.0 {
        0i32
    } else {
        v.abs().log10().floor() as i32
    };
    if mag >= -6 && mag < sig as i32 {
        let frac = (sig as i32 - 1 - mag).max(0) as usize;
        let neg = v.is_sign_negative() && v != 0.0;
        let high = format!("{:.25}", v.abs());
        let mut body = round_decimal_str(&high, frac);
        let int_part = body.split('.').next().unwrap_or("");
        let int_digits = int_part.trim_start_matches('0').len();
        if int_digits > sig {
            let s = round_exp_str(v, sig - 1);
            return alloc_str(&js_exp_notation(&s));
        }
        if int_digits >= sig && body.contains('.') {
            body = int_part.to_string();
        }
        let out = if neg { format!("-{body}") } else { body };
        alloc_str(&out)
    } else {
        let s = round_exp_str(v, sig - 1);
        alloc_str(&js_exp_notation(&s))
    }
}

/// n.toExponential(digits) — exponential notation string.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_NUMBER_TO_EXPONENTIAL(v: F64, digits: I64) -> Handle {
    let s = if digits < 0 {
        let formatted = format!("{v:e}");
        let with_js_notation = js_exp_notation(&formatted);
        remove_trailing_zeros_exp(&with_js_notation)
    } else {
        let d = digits.clamp(0, 100) as usize;
        let formatted = round_exp_str(v, d);
        js_exp_notation(&formatted)
    };
    alloc_str(&s)
}

// ── Member builder helper (espelha `leak_class` da macro). ───────────────────

/// Membro de classe global (helper hand-written).
#[allow(clippy::too_many_arguments)]
fn m(
    name: &str,
    kind: MemberKind,
    sig: Sig,
    symbol: &str,
    ts: &str,
    doc: &str,
    fp: *const u8,
    pure: bool,
) -> Member {
    Member {
        name: name.to_string(),
        kind,
        sig,
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: doc.to_string(),
        pure,
        intrinsic: None,
    }
}

/// Registra a classe global `Number` no motor (hand-written, sem macro).
pub fn register_number_class_spec(e: &mut Engine) {
    e.class("Number")
        .doc("JS Number primitive class — Number.isNaN(), Number.MAX_SAFE_INTEGER, n.toFixed() etc.")
        .member(m(
            "new",
            MemberKind::Constructor,
            Sig::new(vec![AbiType::F64], AbiType::Handle),
            "__RTS_FN_GL_NUMBER_NEW_BOXED",
            "new(value: number): number",
            "new Number(value) — wraps value como NumberBox.",
            __RTS_FN_GL_NUMBER_NEW_BOXED as *const u8,
            true,
        ))
        .member(m(
            "new",
            MemberKind::Constructor,
            Sig::new(Vec::new(), AbiType::Handle),
            "__RTS_FN_GL_NUMBER_NEW_BOXED_EMPTY",
            "new(): number",
            "new Number() — wraps 0 como NumberBox.",
            __RTS_FN_GL_NUMBER_NEW_BOXED_EMPTY as *const u8,
            true,
        ))
        .member(m(
            "newFrom",
            MemberKind::Function,
            Sig::new(vec![AbiType::F64], AbiType::F64),
            "__RTS_FN_GL_NUMBER_NEW_FROM",
            "newFrom(value: number): number",
            "Number(value) — coerce value to number (no `new`, returns primitive).",
            __RTS_FN_GL_NUMBER_NEW_FROM as *const u8,
            true,
        ))
        .member(m(
            "fromStr",
            MemberKind::Function,
            Sig::new(vec![AbiType::Handle], AbiType::F64),
            "__RTS_FN_GL_NUMBER_FROM_STR",
            "fromStr(s: string): number",
            "Internal: parse string handle to f64 (used by Number() coercion).",
            __RTS_FN_GL_NUMBER_FROM_STR as *const u8,
            true,
        ))
        .member(m(
            "MAX_SAFE_INTEGER",
            MemberKind::Constant,
            Sig::new(Vec::new(), AbiType::F64),
            "__RTS_DATA_GL_NUMBER_MAX_SAFE_INTEGER",
            "MAX_SAFE_INTEGER: number",
            "Number.MAX_SAFE_INTEGER — 2^53 - 1.",
            core::ptr::null::<u8>(),
            true,
        ))
        .member(m(
            "MIN_SAFE_INTEGER",
            MemberKind::Constant,
            Sig::new(Vec::new(), AbiType::F64),
            "__RTS_DATA_GL_NUMBER_MIN_SAFE_INTEGER",
            "MIN_SAFE_INTEGER: number",
            "Number.MIN_SAFE_INTEGER — -(2^53 - 1).",
            core::ptr::null::<u8>(),
            true,
        ))
        .member(m(
            "MAX_VALUE",
            MemberKind::Constant,
            Sig::new(Vec::new(), AbiType::F64),
            "__RTS_DATA_GL_NUMBER_MAX_VALUE",
            "MAX_VALUE: number",
            "Number.MAX_VALUE — largest finite f64.",
            core::ptr::null::<u8>(),
            true,
        ))
        .member(m(
            "MIN_VALUE",
            MemberKind::Constant,
            Sig::new(Vec::new(), AbiType::F64),
            "__RTS_DATA_GL_NUMBER_MIN_VALUE",
            "MIN_VALUE: number",
            "Number.MIN_VALUE — smallest positive f64.",
            core::ptr::null::<u8>(),
            true,
        ))
        .member(m(
            "EPSILON",
            MemberKind::Constant,
            Sig::new(Vec::new(), AbiType::F64),
            "__RTS_DATA_GL_NUMBER_EPSILON",
            "EPSILON: number",
            "Number.EPSILON — 2^-52.",
            core::ptr::null::<u8>(),
            true,
        ))
        .member(m(
            "POSITIVE_INFINITY",
            MemberKind::Constant,
            Sig::new(Vec::new(), AbiType::F64),
            "__RTS_DATA_GL_NUMBER_POS_INF",
            "POSITIVE_INFINITY: number",
            "Number.POSITIVE_INFINITY.",
            core::ptr::null::<u8>(),
            true,
        ))
        .member(m(
            "NEGATIVE_INFINITY",
            MemberKind::Constant,
            Sig::new(Vec::new(), AbiType::F64),
            "__RTS_DATA_GL_NUMBER_NEG_INF",
            "NEGATIVE_INFINITY: number",
            "Number.NEGATIVE_INFINITY.",
            core::ptr::null::<u8>(),
            true,
        ))
        .member(m(
            "NaN",
            MemberKind::Constant,
            Sig::new(Vec::new(), AbiType::F64),
            "__RTS_DATA_GL_NUMBER_NAN",
            "NaN: number",
            "Number.NaN.",
            core::ptr::null::<u8>(),
            true,
        ))
        .member(m(
            "isNaN",
            MemberKind::StaticMethod,
            Sig::new(vec![AbiType::F64], AbiType::Bool),
            "__RTS_FN_GL_NUMBER_IS_NAN",
            "isNaN(value: number): boolean",
            "Number.isNaN(value).",
            __RTS_FN_GL_NUMBER_IS_NAN as *const u8,
            true,
        ))
        .member(m(
            "isFinite",
            MemberKind::StaticMethod,
            Sig::new(vec![AbiType::F64], AbiType::Bool),
            "__RTS_FN_GL_NUMBER_IS_FINITE",
            "isFinite(value: number): boolean",
            "Number.isFinite(value).",
            __RTS_FN_GL_NUMBER_IS_FINITE as *const u8,
            true,
        ))
        .member(m(
            "isInteger",
            MemberKind::StaticMethod,
            Sig::new(vec![AbiType::F64], AbiType::Bool),
            "__RTS_FN_GL_NUMBER_IS_INTEGER",
            "isInteger(value: number): boolean",
            "Number.isInteger(value).",
            __RTS_FN_GL_NUMBER_IS_INTEGER as *const u8,
            true,
        ))
        .member(m(
            "isSafeInteger",
            MemberKind::StaticMethod,
            Sig::new(vec![AbiType::F64], AbiType::Bool),
            "__RTS_FN_GL_NUMBER_IS_SAFE_INT",
            "isSafeInteger(value: number): boolean",
            "Number.isSafeInteger(value).",
            __RTS_FN_GL_NUMBER_IS_SAFE_INT as *const u8,
            true,
        ))
        .member(m(
            "parseInt",
            MemberKind::StaticMethod,
            Sig::new(vec![AbiType::StrPtr], AbiType::I64),
            "__RTS_FN_NS_FMT_PARSE_I64",
            "parseInt(s: string): number",
            "Number.parseInt(s) — delega ao extern de fmt.",
            core::ptr::null::<u8>(),
            true,
        ))
        .member(m(
            "parseFloat",
            MemberKind::StaticMethod,
            Sig::new(vec![AbiType::StrPtr], AbiType::F64),
            "__RTS_FN_NS_FMT_PARSE_F64",
            "parseFloat(s: string): number",
            "Number.parseFloat(s) — delega ao extern de fmt.",
            core::ptr::null::<u8>(),
            true,
        ))
        .member(m(
            "toFixed",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::F64, AbiType::I64], AbiType::Handle),
            "__RTS_FN_GL_NUMBER_TO_FIXED",
            "toFixed(digits?: number): string",
            "n.toFixed(digits) — fixed-point string.",
            __RTS_FN_GL_NUMBER_TO_FIXED as *const u8,
            true,
        ))
        .member(m(
            "toString",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::F64], AbiType::Handle),
            "__RTS_FN_NS_GC_STRING_FROM_F64",
            "toString(): string",
            "n.toString() — delega ao extern de gc (string from f64).",
            core::ptr::null::<u8>(),
            true,
        ))
        .member(m(
            "valueOf",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle], AbiType::F64),
            "__RTS_FN_GL_NUMBER_VALUE_OF",
            "valueOf(): number",
            "n.valueOf() — primitive identity ou unbox NumberBox.",
            __RTS_FN_GL_NUMBER_VALUE_OF as *const u8,
            true,
        ))
        .member(m(
            "toPrecision",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::F64, AbiType::I64], AbiType::Handle),
            "__RTS_FN_GL_NUMBER_TO_PRECISION",
            "toPrecision(digits?: number): string",
            "n.toPrecision(digits) — significant digits string.",
            __RTS_FN_GL_NUMBER_TO_PRECISION as *const u8,
            true,
        ))
        .member(m(
            "toExponential",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::F64, AbiType::I64], AbiType::Handle),
            "__RTS_FN_GL_NUMBER_TO_EXPONENTIAL",
            "toExponential(digits?: number): string",
            "n.toExponential(digits) — exponential notation string.",
            __RTS_FN_GL_NUMBER_TO_EXPONENTIAL as *const u8,
            true,
        ))
        .done();
}

// ── Free items: non-member externs + helpers + const resolver. ───────────────

/// `new Number()` primitivo (sem arg) — wraps 0 como f64. Non-member.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_NUMBER_NEW_EMPTY() -> f64 {
    0.0
}

/// NumberBox.valueOf()/toString() — recupera o primitive embrulhado. Non-member.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_NUMBER_BOX_VALUE_OF(boxed: u64) -> f64 {
    rts_engine::heap::handles::with_entry(boxed, |e| match e {
        Some(rts_engine::heap::handles::Entry::NumberBox(v)) => *v,
        _ => f64::from_bits(boxed),
    })
}

/// Arredonda a string decimal `s` ("int.frac", sem sinal) para `d` casas,
/// half-away-from-zero. `s` tem precisao suficiente (>= d+1 casas).
fn round_decimal_str(s: &str, d: usize) -> String {
    let (int_part, frac_part) = match s.split_once('.') {
        Some((i, f)) => (i.to_string(), f.to_string()),
        None => (s.to_string(), String::new()),
    };
    let frac_bytes: Vec<u8> = frac_part.bytes().collect();
    let round_up = frac_bytes.get(d).map(|&b| b >= b'5').unwrap_or(false);
    let mut digits: Vec<u8> = Vec::new();
    for b in int_part.bytes() {
        digits.push(b);
    }
    let int_len = digits.len();
    for i in 0..d {
        digits.push(*frac_bytes.get(i).unwrap_or(&b'0'));
    }
    if round_up {
        let mut i = digits.len();
        loop {
            if i == 0 {
                digits.insert(0, b'1');
                break;
            }
            i -= 1;
            if digits[i] == b'9' {
                digits[i] = b'0';
            } else {
                digits[i] += 1;
                break;
            }
        }
    }
    let new_int_len = digits.len() - d;
    let int_s: String = String::from_utf8_lossy(&digits[..new_int_len]).into_owned();
    let int_s = if int_s.is_empty() {
        "0".to_string()
    } else {
        int_s
    };
    let _ = int_len;
    if d == 0 {
        int_s
    } else {
        let frac_s: String = String::from_utf8_lossy(&digits[new_int_len..]).into_owned();
        format!("{int_s}.{frac_s}")
    }
}

/// Formata `v` em notacao cientifica com `prec` casas na mantissa,
/// half-away-from-zero sobre o valor real. Formato de `format!("{:e}")`.
fn round_exp_str(v: f64, prec: usize) -> String {
    if v == 0.0 {
        let mant = if prec == 0 {
            "0".to_string()
        } else {
            format!("0.{}", "0".repeat(prec))
        };
        return format!("{mant}e0");
    }
    let neg = v < 0.0;
    let abs = v.abs();
    let high = format!("{abs:.30e}");
    let (mant_part, exp_part) = high.split_once('e').unwrap_or((high.as_str(), "0"));
    let mut exp: i32 = exp_part.parse().unwrap_or(0);
    let mant_digits: Vec<u8> = mant_part.bytes().filter(|b| b.is_ascii_digit()).collect();
    let keep = prec + 1;
    let mut digits: Vec<u8> = mant_digits.iter().take(keep).copied().collect();
    while digits.len() < keep {
        digits.push(b'0');
    }
    let round_up = mant_digits.get(keep).map(|&b| b >= b'5').unwrap_or(false);
    if round_up {
        let mut i = digits.len();
        loop {
            if i == 0 {
                digits.insert(0, b'1');
                exp += 1;
                digits.pop();
                break;
            }
            i -= 1;
            if digits[i] == b'9' {
                digits[i] = b'0';
            } else {
                digits[i] += 1;
                break;
            }
        }
    }
    let first = digits[0] as char;
    let rest: String = digits[1..].iter().map(|&b| b as char).collect();
    let mantissa = if prec == 0 {
        first.to_string()
    } else {
        format!("{first}.{rest}")
    };
    let sign = if neg { "-" } else { "" };
    format!("{sign}{mantissa}e{exp}")
}

/// `n.toString(radix)` com radix arbitrario. Non-member (codegen chama por simbolo).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_NUMBER_TO_STRING_RADIX(v: f64, radix: i64) -> u64 {
    if v.is_nan() {
        return alloc_str("NaN");
    }
    if v.is_infinite() {
        return alloc_str(if v > 0.0 { "Infinity" } else { "-Infinity" });
    }
    let r = radix.clamp(2, 36) as u32;
    if r == 10 {
        if v.fract() == 0.0 && v.is_finite() {
            return alloc_str(&format!("{}", v as i64));
        }
        return alloc_str(&format!("{v}"));
    }
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
        match r {
            16 => alloc_str(&format!("{:x}", v.to_bits())),
            _ => alloc_str(&format!("{v}")),
        }
    }
}

/// Remove trailing zeros da mantissa em notação exponencial.
fn remove_trailing_zeros_exp(s: &str) -> String {
    if let Some(e_pos) = s.find('e') {
        let (mantissa, exp_part) = s.split_at(e_pos);
        let trimmed = mantissa.trim_end_matches('0');
        let trimmed = if trimmed.ends_with('.') {
            &trimmed[..trimmed.len() - 1]
        } else {
            trimmed
        };
        format!("{trimmed}{exp_part}")
    } else {
        s.to_string()
    }
}

/// Converte `1.5e10` / `1.5e-3` (Rust) para `1.5e+10` / `1.5e-3` (JS).
fn js_exp_notation(s: &str) -> String {
    if let Some(e_pos) = s.find('e') {
        let (mantissa, exp_part) = s.split_at(e_pos);
        let exp_str = &exp_part[1..];
        if exp_str.starts_with('-') {
            format!("{mantissa}e{exp_str}")
        } else {
            format!("{mantissa}e+{exp_str}")
        }
    } else {
        s.to_string()
    }
}

/// Valor f64 de uma constante `Number.*` por nome. Usado pelo codegen para
/// emitir `f64const` inline.
pub fn number_const_value(name: &str) -> Option<f64> {
    match name {
        "MAX_SAFE_INTEGER" => Some(9_007_199_254_740_991.0_f64),
        "MIN_SAFE_INTEGER" => Some(-9_007_199_254_740_991.0_f64),
        "MAX_VALUE" => Some(f64::MAX),
        "MIN_VALUE" => Some(5e-324),
        "EPSILON" => Some(f64::EPSILON),
        "POSITIVE_INFINITY" => Some(f64::INFINITY),
        "NEGATIVE_INFINITY" => Some(f64::NEG_INFINITY),
        "NaN" => Some(f64::NAN),
        _ => None,
    }
}
