//! `Number`'s 8 numeric constants (`MAX_SAFE_INTEGER`, `EPSILON`, …) plus the
//! `parseInt`/`parseFloat` aliases onto the `fmt` namespace's real parser
//! externs. Split out of `mod.rs` (the value-class ctor/methods/predicates) for
//! the file-size ceiling. See `mod.rs`'s module doc for how this composes onto
//! the SAME "Number" Registry class entry as the macro-generated members.
//!
//! The 8 constants are `#[rtse::constant(global = "Number", value = "…")]` —
//! each emits its own `<name>_member()` fn, composed through `.member(...)`
//! below (a `#[rtse::class]` `impl` block cannot carry a `const`, so these
//! cannot join the ctor/methods/statics in `mod.rs`'s single impl block).
//!
//! `parseInt`/`parseFloat` stay hand-written `Member` literals: they have a
//! NULL `fn_ptr` and point at ANOTHER namespace's symbol
//! (`__RTS_FN_NS_FMT_PARSE_I64`/`_F64`) rather than a body of their own, so
//! neither `#[rtse::statical]` (which generates a body + symbol here) nor
//! `#[rtse::constant]` (a zero-arg getter) fits — there is nothing to generate.
//! The live call path is `front/run/mathobj.rs`'s `"Number"` arm, which aliases
//! the global `parseInt`/`parseFloat` directly; these two `Member`s exist for
//! reflection/`.d.ts` only.

use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

/// `Number.MAX_SAFE_INTEGER` — 2^53 - 1.
#[rtse::constant(global = "Number", value = "MAX_SAFE_INTEGER")]
pub const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;

/// `Number.MIN_SAFE_INTEGER` — -(2^53 - 1).
#[rtse::constant(global = "Number", value = "MIN_SAFE_INTEGER")]
pub const MIN_SAFE_INTEGER: f64 = -9_007_199_254_740_991.0;

/// `Number.MAX_VALUE` — largest finite f64.
#[rtse::constant(global = "Number", value = "MAX_VALUE")]
pub const MAX_VALUE: f64 = f64::MAX;

/// `Number.MIN_VALUE` — smallest positive f64 (subnormal).
#[rtse::constant(global = "Number", value = "MIN_VALUE")]
pub const MIN_VALUE: f64 = 5e-324;

/// `Number.EPSILON` — 2^-52.
#[rtse::constant(global = "Number", value = "EPSILON")]
pub const EPSILON: f64 = f64::EPSILON;

/// `Number.POSITIVE_INFINITY`.
#[rtse::constant(global = "Number", value = "POSITIVE_INFINITY")]
pub const POSITIVE_INFINITY: f64 = f64::INFINITY;

/// `Number.NEGATIVE_INFINITY`.
#[rtse::constant(global = "Number", value = "NEGATIVE_INFINITY")]
pub const NEGATIVE_INFINITY: f64 = f64::NEG_INFINITY;

/// `Number.NaN`.
#[rtse::constant(global = "Number", value = "NaN")]
pub const NAN: f64 = f64::NAN;

/// Registry-member builder helper for the two hand-written aliases below (see
/// this module's doc for why `parseInt`/`parseFloat` cannot be macro-generated).
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

/// Register `Number`'s 8 constants + the `parseInt`/`parseFloat` aliases.
/// Composed onto the SAME "Number" Registry class entry as
/// `NumberWrapper::register`'s macro-generated ctor/methods/statics via
/// `Registry::insert_class`'s merge-on-second-call. Call this AND
/// `super::register` — order does not matter (see
/// `registry_build.rs::REGISTER`'s doc).
pub fn register_number_statics(e: &mut Engine) {
    e.class("Number")
        .member(max_safe_integer_member())
        .member(min_safe_integer_member())
        .member(max_value_member())
        .member(min_value_member())
        .member(epsilon_member())
        .member(positive_infinity_member())
        .member(negative_infinity_member())
        .member(nan_member())
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
        .done();
}

/// Value f64 of a `Number.*` constant by name. Used by the codegen fast path
/// (`front/run/mathobj.rs::number_const` carries its OWN copy for its inline
/// `f64const` emission — pre-existing duplication, out of this migration's
/// scope) and available for any other consumer needing the constant by name.
pub fn number_const_value(name: &str) -> Option<f64> {
    match name {
        "MAX_SAFE_INTEGER" => Some(MAX_SAFE_INTEGER),
        "MIN_SAFE_INTEGER" => Some(MIN_SAFE_INTEGER),
        "MAX_VALUE" => Some(MAX_VALUE),
        "MIN_VALUE" => Some(MIN_VALUE),
        "EPSILON" => Some(EPSILON),
        "POSITIVE_INFINITY" => Some(POSITIVE_INFINITY),
        "NEGATIVE_INFINITY" => Some(NEGATIVE_INFINITY),
        "NaN" => Some(NAN),
        _ => None,
    }
}
