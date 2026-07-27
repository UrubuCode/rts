//! `Number` statics + constants — `isNaN`/`isFinite`/`isInteger`/`isSafeInteger`/
//! `parseInt`/`parseFloat` and the 8 numeric constants. Split out of `mod.rs`
//! (the value-class ctor/methods) for the file-size ceiling AND because these
//! are hand-written `Member`s (a `#[rtse::class]` `impl` block can only carry
//! `Fn` items, so a Rust `const` — the constants especially — cannot live
//! inside it). See `mod.rs`'s module doc for how this composes onto the SAME
//! "Number" Registry class entry as the macro-generated ctor/methods.

use rts_engine::abi::ty::{Bool, F64};
use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

/// `Number.isNaN(value)`. LOAD-BEARING: `front/run/mathobj.rs`'s
/// `number_predicate` calls this symbol directly for a proven-number arg.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_NUMBER_IS_NAN(v: F64) -> Bool {
    v.is_nan() as i64
}

/// `Number.isFinite(value)`. LOAD-BEARING (see [`__RTS_FN_GL_NUMBER_IS_NAN`]).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_NUMBER_IS_FINITE(v: F64) -> Bool {
    v.is_finite() as i64
}

/// `Number.isInteger(value)`. LOAD-BEARING (see [`__RTS_FN_GL_NUMBER_IS_NAN`]).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_NUMBER_IS_INTEGER(v: F64) -> Bool {
    (v.is_finite() && v.fract() == 0.0) as i64
}

/// `Number.isSafeInteger(value)`. LOAD-BEARING (see [`__RTS_FN_GL_NUMBER_IS_NAN`]).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_NUMBER_IS_SAFE_INT(v: F64) -> Bool {
    const MAX: f64 = 9_007_199_254_740_991.0;
    (v.is_finite() && v.fract() == 0.0 && v.abs() <= MAX) as i64
}

/// Registry-member builder helper (mirrors the macro's own `.member(...)` shape
/// for the members a `#[rtse::class]` `impl` block cannot express).
#[allow(clippy::too_many_arguments)]
fn m(name: &str, kind: MemberKind, sig: Sig, symbol: &str, ts: &str, doc: &str, fp: *const u8) -> Member {
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
        pure: true,
        ret_class: None,
        emit: None,
    }
}

