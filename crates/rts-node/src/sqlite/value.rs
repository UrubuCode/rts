//! JS value <-> SQLite storage class, in both directions.
//!
//! # The four storage classes this crate can round-trip, and the one it cannot
//!
//! SQLite has five storage classes. `NULL`, `REAL`, `TEXT` and `BLOB` cross
//! cleanly: `NULL` is the engine's `null` singleton, `REAL` and `TEXT` are a
//! `number`/`string` with no representational gap, `BLOB` is a `Buffer` built
//! by [`rts_core::entry::make_buffer`] over a copy of the bytes (the same
//! copy-not-borrow rule every buffer-crossing helper in `entry::modules`
//! documents).
//!
//! `INTEGER` is the one that does not: `turso_core::Numeric::Integer` is a
//! real `i64`, so the Rust side loses nothing, but there is no `make_bigint`
//! in `rts_core::entry::modules` — the only value API this crate is
//! allowed to reach — and [`rts_core::entry::bigint_new`] is not part of
//! that surface, parses TEXT rather than taking an integer, and calls
//! `with_current` itself, so invoking it from inside [`with_runtime`] would be
//! the nested borrow `docs/reference/node/STATUS.md` names as the abort every
//! module here pays for. So every `INTEGER` this module reads back — safe
//! range or not — becomes a JS `number` through [`rts_core::entry::make_number`].
//! `setReadBigInts`/`setReturnArrays` are accepted (see [`super::statement`])
//! but do nothing: a `readBigInts`-enabled statement in Node throws
//! `ERR_OUT_OF_RANGE` for a value outside `Number.MIN_SAFE_INTEGER..=
//! Number.MAX_SAFE_INTEGER` instead of rounding it, and this module cannot
//! throw either — `rts_core::entry::throw` ends the PROGRAM (see its own
//! doc), it is not a catchable JS exception, and no host module in this crate
//! uses it for that reason. So a `rowid`, an `AUTOINCREMENT` counter or a
//! hash stored as an `INTEGER` past 2^53 is a `number` that has silently
//! rounded — named here, in the one place every reader of an `INTEGER` goes
//! through, rather than guessed at per call site.
//!
//! [`with_runtime`]: rts_core::entry::with_runtime

use rts_core::entry::{self, Context};

/// A SQLite value, read back as a JS value.
///
/// `context` is already in hand — this is the context-taking half every
/// caller inside a `with_runtime` body needs, per [`super`]'s own rule.
pub(super) fn to_js(context: &mut Context, value: &turso_core::Value) -> u64 {
    match value {
        turso_core::Value::Null => entry::null_in(context),
        // A whole number a double holds exactly stays a number, which is what a
        // program expects for an ordinary id or count. One that a double would
        // ROUND becomes a bigint instead — silently rounding it was the wrong
        // answer this module shipped with, and `make_bigint` is what removed it.
        turso_core::Value::Numeric(turso_core::Numeric::Integer(i)) => match i.unsigned_abs() {
            0..=9_007_199_254_740_991 => entry::make_number(*i as f64),
            _ => entry::make_bigint(context, *i),
        },
        turso_core::Value::Numeric(turso_core::Numeric::Float(f)) => entry::make_number(f64::from(*f)),
        turso_core::Value::Text(text) => entry::make_string(context, text.as_str()),
        turso_core::Value::Blob(bytes) => entry::make_buffer(context, bytes),
    }
}

