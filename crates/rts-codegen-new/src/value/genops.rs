//! The generic JS operators on [`PolyValue`] — the ONE tag-dispatched path that
//! replaces the old engine's AST-shape guessing.
//!
//! There is NO single real runtime symbol for "JS `+` with tag dispatch" — the
//! real runtime exposes lower-level primitives (`STRING_CONCAT`, `STRING_FROM_*`,
//! `STRING_EQ`). So these generic operators are legitimately codegen-owned
//! (`__rtsadp_*`, NOT `__RTS_FN_*`); whenever they touch string BYTES they go
//! through the REAL string pool via [`super::abi_adapter`], never a fake interner.
//!
//! Each takes raw `u64` PolyValue words in and returns a raw `u64` PolyValue word
//! out (the tagged-in/tagged-out convention), except [`__rtsadp_to_boolean`]
//! which returns an UNBOXED i64 0/1 for direct `brif`/`select` consumption. The
//! discriminant is always the tag inside the value — never the source shape.

use rts_runtime::namespaces::gc::string_pool as rt_str;

use super::abi_adapter;
use super::PolyValue;

/// JS `ToString` for any [`PolyValue`], resolving heap strings through the REAL
/// pool. Numbers use the runtime's own JS `Number→String` (STRING_FROM_F64), so
/// formatting matches the rest of the runtime exactly.
///
/// `pub(super)` so the sibling generic-arithmetic trampolines
/// ([`super::genops_arith`]) reuse the SAME path (no divergent formatting).
pub(super) fn to_string(v: PolyValue) -> String {
    if v.is_double() {
        return number_to_string(v.as_f64());
    }
    if v.is_int32() {
        return v.as_i32().to_string();
    }
    if v.is_string() {
        return abi_adapter::resolve_poly(v);
    }
    if v.is_undefined() {
        return "undefined".to_string();
    }
    if v.is_null() {
        return "null".to_string();
    }
    if v.is_bool() {
        return if v.as_bool() { "true" } else { "false" }.to_string();
    }
    if v.is_function() {
        return "function".to_string();
    }
    // An ARRAY (TAG_OBJECT that is NOT a keyed object) ToStrings as its elements
    // joined by "," (JS `String([1,2,3])` = "1,2,3", `String([])` = ""), with
    // `null`/`undefined` elements rendering as the empty string. A keyed OBJECT
    // stays `[object Object]`.
    if v.is_object() && !crate::value::inspect::looks_like_object(v) {
        return array_to_string(v);
    }
    // objects (and any reserved/leaked singleton)
    "[object Object]".to_string()
}

/// JS `Array.prototype.toString` = `join(",")` with `null`/`undefined` elements
/// rendered as the empty string. Reads the REAL Vec behind the array word and
/// recurses through [`to_string`] for each element (so nested arrays flatten the
/// same way JS does).
fn array_to_string(v: PolyValue) -> String {
    use rts_runtime::namespaces::collections::vec as rt_vec;
    use rts_runtime::namespaces::gc::handles as rt_handles;
    let handle = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(v.as_handle());
    let len = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_LEN(handle).max(0);
    let mut out = String::new();
    for i in 0..len {
        if i > 0 {
            out.push(',');
        }
        let w = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_GET(handle, i) as u64;
        let ev = PolyValue::from_raw(w);
        if ev.is_null() || ev.is_undefined() {
            continue; // JS renders null/undefined elements as empty.
        }
        out.push_str(&to_string(ev));
    }
    out
}

/// JS `Number→String` for an `f64`, delegating to the REAL runtime
/// (`STRING_FROM_F64`) so the new engine's number formatting is byte-identical to
/// the rest of RTS (integer-valued doubles drop the `.0`, the non-finite
/// spellings, exponential thresholds — all the runtime's, not a reimplementation).
fn number_to_string(f: f64) -> String {
    let handle = rt_str::__RTS_FN_NS_GC_STRING_FROM_F64(f);
    abi_adapter::real_handle_to_string(handle)
}

/// Is this PolyValue a JS number (either an inline double or a tagged int32)?
pub(super) fn is_number(v: PolyValue) -> bool {
    v.is_double() || v.is_int32()
}