/// Register `Number`'s statics + constants — the isNaN/isFinite/isInteger/
/// isSafeInteger/parseInt/parseFloat predicates and the 8 numeric constants.
/// Composed onto the SAME "Number" Registry class entry as
/// `NumberWrapper::register`'s ctor/methods via `Registry::insert_class`'s
/// merge-on-second-call. Call this AND `super::register` — order does not
/// matter (see `registry_build.rs::REGISTER`'s doc).
pub fn register_number_statics(e: &mut Engine) {
    e.class("Number")
        .member(m(
            "isNaN",
            MemberKind::StaticMethod,
            Sig::new(vec![AbiType::F64], AbiType::Bool),
            "__RTS_FN_GL_NUMBER_IS_NAN",
            "isNaN(value: number): boolean",
            "Number.isNaN(value) — no coercion (front/run/mathobj.rs owns the live call path).",
            __RTS_FN_GL_NUMBER_IS_NAN as *const u8,
        ))
        .member(m(
            "isFinite",
            MemberKind::StaticMethod,
            Sig::new(vec![AbiType::F64], AbiType::Bool),
            "__RTS_FN_GL_NUMBER_IS_FINITE",
            "isFinite(value: number): boolean",
            "Number.isFinite(value) — no coercion (front/run/mathobj.rs owns the live call path).",
            __RTS_FN_GL_NUMBER_IS_FINITE as *const u8,
        ))
        .member(m(
            "isInteger",
            MemberKind::StaticMethod,
            Sig::new(vec![AbiType::F64], AbiType::Bool),
            "__RTS_FN_GL_NUMBER_IS_INTEGER",
            "isInteger(value: number): boolean",
            "Number.isInteger(value) — no coercion (front/run/mathobj.rs owns the live call path).",
            __RTS_FN_GL_NUMBER_IS_INTEGER as *const u8,
        ))
        .member(m(
            "isSafeInteger",
            MemberKind::StaticMethod,
            Sig::new(vec![AbiType::F64], AbiType::Bool),
            "__RTS_FN_GL_NUMBER_IS_SAFE_INT",
            "isSafeInteger(value: number): boolean",
            "Number.isSafeInteger(value) — no coercion (front/run/mathobj.rs owns the live call path).",
            __RTS_FN_GL_NUMBER_IS_SAFE_INT as *const u8,
        ))
        .member(m(
            "parseInt",
            MemberKind::StaticMethod,
            Sig::new(vec![AbiType::StrPtr], AbiType::I64),
            "__RTS_FN_NS_FMT_PARSE_I64",
            "parseInt(s: string): number",
            "Number.parseInt(s) — delegates to the fmt namespace extern (front/run/mathobj.rs aliases the global parseInt for the live call path).",
            core::ptr::null::<u8>(),
        ))
        .member(m(
            "parseFloat",
            MemberKind::StaticMethod,
            Sig::new(vec![AbiType::StrPtr], AbiType::F64),
            "__RTS_FN_NS_FMT_PARSE_F64",
            "parseFloat(s: string): number",
            "Number.parseFloat(s) — delegates to the fmt namespace extern (front/run/mathobj.rs aliases the global parseFloat for the live call path).",
            core::ptr::null::<u8>(),
        ))
        .member(m(
            "MAX_SAFE_INTEGER",
            MemberKind::Constant,
            Sig::new(Vec::new(), AbiType::F64),
            "__RTS_DATA_GL_NUMBER_MAX_SAFE_INTEGER",
            "MAX_SAFE_INTEGER: number",
            "Number.MAX_SAFE_INTEGER — 2^53 - 1.",
            core::ptr::null::<u8>(),
        ))
        .member(m(
            "MIN_SAFE_INTEGER",
            MemberKind::Constant,
            Sig::new(Vec::new(), AbiType::F64),
            "__RTS_DATA_GL_NUMBER_MIN_SAFE_INTEGER",
            "MIN_SAFE_INTEGER: number",
            "Number.MIN_SAFE_INTEGER — -(2^53 - 1).",
            core::ptr::null::<u8>(),
        ))
        .member(m(
            "MAX_VALUE",
            MemberKind::Constant,
            Sig::new(Vec::new(), AbiType::F64),
            "__RTS_DATA_GL_NUMBER_MAX_VALUE",
            "MAX_VALUE: number",
            "Number.MAX_VALUE — largest finite f64.",
            core::ptr::null::<u8>(),
        ))
        .member(m(
            "MIN_VALUE",
            MemberKind::Constant,
            Sig::new(Vec::new(), AbiType::F64),
            "__RTS_DATA_GL_NUMBER_MIN_VALUE",
            "MIN_VALUE: number",
            "Number.MIN_VALUE — smallest positive f64 (subnormal).",
            core::ptr::null::<u8>(),
        ))
        .member(m(
            "EPSILON",
            MemberKind::Constant,
            Sig::new(Vec::new(), AbiType::F64),
            "__RTS_DATA_GL_NUMBER_EPSILON",
            "EPSILON: number",
            "Number.EPSILON — 2^-52.",
            core::ptr::null::<u8>(),
        ))
        .member(m(
            "POSITIVE_INFINITY",
            MemberKind::Constant,
            Sig::new(Vec::new(), AbiType::F64),
            "__RTS_DATA_GL_NUMBER_POS_INF",
            "POSITIVE_INFINITY: number",
            "Number.POSITIVE_INFINITY.",
            core::ptr::null::<u8>(),
        ))
        .member(m(
            "NEGATIVE_INFINITY",
            MemberKind::Constant,
            Sig::new(Vec::new(), AbiType::F64),
            "__RTS_DATA_GL_NUMBER_NEG_INF",
            "NEGATIVE_INFINITY: number",
            "Number.NEGATIVE_INFINITY.",
            core::ptr::null::<u8>(),
        ))
        .member(m(
            "NaN",
            MemberKind::Constant,
            Sig::new(Vec::new(), AbiType::F64),
            "__RTS_DATA_GL_NUMBER_NAN",
            "NaN: number",
            "Number.NaN.",
            core::ptr::null::<u8>(),
        ))
        .done();
}

/// Value f64 of a `Number.*` constant by name. Used by the codegen fast path
/// (`front/run/mathobj.rs::number_const` carries its OWN copy for its inline
/// `f64const` emission — pre-existing duplication, out of this migration's
/// scope) and available for any other consumer needing the constant by name.
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
