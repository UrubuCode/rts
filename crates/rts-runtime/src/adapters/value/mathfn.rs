//! `Math` methods reached as first-class FUNCTION VALUES.
//!
//! The static call form (`Math.abs(x)`) is lowered by the engine straight to the
//! `__RTS_FN_NS_MATH_*` extern; this module covers the OTHER form — the method
//! read as a value (`const f = Math.clz32`, `arr.map(Math.abs)`,
//! `Reflect.apply(Math.max, null, xs)`) — by handing back a real callable whose
//! env slot names the method. Both forms end up in the SAME extern, so the two
//! cannot diverge.
//!
//! Split out of [`super::funcops`] (which is already far over the 500-line
//! ceiling) rather than appended to it.

use rts_runtime::namespaces::gc::handles::{Entry, FunctionData, alloc_entry};

use super::PolyValue;

/// Every `Math` METHOD reachable as a first-class function VALUE, in a FIXED
/// order. This is the SINGLE source both sides read: the engine's member-read
/// path (`rts-codegen-new/src/front/run/mathobj.rs`) resolves a property name to
/// its position here, and [`math_method_thunk`] dispatches on the same entry — so
/// a reordering can never desynchronize them.
///
/// The op code carried in the env slot is the position **plus one**, because the
/// uniform invoker ([`__rtsadp_fn_invoke`]) reserves `env == 0` as the "this
/// function captures nothing" sentinel and rewrites it to the `undefined` word
/// before the thunk ever sees it. With a 0-based code the FIRST entry (`abs`)
/// reached the thunk as a huge index, missed the table and returned `undefined` —
/// `typeof Math.abs` said `"function"` while `Math.abs(-9)` gave `undefined`.
/// Use [`math_fn_op_code`]/[`math_fn_op_name`] rather than indexing directly.
///
/// `Math` is PRIMORDIAL (owner decision 2026-07-03), so naming its members is
/// allowed; the values are computed by the very same `__RTS_FN_NS_MATH_*` externs
/// the static call path emits, never a re-implementation.
pub const MATH_FN_OPS: &[&str] = &[
    "abs", "acos", "acosh", "asin", "asinh", "atan", "atan2", "atanh", "cbrt", "ceil", "clz32",
    "cos", "cosh", "exp", "expm1", "f16round", "floor", "fround", "hypot", "imul", "log", "log10",
    "log1p", "log2", "max", "min", "pow", "random", "round", "sign", "sin", "sinh", "sqrt", "tan",
    "tanh", "trunc",
];

/// The env-slot op code for a `Math` method name, or `None` when the name is not
/// a `Math` method. 1-based — see [`MATH_FN_OPS`] for why 0 is reserved.
pub fn math_fn_op_code(name: &str) -> Option<i64> {
    MATH_FN_OPS.iter().position(|m| *m == name).map(|i| i as i64 + 1)
}

/// The `Math` method name an env-slot op code denotes, or `None` when the code is
/// out of range (including the reserved 0).
fn math_fn_op_name(op: u64) -> Option<&'static str> {
    MATH_FN_OPS.get((op as usize).checked_sub(1)?).copied()
}

/// Collect the actual argument words of a uniform-ABI call: `a0..a3` truncated to
/// the argc the `rest` slot carries (≤4-arg convention), or `a0..a3` plus the
/// overflow array's elements when `rest` is an ARRAY (>4 args).
///
/// Shared with [`super::objfn`] — the decode belongs to the uniform ABI, not to
/// `Math`; a second copy is how the two thunks would drift apart.
pub(super) fn uniform_args(a0: u64, a1: u64, a2: u64, a3: u64, rest: u64) -> Vec<u64> {
    let rv = PolyValue::from_raw(rest);
    if rv.is_object() {
        let mut out = vec![a0, a1, a2, a3];
        let h = rts_runtime::namespaces::gc::handles::__rtsn_poly_to_handle(rv.as_handle());
        let len =
            rts_runtime::namespaces::collections::vec::__RTS_FN_NS_COLLECTIONS_VEC_LEN(h).max(0);
        for i in 0..len {
            out.push(
                rts_runtime::namespaces::collections::vec::__RTS_FN_NS_COLLECTIONS_VEC_GET(h, i)
                    as u64,
            );
        }
        return out;
    }
    let argc = if rv.is_int32() {
        rv.as_i32().clamp(0, 4)
    } else {
        4
    } as usize;
    [a0, a1, a2, a3][..argc].to_vec()
}

/// JS `Math.max` of two numbers: NaN dominates, and `max(-0, +0)` is `+0`
/// (Rust's `f64::max` does neither).
fn js_max2(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        return f64::NAN;
    }
    if a == b {
        return if a.is_sign_negative() { b } else { a };
    }
    if a > b { a } else { b }
}