/// JS `ToNumber` for any [`PolyValue`]. Shared by the generic-arithmetic
/// trampolines so numeric coercion is ONE rule, not re-derived per op.
///
/// - number → itself; `true`→1, `false`→0; `null`→0; `undefined`→`NaN`.
/// - string → JS `StringToNumber` (`""`/all-whitespace → 0, else `f64` parse,
///   `NaN` on a non-numeric string).
/// - object/function/hole/empty → `NaN` (basic ToPrimitive; the faithful
///   valueOf/toString chain is a later increment).
pub(super) fn to_number(v: PolyValue) -> f64 {
    if v.is_double() {
        return v.as_f64();
    }
    if v.is_int32() {
        return v.as_i32() as f64;
    }
    if v.is_bool() {
        return if v.as_bool() { 1.0 } else { 0.0 };
    }
    if v.is_null() {
        return 0.0;
    }
    if v.is_string() {
        return string_to_number(&abi_adapter::resolve_poly(v));
    }
    // undefined, object, function, hole, empty → NaN.
    f64::NAN
}

/// JS `StringToNumber`: trim ASCII whitespace; empty → 0; otherwise parse as an
/// `f64` (accepting the `Infinity` spellings), `NaN` on failure. Hex/octal/binary
/// literal prefixes and the full grammar are a later increment.
fn string_to_number(s: &str) -> f64 {
    let t = s.trim();
    if t.is_empty() {
        return 0.0;
    }
    // Non-decimal integer literals (`ToNumber("0b101")` = 5, `0o17` = 15,
    // `0xFF` = 255) — JS recognizes these prefixes in `Number()`/numeric coercion
    // (NOT in `parseInt`/`parseFloat`, which stop at the prefix).
    if let Some(rest) = t.strip_prefix("0b").or_else(|| t.strip_prefix("0B")) {
        return radix_int(rest, 2);
    }
    if let Some(rest) = t.strip_prefix("0o").or_else(|| t.strip_prefix("0O")) {
        return radix_int(rest, 8);
    }
    if let Some(rest) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        return radix_int(rest, 16);
    }
    match t {
        "Infinity" | "+Infinity" => f64::INFINITY,
        "-Infinity" => f64::NEG_INFINITY,
        _ => t.parse::<f64>().unwrap_or(f64::NAN),
    }
}

/// Parse `digits` as a non-decimal integer in `radix`, returning the value as an
/// `f64` (`NaN` on an empty / invalid run — JS `Number("0x")` is `NaN`).
fn radix_int(digits: &str, radix: u32) -> f64 {
    if digits.is_empty() {
        return f64::NAN;
    }
    let mut acc = 0.0f64;
    for c in digits.chars() {
        match c.to_digit(radix) {
            Some(d) => acc = acc * radix as f64 + d as f64,
            None => return f64::NAN,
        }
    }
    acc
}

/// JS `ToBoolean` for any [`PolyValue`], resolving the empty-string case on the
/// heap (a non-empty string is truthy). Shared by the generic `!` would-be path
/// and the relational helpers where a boolean is produced.
pub(super) fn to_boolean(v: PolyValue) -> bool {
    if v.is_string() {
        let h = abi_adapter::real_handle_of(v);
        return rt_str::__RTS_FN_NS_GC_STRING_LEN(h) > 0;
    }
    v.is_truthy()
}

/// Box an `f64` numeric RESULT as the tightest PolyValue: a tagged `int32` when
/// the value is an exact integer in `i32` range (so `2*3` stays `6` int32, the
/// JS small-int fast path), otherwise an inline double. This mirrors the int/
/// double choice [`__rtsadp_add`] makes for `+`.
pub(super) fn number_result(f: f64) -> PolyValue {
    if f.is_finite() && f.fract() == 0.0 && f >= i32::MIN as f64 && f <= i32::MAX as f64 {
        PolyValue::from_i32(f as i32)
    } else {
        PolyValue::from_f64(f)
    }
}

/// Read a string PolyValue's UTF-8 content (for the relational string compare).
pub(super) fn string_content(v: PolyValue) -> String {
    abi_adapter::resolve_poly(v)
}

