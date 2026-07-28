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

use super::PolyValue;
use super::abi_adapter;

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
    if v.is_undefined() || v.is_hole() {
        // A leaked HOLE word reads as `undefined` everywhere user code can see it.
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
    if v.is_object() && !crate::adapters::value::inspect::looks_like_object(v) {
        // A REAL array joins its elements; an OPAQUE runtime entry (Date /
        // RegExp / …) ToStrings through the runtime-layer authority
        // (`OPAQUE_TO_STRING` — entry-kind dispatch, no class name here);
        // an unknown opaque entry keeps the object default.
        use rts_engine::heap::handles::{Entry, with_entry};
        use rts_runtime::namespaces::gc::handles as rt_handles;
        let h = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(v.as_handle());
        let is_vec = with_entry(h, |e| matches!(e, Some(Entry::Vec(_))));
        if is_vec {
            return array_to_string(v);
        }
        let s = rts_runtime::namespaces::globals::date::coercion::__RTS_FN_RT_OPAQUE_TO_STRING(h);
        if s != 0 {
            return abi_adapter::real_handle_to_string(s);
        }
        return "[object Object]".to_string();
    }
    // A keyed object with a `[Symbol.toPrimitive]` method: ToString runs it with
    // hint "string" and stringifies the primitive result (spec ToPrimitive).
    if let Some(p) = to_primitive_via_method(v, "string") {
        return to_string(p);
    }
    // objects (and any reserved/leaked singleton)
    "[object Object]".to_string()
}

/// JS `ToPrimitive(v, hint)` OBJECT step: when `v` is a keyed object, run the spec
/// ToPrimitive algorithm.
///
/// 1. An own `[Symbol.toPrimitive](hint)` method (the object-literal desugar
///    recovers it as the `@@toPrimitive` own-prop fn slot — see `desugar::objmethod`)
///    takes precedence: invoke it with the hint string and return the PRIMITIVE
///    result.
/// 2. Otherwise OrdinaryToPrimitive: with hint `"string"` try `toString` then
///    `valueOf`; with hint `"number"`/`"default"` try `valueOf` then `toString`.
///    Each own method that IS a function is called with no args; the first one to
///    return a primitive wins.
///
/// `None` = not a keyed object, no usable method found, or every method returned a
/// non-primitive (spec: TypeError; here the caller keeps its default coercion —
/// never a wrong recursion).
pub(super) fn to_primitive_via_method(v: PolyValue, hint: &str) -> Option<PolyValue> {
    if !v.is_object() || !crate::adapters::value::inspect::looks_like_object(v) {
        return None;
    }
    // 1. `[Symbol.toPrimitive]` (canonicalized to the `@@toPrimitive` own slot).
    if let Some(p) = invoke_prim_method(v, "@@toPrimitive", Some(hint)) {
        return Some(p);
    }
    // 2. OrdinaryToPrimitive: the method order depends on the hint. `"string"`
    //    tries `toString` first; every other hint (`"number"`/`"default"`) tries
    //    `valueOf` first.
    let order: [&str; 2] = if hint == "string" {
        ["toString", "valueOf"]
    } else {
        ["valueOf", "toString"]
    };
    for name in order {
        if let Some(p) = invoke_prim_method(v, name, None) {
            return Some(p);
        }
    }
    None
}