/// A JS value, read as a SQLite value to bind.
///
/// # What each JS type becomes
///
/// - `null`/`undefined` -> `NULL`.
/// - a `number` -> `INTEGER` when it has no fractional part and fits an
///   `i64` (`turso_core::Numeric::Integer` is exactly that width, matching
///   SQLite's own 64-bit `INTEGER`), `REAL` otherwise.
/// - a `Buffer`/typed array/`DataView` (anything [`entry::bytes_of`] answers)
///   -> `BLOB`, over a copy of the bytes.
/// - anything else [`entry::text_in`] can stringify without running user
///   code — a `string`, and, indistinguishably, a `boolean` or a `bigint` —
///   becomes `TEXT`. Node binds a `bigint` as `INTEGER`; this crate has no
///   `is_bigint` predicate in `entry::modules` to tell one from a `string`
///   that merely looks like an integer, and guessing which one a caller meant
///   is exactly the "plausible wrong value" this repository's Node modules
///   refuse elsewhere — so a `bigint` parameter binds as `TEXT` of its
///   decimal digits instead, named as a divergence rather than silently
///   producing the SQL a caller did not write. Node itself would reject a
///   `boolean` parameter outright; this module accepts one, as text, for the
///   same reason: there is no throw to reject it with (see [`to_js`]'s doc).
/// - an object (anything neither of the above reaches, `ToString` of which
///   is user code an entry point cannot call) -> `NULL`.
pub(super) fn from_js(context: &Context, value: u64) -> turso_core::Value {
    let undefined = entry::undefined_in(context);
    let null = entry::null_in(context);
    if value == undefined || value == null {
        return turso_core::Value::Null;
    }
    if let Some(number) = entry::number_of(value) {
        return numeric_of(number);
    }
    if let Some(bytes) = entry::bytes_of(context, value) {
        return turso_core::Value::Blob(bytes);
    }
    if let Some(text) = entry::text_in(context, value) {
        return turso_core::Value::Text(turso_core::types::Text::new(text));
    }
    turso_core::Value::Null
}

/// A JS `number` as `INTEGER` (whole, in `i64` range) or `REAL`.
fn numeric_of(number: f64) -> turso_core::Value {
    // `i64::MAX as f64` rounds UP to 2^63 (f64 cannot represent `i64::MAX`
    // exactly), so a half-open bound against 2^63 is used instead of
    // `..=i64::MAX as f64` — the closed range would accept 2^63 itself, which
    // overflows an `i64` and would have been truncated by `as i64` rather
    // than refused.
    const MIN: f64 = -9_223_372_036_854_775_808.0; // -2^63, exact in f64
    const BOUND: f64 = 9_223_372_036_854_775_808.0; // 2^63, exact in f64
    if number.fract() == 0.0 && number.is_finite() && number >= MIN && number < BOUND {
        return turso_core::Value::Numeric(turso_core::Numeric::Integer(number as i64));
    }
    match turso_core::NonNan::new(number) {
        Some(nn) => turso_core::Value::Numeric(turso_core::Numeric::Float(nn)),
        // `NaN` has no SQLite storage class; SQLite itself stores a bound NaN
        // as NULL (its own C API does the same coercion), so this matches
        // rather than diverging.
        None => turso_core::Value::Null,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_whole_number_in_range_becomes_an_integer() {
        assert!(matches!(
            numeric_of(42.0),
            turso_core::Value::Numeric(turso_core::Numeric::Integer(42))
        ));
        assert!(matches!(
            numeric_of(-1.0),
            turso_core::Value::Numeric(turso_core::Numeric::Integer(-1))
        ));
    }

    #[test]
    fn a_fractional_number_becomes_real() {
        assert!(matches!(
            numeric_of(3.5),
            turso_core::Value::Numeric(turso_core::Numeric::Float(_))
        ));
    }

    #[test]
    fn a_whole_number_past_i64_stays_real_rather_than_wrapping() {
        // 2^63 does not fit i64 — binding it as REAL is honest; wrapping it
        // into i64::MIN via `as` would silently corrupt the value instead.
        let huge = 9_223_372_036_854_775_808.0_f64; // 2^63
        assert!(matches!(
            numeric_of(huge),
            turso_core::Value::Numeric(turso_core::Numeric::Float(_))
        ));
    }

    #[test]
    fn nan_becomes_null_matching_sqlites_own_bind_coercion() {
        assert!(matches!(numeric_of(f64::NAN), turso_core::Value::Null));
    }
}