/// `__rtsadp_add` — the JS binary `+` on two PolyValues.
///
/// - both numbers → numeric add (int32 when both int32 and the sum fits, else
///   double).
/// - either operand a string → ToString both, concatenate in the REAL pool, box
///   the result handle as a string PolyValue.
///
/// The refutation of the old `arr[0] + 5 → "05"` bug: the decision is made on the
/// runtime tags of the ACTUAL values, never on the shape of the source expr.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_add(a: u64, b: u64) -> u64 {
    let av = PolyValue::from_raw(a);
    let bv = PolyValue::from_raw(b);

    // String path: if either side is a string, ToString both and concat through
    // the REAL pool.
    if av.is_string() || bv.is_string() {
        return concat_via_real_pool(av, bv).raw();
    }

    // Numeric path: both numbers.
    if is_number(av) && is_number(bv) {
        if av.is_int32() && bv.is_int32() {
            let lhs = av.as_i32();
            let rhs = bv.as_i32();
            if let Some(sum) = lhs.checked_add(rhs) {
                return PolyValue::from_i32(sum).raw();
            }
            return PolyValue::from_f64(lhs as f64 + rhs as f64).raw();
        }
        let sum = av.number_as_f64() + bv.number_as_f64();
        return PolyValue::from_f64(sum).raw();
    }

    // Mixed/other: JS would ToPrimitive; stringify both (never panics).
    concat_via_real_pool(av, bv).raw()
}

/// ToString both operands and concatenate through the REAL string pool. Each side
/// is interned (STRING_NEW), then STRING_CONCAT joins the two real handles; the
/// result handle is boxed as a string PolyValue via the indirection table.
fn concat_via_real_pool(av: PolyValue, bv: PolyValue) -> PolyValue {
    let ah = real_handle_for_concat(av);
    let bh = real_handle_for_concat(bv);
    let joined = rt_str::__RTS_FN_NS_GC_STRING_CONCAT(ah, bh);
    abi_adapter::poly_from_real_handle(joined)
}

/// The real string handle to feed STRING_CONCAT for one operand: a string's own
/// handle, or a fresh interned ToString of a non-string.
fn real_handle_for_concat(v: PolyValue) -> u64 {
    if v.is_string() {
        abi_adapter::real_handle_of(v)
    } else {
        let s = to_string(v);
        // STRING_NEW (safe extern) reads `len` bytes from a live &str's ptr+len.
        rt_str::__RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64)
    }
}

/// `__rtsadp_strict_eq` — JS `===` on two PolyValues, returning a PolyValue bool.
///
/// Numbers compare cross-representation by value (int32 `7` `===` double `7.0`),
/// NaN is never `===` itself. Strings compare by CONTENT via the REAL pool
/// (`STRING_EQ`) — the indirection table means two equal strings may sit at
/// different idxs, so a raw-word compare is NOT sufficient. Everything else
/// compares by identical representation.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_strict_eq(a: u64, b: u64) -> u64 {
    let av = PolyValue::from_raw(a);
    let bv = PolyValue::from_raw(b);

    let eq = if is_number(av) && is_number(bv) {
        // NaN !== NaN, +0 === -0 — both fall out of IEEE `==` on f64.
        av.number_as_f64() == bv.number_as_f64()
    } else if av.is_string() && bv.is_string() {
        let ah = abi_adapter::real_handle_of(av);
        let bh = abi_adapter::real_handle_of(bv);
        rt_str::__RTS_FN_NS_GC_STRING_EQ(ah, bh) != 0
    } else {
        // Non-number, non-both-string: identical representation ⇒ ===.
        av.raw() == bv.raw()
    };

    PolyValue::bool(eq).raw()
}

/// `__rtsadp_strict_neq` — JS `!==`, the boolean complement of strict-eq.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_strict_neq(a: u64, b: u64) -> u64 {
    let eq = PolyValue::from_raw(__rtsadp_strict_eq(a, b));
    PolyValue::bool(!eq.as_bool()).raw()
}

