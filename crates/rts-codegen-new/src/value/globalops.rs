//! Codegen-owned GLOBAL constant/function + Array/String STATIC trampolines (P5.2).
//!
//! The global coercion functions (`Number(x)`, `String(x)`, `Boolean(x)`,
//! `parseInt`, `parseFloat`, `isNaN`, `isFinite`), the Array statics
//! (`Array.isArray`, `Array.of`, `Array.from`, `Array(n)`), and the String
//! statics (`String.fromCharCode`, `String.fromCodePoint`, `str.split`) all need
//! to produce / inspect the engine's OWN value representation: a [`PolyValue`]
//! word and, for the array-producing ones, a real `Entry::Vec` of boxed PolyValue
//! WORDS (NOT the runtime's raw-i64 element convention). So — exactly like
//! [`super::arrayops`] / [`super::genops`] — these are codegen-owned `__rtsadp_*`
//! trampolines (NOT `__RTS_FN_*`), reusing the SAME genops coercions
//! (`to_number`/`to_string`/`to_boolean`) so number formatting / coercion never
//! diverge from the `+`/`===` path, and the SAME real string pool for bytes.
//!
//! Convention (matching the rest of the `__rtsadp_*` surface): PolyValue words
//! cross as raw `u64`; string args/results cross as real string handles (`u64`); a
//! returned array is a fresh `Entry::Vec` boxed as a `TAG_OBJECT` PolyValue word.

use rts_runtime::namespaces::collections::vec as rt_vec;
use rts_runtime::namespaces::gc::handles as rt_handles;

use super::genops::{to_boolean, to_number};
use super::{abi_adapter, genops, PolyValue};

/// Box a fresh real Vec handle as a `TAG_OBJECT` array PolyValue word (the engine's
/// array representation), matching [`super::arrayops`]'s `slice` result.
fn box_vec_as_array(vec_handle: u64) -> u64 {
    PolyValue::from_object_handle(rt_handles::__RTS_FN_NS_GC_POLY_FROM_HANDLE(vec_handle)).raw()
}

/// JS `ToString` of a PolyValue, interned in the REAL pool, returned as a string
/// PolyValue WORD (the same path `__rtsadp_to_string` uses).
fn to_string_word(v: u64) -> u64 {
    genops::__rtsadp_to_string(v)
}

// ===========================================================================
// Global coercion functions — `Number(x)`, `String(x)`, `Boolean(x)`.
// ===========================================================================

/// `Number(x)` — JS `ToNumber`, the SAME coercion `genops::to_number` performs
/// (so `Number("42")` → 42, `Number(true)` → 1, `Number("x")` → NaN). The result
/// is re-tightened to int32 when exact-in-range (via [`genops::number_result`]).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_g_number(a: u64) -> u64 {
    genops::number_result(to_number(PolyValue::from_raw(a))).raw()
}

/// `String(x)` — JS `ToString`, returning a string PolyValue word (real pool).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_g_string(a: u64) -> u64 {
    to_string_word(a)
}

/// `Boolean(x)` — JS `ToBoolean`, returning a PolyValue bool (the empty-string
/// case resolved on the heap, like the rest of the engine).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_g_boolean(a: u64) -> u64 {
    PolyValue::bool(to_boolean(PolyValue::from_raw(a))).raw()
}

// ===========================================================================
// `parseInt(s, radix)` / `parseFloat(s)` / `isNaN` / `isFinite`.
// ===========================================================================

