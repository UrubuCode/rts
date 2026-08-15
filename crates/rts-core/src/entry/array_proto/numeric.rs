//! The two coercions an array method performs on a number argument.
//!
//! # Why they are a module and not `Value::numeric` at each site
//!
//! `Value::numeric` answers what a word already IS — a double, or nothing. Every
//! index argument in this folder went through it and then `unwrap_or(0.0)`, which
//! is three divergences at once and each of them is a wrong program that runs:
//! `a.at("2")` read element 0 rather than element 2, `a.at(-2.7)` read the third
//! element from the end rather than the second, and `a.flat("2")` flattened one
//! level rather than two. The specification puts `ToIntegerOrInfinity` in front of
//! every one of them, and it is a conversion rather than a read.
//!
//! # Why every caller must be outside a borrow
//!
//! [`integer_or_infinity`] reaches `ToPrimitive`, which calls a `valueOf` the
//! program wrote. That is user code, and calling it from inside `with_current`
//! re-enters the `RefCell` — a hang, not a wrong answer, which is the trap this
//! whole folder is shaped around. So a method converts FIRST and takes its borrow
//! afterwards, which is why `at` and `flat` are now two statements.
//!
//! The alternative rejected: a context-taking form that reads only what is
//! already a number. That is what was there, spelled differently, and it answers
//! the wrong number for exactly the arguments a program passes by accident.

/// `ToIntegerOrInfinity(value)` — what an index argument goes through.
///
/// `NaN` is zero and everything else truncates TOWARD zero, which is the half a
/// `floor` would get wrong: `-2.7` becomes `-2`, so `a.at(-2.7)` is the second
/// element from the end and not the third.
///
/// Infinity survives as infinity rather than saturating to a `usize`, because the
/// callers compare it against a length — and `a.at(Infinity)` has to be out of
/// range rather than the last element.
pub(super) fn integer_or_infinity(value: u64) -> f64 {
    let number = super::super::class_support::to_number(value);
    if number.is_nan() {
        return 0.0;
    }
    number.trunc()
}

/// `ToLength(value)` — what a `length` an object merely CLAIMS goes through.
///
/// The clamp is the specification's: below zero is zero, so `{length: -5}` is
/// empty rather than enormous, and above 2^53-1 stops there. Both matter for the
/// same reason — this number decides how many times a loop reads a property, so a
/// negative one that wrapped and an infinite one that did not stop are the two
/// ways `Array.from` becomes a hang.
pub(super) fn length(value: u64) -> usize {
    integer_or_infinity(value).clamp(0.0, 9_007_199_254_740_991.0) as usize
}