/// `__rtsadp_loose_eq` — JS `==` (Abstract Equality Comparison) on two PolyValues,
/// returning a PolyValue bool.
///
/// Covers the reachable primitive cases of the spec algorithm:
/// - same-kind (both number, both string, both bool, both null/undefined) →
///   defers to strict-eq's value compare;
/// - `null == undefined` → `true` (and `null`/`undefined` `==` anything else →
///   `false`);
/// - number ↔ string → ToNumber the string, numeric compare (`1 == "1"`,
///   `0 == ""`);
/// - bool on either side → ToNumber the bool (`true == 1`, `false == ""` via two
///   ToNumber steps);
/// - number/string ↔ object → ToPrimitive(object) is a later increment → falls to
///   the strict raw-compare (sound for the proven corpus, never a wrong `true`).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_loose_eq(a: u64, b: u64) -> u64 {
    PolyValue::bool(loose_eq(PolyValue::from_raw(a), PolyValue::from_raw(b))).raw()
}

/// `__rtsadp_loose_neq` — JS `!=`, the boolean complement of loose-eq.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_loose_neq(a: u64, b: u64) -> u64 {
    PolyValue::bool(!loose_eq(PolyValue::from_raw(a), PolyValue::from_raw(b))).raw()
}

/// The JS Abstract Equality Comparison (`==`) as a Rust bool. See
/// [`__rtsadp_loose_eq`] for the covered cases.
fn loose_eq(av: PolyValue, bv: PolyValue) -> bool {
    // null/undefined are loosely-equal to each other and to nothing else.
    let a_nullish = av.is_null() || av.is_undefined();
    let b_nullish = bv.is_null() || bv.is_undefined();
    if a_nullish || b_nullish {
        return a_nullish && b_nullish;
    }
    // Both numbers → numeric compare (NaN never equal; same as strict).
    if is_number(av) && is_number(bv) {
        return av.number_as_f64() == bv.number_as_f64();
    }
    // Both strings → content compare (real pool).
    if av.is_string() && bv.is_string() {
        let ah = abi_adapter::real_handle_of(av);
        let bh = abi_adapter::real_handle_of(bv);
        return rt_str::__RTS_FN_NS_GC_STRING_EQ(ah, bh) != 0;
    }
    // Number/string/bool mix: ToNumber BOTH and compare (this subsumes
    // number==string, bool==number, bool==string, bool==bool-via-number).
    let coercible = |v: PolyValue| is_number(v) || v.is_string() || v.is_bool();
    if coercible(av) && coercible(bv) {
        let x = to_number(av);
        let y = to_number(bv);
        return x == y;
    }
    // Object/function on a side (ToPrimitive is a later increment): fall to the
    // strict identity compare — sound, never a wrong `true`.
    av.raw() == bv.raw()
}

/// `__rtsadp_typeof` — JS `typeof`, returning a PolyValue **string** handle of the
/// `typeof_str` (interned in the REAL pool). A single tag inspection.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_typeof(a: u64) -> u64 {
    let v = PolyValue::from_raw(a);
    abi_adapter::intern_poly(v.typeof_str()).raw()
}

/// `__rtsadp_to_string` — JS `ToString`, returning a PolyValue string handle
/// (interned in the REAL pool). Used by the tests to read results back as text.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_to_string(a: u64) -> u64 {
    // A string already is its own ToString — keep its idx (avoid re-interning).
    let v = PolyValue::from_raw(a);
    if v.is_string() {
        return a;
    }
    let s = to_string(v);
    abi_adapter::intern_poly(&s).raw()
}

/// `__rtsadp_to_boolean` — JS `ToBoolean`, returning an UNBOXED i64 0/1 (NOT a
/// PolyValue) to feed a Cranelift `brif`/`select` directly. The empty-string case
/// needs the heap (length lives in the real pool), so it is resolved here.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_to_boolean(a: u64) -> u64 {
    let v = PolyValue::from_raw(a);
    let truthy = if v.is_string() {
        // A string is truthy iff non-empty — STRING_LEN over the real handle.
        let h = abi_adapter::real_handle_of(v);
        rt_str::__RTS_FN_NS_GC_STRING_LEN(h) > 0
    } else {
        v.is_truthy()
    };
    truthy as u64
}