/// `parseInt(value, radix)` — JS spec subset: ToString the value, trim leading
/// whitespace, an optional sign, an optional `0x`/`0X` prefix when radix is 16 or
/// 0, then consume the longest run of digits valid in the (defaulted) radix. An
/// empty / no-leading-digit string yields `NaN`. `radix` 0 means "auto" (16 if a
/// `0x` prefix, else 10). This matches Node/Bun for the common decimal/binary/hex
/// fixtures; the full grammar (radix-2..36 with letters past `z`) is covered.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_g_parse_int(value: u64, radix: u64) -> u64 {
    let s = poly_to_string(value);
    // `radix` is a raw PolyValue WORD — ToNumber it (NaN/non-integer → 0 = auto).
    let r = to_number(PolyValue::from_raw(radix));
    let radix_i = if r.is_finite() { r as i64 } else { 0 };
    let f = parse_int_str(&s, radix_i);
    genops::number_result(f).raw()
}

/// `parseFloat(value)` — ToString, trim leading whitespace, then parse the longest
/// leading run that is a valid JS float literal (`[+-]?(Infinity | digits . digits
/// e±digits)`). Trailing garbage is ignored (`parseFloat("3.14x")` → 3.14). No
/// leading float → `NaN`.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_g_parse_float(value: u64) -> u64 {
    let s = poly_to_string(value);
    genops::number_result(parse_float_str(&s)).raw()
}

/// `isNaN(x)` — JS global: `Number.isNaN(ToNumber(x))`. Returns a PolyValue bool.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_g_is_nan(a: u64) -> u64 {
    PolyValue::bool(to_number(PolyValue::from_raw(a)).is_nan()).raw()
}

/// `isFinite(x)` — JS global: `ToNumber(x)` is a finite number. Returns a bool.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_g_is_finite(a: u64) -> u64 {
    PolyValue::bool(to_number(PolyValue::from_raw(a)).is_finite()).raw()
}

// ===========================================================================
// `Array.isArray(x)` — the array-vs-object runtime discriminator.
// ===========================================================================

/// `Array.isArray(x)` — `1` iff `x` is a `TAG_OBJECT` PolyValue that is NOT a
/// runtime OBJECT (an object literal / class instance carries a slot-0 shape-id
/// header — see [`super::inspect::looks_like_object`]). Everything else (numbers,
/// strings, null, functions, plain objects) is `0`. Reuses the SAME discriminator
/// the inspect path uses, so the two never disagree.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_arr_is_array(a: u64) -> u64 {
    let v = PolyValue::from_raw(a);
    let is_array = v.is_object() && !super::inspect::looks_like_object(v);
    PolyValue::bool(is_array).raw()
}

// ===========================================================================
// `Array.of(...)` / `Array.from(...)` / `new Array(n)` / `Array(n)`.
// ===========================================================================

/// `Array(n)` / `new Array(n)` — a fresh array of length `n` whose `n` holes read
/// `undefined` (the engine fills them with the `undefined` PolyValue word, which
/// is what `arr[i]`/inspect both then see). `n` is a real JS number word; a
/// non-integer / negative / out-of-range `n` yields an empty array (the runtime
/// would throw a RangeError — a later increment; an empty array is the safe,
/// never-wrong fallback the lowering only reaches for an integer literal anyway).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_arr_new_sized(n_word: u64) -> u64 {
    let n = to_number(PolyValue::from_raw(n_word));
    let vec = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW();
    if n.is_finite() && n.fract() == 0.0 && n >= 0.0 && n <= (1u64 << 31) as f64 {
        let undef = PolyValue::undefined().raw() as i64;
        for _ in 0..(n as i64) {
            rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(vec, undef);
        }
    }
    box_vec_as_array(vec)
}