/// JS `Math.min` of two numbers: NaN dominates, and `min(-0, +0)` is `-0`.
fn js_min2(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        return f64::NAN;
    }
    if a == b {
        return if a.is_sign_negative() { a } else { b };
    }
    if a < b { a } else { b }
}

/// Apply the `Math` method named `name` (an entry of [`MATH_FN_OPS`]) to the
/// already-collected argument WORDS, `ToNumber`-coercing each exactly as the
/// static call path does. A missing argument reads as `undefined` ⇒ `NaN`, which
/// is the JS result for every fixed-arity member (`Math.floor()` is `NaN`); the
/// variadic members use their spec identity instead.
fn math_apply(name: &str, args: &[u64]) -> f64 {
    use rts_runtime::namespaces::math as m;
    let num = |i: usize| {
        args.get(i)
            .map(|w| super::genops::dyn_to_number(*w))
            .unwrap_or(f64::NAN)
    };
    // ToInt32 of an argument (the i32-domain members `imul`/`clz32`).
    let int = |i: usize| {
        args.get(i)
            .map(|w| super::genops_arith::to_int32(PolyValue::from_raw(*w)))
            .unwrap_or(0) as i64
    };
    match name {
        // ---- variadic ----
        "max" => args
            .iter()
            .fold(f64::NEG_INFINITY, |acc, w| {
                js_max2(acc, super::genops::dyn_to_number(*w))
            }),
        "min" => args.iter().fold(f64::INFINITY, |acc, w| {
            js_min2(acc, super::genops::dyn_to_number(*w))
        }),
        // N-ary hypot uses the SAME max-scaled compensated sum the spread call
        // path uses — a pairwise fold drifts 1 ULP from V8/JSC.
        "hypot" => {
            let xs: Vec<f64> = args
                .iter()
                .map(|w| super::genops::dyn_to_number(*w))
                .collect();
            super::globalops::hypot_n(&xs)
        }
        // ---- nullary ----
        "random" => m::__RTS_FN_NS_MATH_RANDOM_F64(),
        // ---- binary ----
        "atan2" => m::__RTS_FN_NS_MATH_ATAN2(num(0), num(1)),
        "pow" => m::__RTS_FN_NS_MATH_POW(num(0), num(1)),
        "imul" => m::__RTS_FN_NS_MATH_IMUL(int(0), int(1)) as f64,
        // ---- i32-domain unary ----
        "clz32" => m::__RTS_FN_NS_MATH_CLZ32(int(0)) as f64,
        // ---- f64 unary ----
        "abs" => m::__RTS_FN_NS_MATH_ABS_F64(num(0)),
        "acos" => m::__RTS_FN_NS_MATH_ACOS(num(0)),
        "acosh" => m::__RTS_FN_NS_MATH_ACOSH(num(0)),
        "asin" => m::__RTS_FN_NS_MATH_ASIN(num(0)),
        "asinh" => m::__RTS_FN_NS_MATH_ASINH(num(0)),
        "atan" => m::__RTS_FN_NS_MATH_ATAN(num(0)),
        "atanh" => m::__RTS_FN_NS_MATH_ATANH(num(0)),
        "cbrt" => m::__RTS_FN_NS_MATH_CBRT(num(0)),
        "ceil" => m::__RTS_FN_NS_MATH_CEIL(num(0)),
        "cos" => m::__RTS_FN_NS_MATH_COS(num(0)),
        "cosh" => m::__RTS_FN_NS_MATH_COSH(num(0)),
        "exp" => m::__RTS_FN_NS_MATH_EXP(num(0)),
        "expm1" => m::__RTS_FN_NS_MATH_EXPM1(num(0)),
        "f16round" => m::__RTS_FN_NS_MATH_F16ROUND(num(0)),
        "floor" => m::__RTS_FN_NS_MATH_FLOOR(num(0)),
        "fround" => m::__RTS_FN_NS_MATH_FROUND(num(0)),
        "log" => m::__RTS_FN_NS_MATH_LN(num(0)),
        "log10" => m::__RTS_FN_NS_MATH_LOG10(num(0)),
        "log1p" => m::__RTS_FN_NS_MATH_LOG1P(num(0)),
        "log2" => m::__RTS_FN_NS_MATH_LOG2(num(0)),
        "round" => m::__RTS_FN_NS_MATH_ROUND(num(0)),
        "sign" => m::__RTS_FN_NS_MATH_SIGN(num(0)),
        "sin" => m::__RTS_FN_NS_MATH_SIN(num(0)),
        "sinh" => m::__RTS_FN_NS_MATH_SINH(num(0)),
        "sqrt" => m::__RTS_FN_NS_MATH_SQRT(num(0)),
        "tan" => m::__RTS_FN_NS_MATH_TAN(num(0)),
        "tanh" => m::__RTS_FN_NS_MATH_TANH(num(0)),
        "trunc" => m::__RTS_FN_NS_MATH_TRUNC(num(0)),
        // `MATH_FN_OPS` is the only producer of `name`, so this is unreachable —
        // NaN is the honest answer for an unknown numeric op, never a wrong value.
        _ => f64::NAN,
    }
}