/// Fetch the own method `name` on keyed object `v`; if it is a function, invoke it
/// (with `hint` as the sole argument when `Some`, else no args) and return the
/// result IFF it is a primitive. `None` when the slot is absent / not a function /
/// the call produced a non-primitive.
fn invoke_prim_method(v: PolyValue, name: &str, hint: Option<&str>) -> Option<PolyValue> {
    let key = abi_adapter::intern_poly(name).raw();
    let f = super::objops::__rtsadp_obj_get(v.raw(), key);
    if !PolyValue::from_raw(f).is_function() {
        return None;
    }
    let undef = PolyValue::undefined().raw();
    let a0 = match hint {
        Some(h) => abi_adapter::intern_poly(h).raw(),
        None => undef,
    };
    let r = super::funcops::__rtsadp_fn_invoke_method(f, v.raw(), a0, undef, undef, undef);
    let p = PolyValue::from_raw(r);
    if p.is_object() || p.is_function() {
        return None;
    }
    Some(p)
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
        if ev.is_null() || ev.is_undefined() || ev.is_hole() {
            continue; // JS renders null/undefined/hole elements as empty.
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

/// JS `ToNumber` for a raw PolyValue word — re-exposed to the `value` siblings
/// (the dynamic-dispatch trampolines) so numeric arg coercion is the SAME rule as
/// `+`/`<`/…, not re-derived per call site.
pub(crate) fn dyn_to_number(word: u64) -> f64 {
    to_number(PolyValue::from_raw(word))
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
    // A keyed object with a `[Symbol.toPrimitive]` method: ToNumber runs it with
    // hint "number" and coerces the primitive result (spec ToPrimitive).
    if let Some(p) = to_primitive_via_method(v, "number") {
        return to_number(p);
    }
    // An opaque runtime entry (Date / RegExp / array / …): ToPrimitive with hint
    // "number" runs `valueOf` FIRST. A `Date` exposes a NUMERIC `valueOf` (its time
    // value), so `+date` / `date < date` use the timestamp — dispatched by ENTRY
    // KIND in the runtime layer (`OPAQUE_HAS_NUMBER`/`OPAQUE_TO_NUMBER`), no class
    // name here. Everything else (arrays, RegExp) has no numeric `valueOf` and
    // falls to ToString then re-parse — `-[]` → -0, `+[5]` → 5, `+[1,2]` → NaN,
    // `+/re/` → NaN.
    if v.is_object() && !super::inspect::looks_like_object(v) {
        use rts_runtime::namespaces::globals::date::coercion as rt_date;
        let h = abi_adapter::real_handle_of(v);
        if rt_date::__RTS_FN_RT_OPAQUE_HAS_NUMBER(h) != 0 {
            return rt_date::__RTS_FN_RT_OPAQUE_TO_NUMBER(h);
        }
        return string_to_number(&to_string(v));
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
    // NEGATIVE ZERO stays an inline double — the int32 0 would lose the sign
    // (`-0 % 5`, `Object.is(x, -0)`, `-0` renderings).
    if f == 0.0 && f.is_sign_negative() {
        return PolyValue::from_f64(f);
    }
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

    // JS `+` runs ToPrimitive(default) on BOTH operands FIRST (spec 13.15.3):
    // an object with a `[Symbol.toPrimitive]` method converts through it even
    // when the other side is already a string (`obj + ""` gets hint "default",
    // NOT the concat's later ToString hint "string"). One re-entry with the
    // primitive results; without the method the tag checks below decide as
    // before.
    if av.is_object() || bv.is_object() {
        let ap = to_primitive_via_method(av, "default");
        let bp = to_primitive_via_method(bv, "default");
        if ap.is_some() || bp.is_some() {
            let a2 = ap.map_or(a, |p| p.raw());
            let b2 = bp.map_or(b, |p| p.raw());
            return __rtsadp_add(a2, b2);
        }
    }

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

    // Mixed/other with NO string side: an OBJECT/ARRAY/FUNCTION side stringifies
    // through its ToString (`[object Object]` / joined elements) and
    // concatenates — the JS default-ToPrimitive outcome for a method-less
    // object. Two NON-object sides (number/bool/null/undefined) are the NUMERIC
    // `+`: ToNumber both (undefined → NaN, null → 0, true → 1), matching JS —
    // `1 + undefined` is `NaN`, never the string `"1undefined"`.
    if !av.is_object() && !av.is_function() && !bv.is_object() && !bv.is_function() {
        return number_result(to_number(av) + to_number(bv)).raw();
    }
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
/// `Object.is(a, b)` — JS SameValue. Like `===` EXCEPT: `Object.is(NaN, NaN)` is
/// `true` and `Object.is(0, -0)` is `false`. Both differences live in the number
/// branch (string/other identity match `===`).
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_same_value(a: u64, b: u64) -> u64 {
    let av = PolyValue::from_raw(a);
    let bv = PolyValue::from_raw(b);
    let same = if is_number(av) && is_number(bv) {
        let (x, y) = (av.number_as_f64(), bv.number_as_f64());
        if x.is_nan() && y.is_nan() {
            true // SameValue: NaN is NaN
        } else if x == 0.0 && y == 0.0 {
            x.is_sign_negative() == y.is_sign_negative() // +0 is NOT -0
        } else {
            x == y
        }
    } else if av.is_string() && bv.is_string() {
        let ah = abi_adapter::real_handle_of(av);
        let bh = abi_adapter::real_handle_of(bv);
        rt_str::__RTS_FN_NS_GC_STRING_EQ(ah, bh) != 0
    } else if av.is_function() && bv.is_function() {
        super::funcops::fn_value_identity_eq(a, b)
    } else {
        av.raw() == bv.raw()
    };
    PolyValue::bool(same).raw()
}

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
    } else if av.is_function() && bv.is_function() {
        // A function value re-reifies a fresh Entry each reference, so raw-word
        // compare would say `f !== f`. Compare stable identity instead.
        super::funcops::fn_value_identity_eq(a, b)
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
    // OBJECT vs primitive: ToPrimitive the object side (default hint — array
    // join / `[object Object]`, matching the engine's single ToString path,
    // exact for method-free objects) and recurse (`[] == ""` → true,
    // `[0] == 0` → true). Object vs object stays IDENTITY (JS never coerces
    // both sides).
    let a_obj = av.is_object();
    let b_obj = bv.is_object();
    if a_obj != b_obj {
        let (obj, prim) = if a_obj { (av, bv) } else { (bv, av) };
        let s = PolyValue::from_raw(__rtsadp_to_string(obj.raw()));
        return loose_eq(s, prim);
    }
    // Object/object (or function): strict identity — sound, never a wrong `true`.
    av.raw() == bv.raw()
}

/// `__rtsadp_typeof` — JS `typeof`, returning a PolyValue **string** handle of the
/// `typeof_str` (interned in the REAL pool). A single tag inspection.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_typeof(a: u64) -> u64 {
    let v = PolyValue::from_raw(a);
    // A Symbol instance rides as an OBJECT word (no distinct tag yet, #216) —
    // `typeof` must still read `"symbol"` (`typeof Symbol.iterator`, a
    // symbol-holding variable the front can't classify statically).
    if v.is_object() {
        use rts_runtime::namespaces::gc::handles as rt_handles;
        let h = rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(v.as_handle());
        let is_sym =
            rt_handles::with_entry(h, |e| matches!(e, Some(rt_handles::Entry::Symbol { .. })));
        if is_sym {
            return abi_adapter::intern_poly("symbol").raw();
        }
    }
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

/// `__rtsadp_await` — JS `await <expr>` under the interim SYNCHRONOUS model
/// (#207). Async fns run synchronously and return their value directly, so
/// `await` is usually the identity — but a REAL Promise (`Promise.resolve(..)`,
/// the `promise` namespace, the combinators) rides as a NUMBER word holding the
/// raw `u64` HandleTable id. Detect exactly that shape — an integral number
/// ≥ 2^48 whose live entry is `PromiseAsync` — and block on `PROMISE_WAIT`
/// (which pumps microtasks/timers while pending), reboxing the settled value.
/// Anything else passes through untouched.
///
/// False positives are impossible: a live handle id carries a non-zero 16-bit
/// generation in the top bits, so its numeric value is always ≥ 2^48 — ordinary
/// JS numbers never reach the table lookup.
///
/// A REJECTED promise records the settled error word in the codegen pending-
/// error slot (the same one `throw` uses), so an enclosing `try/catch` — or the
/// host-side uncaught check after `main` — surfaces it; the returned word is
/// `undefined` in that case.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_await(word: u64) -> u64 {
    use rts_engine::heap::handles::{Entry, with_entry};
    use rts_runtime::namespaces::gc::handles as rt_handles;
    let v = PolyValue::from_raw(word);
    // Tag-dispatch, no static guessing. An OBJECT word may be a boxed Promise
    // (`new Promise(executor)` returns its handle through the heterogeneous
    // `__rtsadp_box_handle_auto`, which tags a PromiseAsync entry OBJECT) —
    // reconstruct the full handle from the live slot and wait on it too.
    let handle = if v.is_object() {
        rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(v.as_handle())
    } else {
        let f = if v.is_double() {
            v.as_f64()
        } else if v.is_int32() {
            v.as_i32() as f64
        } else {
            return word;
        };
        const HANDLE_MIN: f64 = 281_474_976_710_656.0; // 2^48 (generation ≥ 1)
        if !(f.is_finite() && f >= HANDLE_MIN && f <= u64::MAX as f64 && f.fract() == 0.0) {
            return word;
        }
        f as u64
    };
    let is_promise = with_entry(handle, |e| matches!(e, Some(Entry::PromiseAsync(_))));
    if !is_promise {
        return word;
    }
    use rts_runtime::namespaces::promise as rt_promise;
    let mut h = handle;
    loop {
        // `wait_raw` — the settled value VERBATIM (a new-engine word or a legacy
        // raw i64); `rebox_settled` below does the honest rebox. The PUBLIC
        // `PROMISE_WAIT` normalizes for the TS i64 surface and would round a
        // fractional double word.
        let settled = rt_promise::wait_raw(h);
        let rejected = rt_promise::__RTS_FN_NS_PROMISE_STATE(h) == 2;
        // JS `await` FLATTENS: a promise settled with another promise resolves to
        // the inner value. Follow the raw-id chain (finite — each hop consumes one
        // settled promise) before reboxing.
        let sw = settled as u64;
        if !rejected
            && sw >= (1 << 48)
            && with_entry(sw, |e| matches!(e, Some(Entry::PromiseAsync(_))))
        {
            h = sw;
            continue;
        }
        let settled_word = rebox_settled(settled);
        if rejected {
            super::errslot::__rtsadp_throw_set(settled_word);
            return PolyValue::undefined().raw();
        }
        return settled_word;
    }
}

/// Rebox a Promise's settled `i64` into a PolyValue word. The slot stores
/// whatever the producer wrote: a NEW-engine producer wrote a PolyValue word
/// (already boxed — pass through); a raw-`i64` producer (the legacy `promise`
/// namespace ABI) wrote a plain integer OR a raw HandleTable id (the combinators
/// settle `Promise.all` with the results `Entry::Vec` handle) — box those as a
/// number / heap value respectively.
fn rebox_settled(raw: i64) -> u64 {
    let w = raw as u64;
    if PolyValue::from_raw(w).is_boxed() {
        return w;
    }
    // A raw heap-handle id (generation ≥ 1 ⇒ value ≥ 2^48, exact): rebox as the
    // right PolyValue heap kind so `.length`/indexing/template printing work on
    // the awaited result (the combinators settle with the results Vec's id).
    if w >= (1 << 48) {
        use rts_engine::heap::handles::{Entry, with_entry};
        let kind = with_entry(w, |e| {
            e.map(|entry| match entry {
                Entry::String(_) => 1u8,
                Entry::Vec(_) | Entry::Map(_) => 2,
                _ => 0,
            })
        });
        match kind {
            Some(1) => {
                return super::abi_adapter::poly_from_real_handle(w).raw();
            }
            Some(2) => {
                use rts_runtime::namespaces::gc::handles as rt_handles;
                let slot = rt_handles::__RTS_FN_NS_GC_POLY_FROM_HANDLE(w);
                return PolyValue::from_object_handle(slot).raw();
            }
            _ => {}
        }
    }
    // Not boxed, not a live handle: a legacy raw int (exact i32) or an
    // inline-double PolyValue word (huge bits when read as an integer).
    if let Ok(i) = i32::try_from(raw) {
        return PolyValue::from_i32(i).raw();
    }
    w
}

/// `__rtsadp_box_handle_auto` — box a RAW runtime handle return into the right
/// PolyValue by its LIVE entry kind: `0` → `null`; `Entry::String` → a string
/// word; `Entry::Vec`/anything else → an object word. A Vec's elements are
/// NORMALIZED in place: a legacy producer (`s.match(re)`, `URLSearchParams`
/// getters) stores RAW string-handle i64s (they read back as denormal doubles),
/// which are reboxed to real string words; a nested Vec recurses (bounded).
/// This is the ONE heterogeneous-handle-return authority — tag-dispatch at
/// runtime, no static guessing.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_box_handle_auto(h: u64) -> u64 {
    box_handle_auto_depth(h, 0)
}

fn box_handle_auto_depth(h: u64, depth: u32) -> u64 {
    use rts_engine::heap::handles::{Entry, with_entry};
    use rts_runtime::namespaces::collections::vec as rt_vec;
    use rts_runtime::namespaces::gc::handles as rt_handles;
    if h == 0 {
        return PolyValue::null().raw();
    }
    let kind = with_entry(h, |e| match e {
        Some(Entry::String(_)) => 1u8,
        Some(Entry::Vec(_)) => 2,
        // A callable entry boxes as TAG_FUNCTION — an OBJECT-tagged function
        // word fails every invoke path ("not a function"). Reached by e.g. the
        // `resolve`/`reject` members of `Promise.withResolvers()`.
        Some(Entry::Function(_)) => 4,
        Some(_) => 3,
        None => 0,
    });
    match kind {
        1 => abi_adapter::poly_from_real_handle(h).raw(),
        4 => {
            let slot = rt_handles::__RTS_FN_NS_GC_POLY_FROM_HANDLE(h);
            PolyValue::from_function_handle(slot).raw()
        }
        2 => {
            // Normalize legacy RAW elements (outside the entry lock — VEC_GET
            // re-locks the shard). A raw handle is NOT a boxed word and NOT a
            // plausible double (its bits read as a denormal ≥ 2^48 as an int).
            if depth < 4 {
                let len = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_LEN(h).max(0);
                for i in 0..len {
                    let e = rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_GET(h, i) as u64;
                    if !PolyValue::from_raw(e).is_boxed() && e >= (1 << 48) {
                        // ANY live entry normalizes (String → str word, Vec →
                        // nested array word, Map/other → object word) —
                        // `matchAll` rows are raw `Entry::Map` handles.
                        let is_heap = with_entry(e, |en| en.is_some());
                        if is_heap {
                            let w = box_handle_auto_depth(e, depth + 1);
                            rt_vec::__RTS_FN_NS_COLLECTIONS_VEC_SET(h, i, w as i64);
                        }
                    }
                }
            }
            let slot = rt_handles::__RTS_FN_NS_GC_POLY_FROM_HANDLE(h);
            PolyValue::from_object_handle(slot).raw()
        }
        3 => {
            let slot = rt_handles::__RTS_FN_NS_GC_POLY_FROM_HANDLE(h);
            PolyValue::from_object_handle(slot).raw()
        }
        _ => PolyValue::null().raw(),
    }
}

/// `__rtsadp_word_to_abi_i64` — marshal a Tagged PolyValue word to a raw-`i64`
/// ABI slot (`I64`/`U64` namespace params), dispatching on the TAG instead of
/// assuming a number: a heap value (string/object/function) yields its REAL
/// HandleTable id (the collections/promise namespaces take handles in `U64`
/// params); a number yields its integer truncation; bool 0/1; anything else 0.
/// This replaces the blind saturating float-convert that turned an awaited
/// `Promise.all` results-array (an OBJECT word) into garbage before `vec_len`.
#[unsafe(no_mangle)]
pub extern "C" fn __rtsadp_word_to_abi_i64(a: u64) -> i64 {
    use rts_runtime::namespaces::gc::handles as rt_handles;
    let v = PolyValue::from_raw(a);
    if v.is_string() || v.is_object() || v.is_function() {
        return rt_handles::__RTS_FN_NS_GC_POLY_TO_HANDLE(v.as_handle()) as i64;
    }
    if v.is_double() {
        let f = v.as_f64();
        if f.is_finite() { f as i64 } else { 0 }
    } else if v.is_int32() {
        v.as_i32() as i64
    } else if v.is_bool() {
        v.as_bool() as i64
    } else {
        0
    }
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