/// `Array.from(arrayLike)` — supports the two reachable shapes: a STRING (→ a char
/// array, each element a one-char string PolyValue) and an ARRAY (→ a shallow
/// copy). Any other value (Map/Set/iterator/typed-array/array-like-with-length)
/// yields the special sentinel [`FROM_UNSUPPORTED`] so the LOWERING bails (the
/// honesty floor: never a wrong-but-closer value). The lowering checks the result
/// against the sentinel and emits an explicit `Unsupported` when seen.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_arr_from(a: u64) -> u64 {
    let v = PolyValue::from_raw(a);
    if v.is_string() {
        let s = abi_adapter::resolve_poly(v);
        let vec = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW();
        for ch in s.chars() {
            let word = abi_adapter::intern_poly(&ch.to_string()).raw() as i64;
            rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(vec, word);
        }
        return box_vec_as_array(vec);
    }
    if v.is_object() && !super::inspect::looks_like_object(v) {
        // A real array → shallow copy of its boxed element words.
        let src = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(v.as_handle());
        let len = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_LEN(src).max(0);
        let vec = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW();
        for i in 0..len {
            let w = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_GET(src, i);
            rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(vec, w);
        }
        return box_vec_as_array(vec);
    }
    FROM_UNSUPPORTED
}

/// Sentinel returned by [`__rtsadp_arr_from`] for an unsupported arrayLike (Map/
/// Set/iterator/etc). A raw `u64` that is NOT a valid array word — the lowering
/// compares the result against it and bails. (It is the `empty` singleton word,
/// which never escapes as a user value, so it is an unambiguous marker.)
pub const FROM_UNSUPPORTED: u64 = PolyValue::empty().raw();

// ===========================================================================
// String statics — `String.fromCharCode(...)`, `String.fromCodePoint(...)`.
// ===========================================================================

/// `String.fromCharCode(code)` for ONE code unit — JS truncates to a u16. The
/// lowering calls this once per argument and concatenates the results through the
/// generic `+` (real `STRING_CONCAT`), so the variadic form falls out of the
/// monadic primitive. Returns a string PolyValue WORD. Reuses the runtime's own
/// `__RTS_FN_GL_STRING_FROM_CHAR_CODE` for the byte production (UTF-16→UTF-8
/// lossy, lone-surrogate handling), then boxes its handle as a PolyValue.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_str_from_char_code(code: u64) -> u64 {
    let n = to_number(PolyValue::from_raw(code)) as i64;
    let handle = rts_runtime::namespaces::globals::string::rt::__RTS_FN_GL_STRING_FROM_CHAR_CODE(n);
    abi_adapter::poly_from_real_handle(handle).raw()
}

/// `String.fromCharCode(...codes)` over a spread ARRAY (`fromCharCode(...arr)`):
/// concatenate `fromCharCode` of each element. `arr_word` is a proven array; each
/// element is `ToNumber`'d to a code unit. Returns a string PolyValue WORD (real
/// pool). The bytes go through the runtime's own `FROM_CHAR_CODE` per element, so
/// surrogate handling matches the scalar path.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_str_from_char_code_arr(arr_word: u64) -> u64 {
    let v = PolyValue::from_raw(arr_word);
    let handle = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(v.as_handle());
    let len = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_LEN(handle).max(0);
    let mut out = String::new();
    for i in 0..len {
        let w = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_GET(handle, i) as u64;
        let code_word = __rtsadp_str_from_char_code(w);
        out.push_str(&abi_adapter::resolve_poly(PolyValue::from_raw(code_word)));
    }
    abi_adapter::intern_poly(&out).raw()
}

/// `String.fromCodePoint(code)` for ONE code point. Same shape as
/// [`__rtsadp_str_from_char_code`] but full Unicode code points.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_str_from_code_point(code: u64) -> u64 {
    let n = to_number(PolyValue::from_raw(code)) as i64;
    let handle = rts_runtime::namespaces::globals::string::rt::__RTS_FN_GL_STRING_FROM_CODE_POINT(n);
    abi_adapter::poly_from_real_handle(handle).raw()
}

// ===========================================================================
// `str.split(sep, limit)` — PolyValue-aware (boxes each substring).
// ===========================================================================