/// Uniform-ABI thunk for a `Math.<method>` FUNCTION VALUE. `env` carries the
/// index into [`MATH_FN_OPS`]; the args follow the ≤4 + overflow-array
/// convention. Every member returns a number, so the result is always an inline
/// double PolyValue.
extern "C" fn math_method_thunk(env: u64, a0: u64, a1: u64, a2: u64, a3: u64, rest: u64) -> u64 {
    let Some(name) = math_fn_op_name(env) else {
        return PolyValue::undefined().raw();
    };
    let args = uniform_args(a0, a1, a2, a3, rest);
    PolyValue::from_f64(math_apply(name, &args)).raw()
}

/// A `Math` method read as a first-class FUNCTION VALUE (`arr.map(Math.abs)`,
/// `Reflect.apply(Math.max, null, xs)`, `const f = Math.clz32` …): a real
/// callable whose env slot carries the [`MATH_FN_OPS`] index.
#[rtse::abi]
pub fn rtsadp_math_fn_value(op: u64) -> u64 {
    let name = math_fn_op_name(op).unwrap_or("");
    // `length` per the spec: the variadic members declare 2 (`max`/`min`/`hypot`),
    // `atan2`/`pow`/`imul` take 2, `random` takes 0, the rest take 1.
    let arity = match name {
        "random" => 0,
        "atan2" | "pow" | "imul" | "max" | "min" | "hypot" => 2,
        _ => 1,
    };
    let data = FunctionData {
        fn_ptr: math_method_thunk as *const () as usize as u64,
        arity,
        name: Box::<str>::from(name),
        bound_this: op as i64,
        has_bound_this: true,
        bound_args: Vec::new(),
        is_arrow: false,
        has_this_param: false,
        param_kinds: Vec::new(),
        return_kind: 0,
        packed_shim: 0,
        source: None,
        keep_alive: None,
        prototype_handle: 0,
        rest_param_idx: -1,
        uniform_thunk: true,
    };
    let h = alloc_entry(Entry::Function(Box::new(data)));
    PolyValue::from_function_handle(h & super::PAYLOAD_MASK).raw()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The two ends of the op code must agree EXACTLY. If `math_fn_op_code` and
    /// `math_fn_op_name` ever disagree by one, every method silently becomes its
    /// neighbour — `Math.sin` computing `Math.sign` and still returning a number,
    /// so nothing crashes and only a value comparison catches it. Walk the whole
    /// table and assert each name resolves back to itself.
    #[test]
    fn op_code_round_trips_for_every_member() {
        for &name in MATH_FN_OPS {
            let code = math_fn_op_code(name).expect("every table entry has a code");
            assert_eq!(
                math_fn_op_name(code as u64),
                Some(name),
                "op code {code} did not round-trip back to `{name}`"
            );
        }
    }

    /// 0 is RESERVED: `__rtsadp_fn_invoke` uses `env == 0` as the "captures
    /// nothing" sentinel and rewrites it to the `undefined` word, so a 0 code
    /// never reaches the thunk intact. Codes must therefore start at 1.
    #[test]
    fn zero_is_reserved_and_codes_start_at_one() {
        assert_eq!(math_fn_op_name(0), None, "0 must decode to nothing");
        assert_eq!(math_fn_op_code(MATH_FN_OPS[0]), Some(1), "first entry is 1");
        assert_eq!(math_fn_op_name(1), Some(MATH_FN_OPS[0]));
    }

    /// A name that is not a `Math` method has no code (the engine's member-read
    /// path relies on `None` to fall through to its bail).
    #[test]
    fn unknown_name_has_no_code() {
        assert_eq!(math_fn_op_code("nopeNotAMember"), None);
        assert_eq!(math_fn_op_name(MATH_FN_OPS.len() as u64 + 1), None);
    }

    /// The table must have no duplicate entry — a duplicate makes
    /// `position()` return the FIRST index, so the second copy is unreachable
    /// and its op code decodes to the wrong method.
    #[test]
    fn table_has_no_duplicates() {
        let mut seen: Vec<&str> = MATH_FN_OPS.to_vec();
        seen.sort_unstable();
        let before = seen.len();
        seen.dedup();
        assert_eq!(before, seen.len(), "MATH_FN_OPS contains a duplicate");
    }
}