/// `str.split(sepHandle, limit)` — split `recv` (a real string handle) on the
/// string separator `sep` (a real string handle), producing a fresh array
/// (`Entry::Vec`) whose elements are STRING PolyValue WORDS (NOT the runtime's
/// raw-handle convention — so `.length`/`.join`/`[i]` on the result work like any
/// engine array). `limit < 0` means "no limit". An empty separator splits into
/// individual chars (JS spec). The bytes go through the REAL pool. This is the
/// PolyValue-native analogue of `__RTS_FN_GL_STRING_SPLIT` (whose Vec holds raw
/// handles the engine could not interpret).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_str_split(recv: u64, sep: u64, limit: i64) -> u64 {
    let s = abi_adapter::real_handle_to_string(recv);
    let delim = abi_adapter::real_handle_to_string(sep);
    let vec = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_NEW();
    let take = if limit < 0 { usize::MAX } else { limit as usize };
    let push = |part: &str| {
        let word = abi_adapter::intern_poly(part).raw() as i64;
        rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(vec, word);
    };
    if delim.is_empty() {
        // JS: split("") = individual chars (no leading/trailing empties).
        for ch in s.chars().take(take) {
            push(&ch.to_string());
        }
    } else {
        for part in s.split(delim.as_str()).take(take) {
            push(part);
        }
    }
    box_vec_as_array(vec)
}

// ===========================================================================
// Spread support — fold a variadic numeric op over a spread array; append one
// array's elements into another (array-literal spread `[...a, ...b]`).
// ===========================================================================

/// `__rtsadp_math_reduce(arr_word, op)` — fold `Math.min/max/hypot` over the
/// elements of the array `arr_word` (spread `Math.max(...xs)`), returning an f64.
/// `op`: `0` = min, `1` = max, `2` = hypot. Each element is `ToNumber`'d (the same
/// coercion the scalar path uses). An empty array yields the JS identity:
/// `Math.min()` = `+Infinity`, `Math.max()` = `-Infinity`, `Math.hypot()` = `0`.
/// A `NaN` element makes min/max `NaN` (JS NaN-propagation).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_math_reduce(arr_word: u64, op: i64) -> f64 {
    let v = PolyValue::from_raw(arr_word);
    let handle = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(v.as_handle());
    let len = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_LEN(handle).max(0);
    let mut acc = match op {
        0 => f64::INFINITY,
        1 => f64::NEG_INFINITY,
        _ => 0.0,
    };
    for i in 0..len {
        let w = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_GET(handle, i) as u64;
        let x = to_number(PolyValue::from_raw(w));
        acc = match op {
            0 => acc.min(x),
            1 => acc.max(x),
            _ => acc.hypot(x),
        };
        // JS min/max propagate NaN (Rust's f64::min/max do NOT — guard).
        if x.is_nan() && op != 2 {
            return f64::NAN;
        }
    }
    acc
}

/// `__rtsadp_canon_double(word)` — `ToNumber` the PolyValue `word` and return it
/// as a GUARANTEED inline-double PolyValue word (never a tagged int32). The
/// front-end uses this to normalize a spread array element to a word the pure
/// `emit_unbox_double` bitcast handles, before coercing to a native numeric param
/// (a boxed-int32 element would otherwise bitcast to a bogus f64). NaN-canonical.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_canon_double(word: u64) -> u64 {
    PolyValue::from_f64(to_number(PolyValue::from_raw(word))).raw()
}

/// `__rtsadp_arr_spread_append(dst_word, src_word)` — push every element of the
/// array `src_word` onto the array `dst_word` (array-literal spread `[...src]`).
/// Reads the raw element WORDS through the real Vec (preserving each boxed
/// PolyValue), so the spread copy is shallow and representation-faithful. A
/// non-array `src` is a no-op (the lowering only routes proven arrays here).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_arr_spread_append(dst_word: u64, src_word: u64) {
    let src = PolyValue::from_raw(src_word);
    if !(src.is_object() && !super::inspect::looks_like_object(src)) {
        return;
    }
    let dst = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(PolyValue::from_raw(dst_word).as_handle());
    let s = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(src.as_handle());
    let len = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_LEN(s).max(0);
    for i in 0..len {
        let w = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_GET(s, i);
        rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_PUSH(dst, w);
    }
}

// ===========================================================================
// String parsing helpers (parseInt / parseFloat grammar).
// ===========================================================================

/// ToString a PolyValue word for the parse functions (a string stays itself;
/// numbers/etc. go through the engine's ToString — JS `parseInt(42)` parses
/// `"42"`).
fn poly_to_string(v: u64) -> String {
    let pv = PolyValue::from_raw(v);
    if pv.is_string() {
        abi_adapter::resolve_poly(pv)
    } else {
        let s_word = genops::__rtsadp_to_string(v);
        abi_adapter::resolve_poly(PolyValue::from_raw(s_word))
    }
}

/// JS `parseInt(s, radix)` returning the parsed `f64` (`NaN` on failure). `radix`
/// 0 = auto (16 with a `0x` prefix, else 10); 2..=36 fixes the base. Leading
/// whitespace + an optional sign are consumed; the longest valid-digit run is
/// parsed; trailing garbage is ignored.
fn parse_int_str(s: &str, radix: i64) -> f64 {
    let t = s.trim_start();
    let (neg, rest) = match t.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, t.strip_prefix('+').unwrap_or(t)),
    };
    let mut base = radix;
    let mut digits = rest;
    if base == 16 || base == 0 {
        if let Some(r) = digits.strip_prefix("0x").or_else(|| digits.strip_prefix("0X")) {
            digits = r;
            base = 16;
        }
    }
    if base == 0 {
        base = 10;
    }
    if !(2..=36).contains(&base) {
        return f64::NAN;
    }
    let radix_u = base as u32;
    let valid: String = digits
        .chars()
        .take_while(|c| c.to_digit(radix_u).is_some())
        .collect();
    if valid.is_empty() {
        return f64::NAN;
    }
    // Accumulate in f64 to keep large magnitudes (JS parseInt returns a double).
    let mut acc = 0.0f64;
    for c in valid.chars() {
        acc = acc * base as f64 + c.to_digit(radix_u).unwrap() as f64;
    }
    if neg {
        -acc
    } else {
        acc
    }
}

/// JS `parseFloat(s)` returning the parsed `f64` (`NaN` on no leading float). The
/// longest leading run matching a JS float literal is parsed; trailing garbage is
/// ignored. `Infinity`/`+Infinity`/`-Infinity` are recognized.
fn parse_float_str(s: &str) -> f64 {
    let t = s.trim_start();
    for spell in ["Infinity", "+Infinity"] {
        if t.starts_with(spell) {
            return f64::INFINITY;
        }
    }
    if t.starts_with("-Infinity") {
        return f64::NEG_INFINITY;
    }
    let bytes = t.as_bytes();
    let mut end = 0usize;
    // optional sign
    if end < bytes.len() && (bytes[end] == b'+' || bytes[end] == b'-') {
        end += 1;
    }
    let mut saw_digit = false;
    while end < bytes.len() && bytes[end].is_ascii_digit() {
        end += 1;
        saw_digit = true;
    }
    if end < bytes.len() && bytes[end] == b'.' {
        end += 1;
        while end < bytes.len() && bytes[end].is_ascii_digit() {
            end += 1;
            saw_digit = true;
        }
    }
    if !saw_digit {
        return f64::NAN;
    }
    // optional exponent
    if end < bytes.len() && (bytes[end] == b'e' || bytes[end] == b'E') {
        let mut e = end + 1;
        if e < bytes.len() && (bytes[e] == b'+' || bytes[e] == b'-') {
            e += 1;
        }
        let mut saw_exp = false;
        while e < bytes.len() && bytes[e].is_ascii_digit() {
            e += 1;
            saw_exp = true;
        }
        if saw_exp {
            end = e;
        }
    }
    t[..end].parse::<f64>().unwrap_or(f64::NAN)
}
