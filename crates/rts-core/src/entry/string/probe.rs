//! `charCodeAt`, cut into prefixes, so a measurement can say which step costs.
//!
//! # Why prefixes of one body rather than five separate experiments
//!
//! Because the question is where ~330 ns inside one call go, and a separate
//! experiment answers about a different call. Each native here is
//! [`super::basic`]'s `charCodeAt` truncated one step earlier, installed on the
//! same prototype, reached by the same chain walk and called through the same
//! convention — so the difference between two adjacent rows is exactly the step
//! between them and nothing else.
//!
//! `__step0` in particular does no work at all: it is what a call to a string
//! method costs before the method does anything, which is the number no amount
//! of reading the body can produce.
//!
//! **Instrumentation, not a feature.** These names are on
//! `String.prototype` for as long as this branch exists and nowhere else.

use super::super::native::Native;
use super::super::with_current;
use super::nothing;
use crate::value::Value;

/// How many times the uncached read and the call ran since the last reset.
///
/// Counted rather than subtracted: two timings differing by 200 ns say the two
/// cases differ, not that one of them ran the lookup twice — and "ran it twice"
/// is exactly the hypothesis a difference of timings cannot tell from "ran it
/// once, slower".
pub static GETS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub static CALLS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub static RESOLVES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub static RESOLVES_INDIRECT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
/// The prefixes, longest name last so a bench file reads in order.
pub(super) const NATIVES: &[(&str, Native)] = &[
    ("__counts", counts),
    ("__step0", step0),
    ("__step1", step1),
    ("__step2", step2),
    ("__step3", step3),
    ("__step4", step4),
];

/// `s.__counts(0)` answers the get count and resets both; `s.__counts(1)`
/// answers the call count and resets both. One native rather than two, because
/// a second one would itself be a call and would perturb the counter it reads.
extern "C" fn counts(_e: u64, _this: u64, which: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    use std::sync::atomic::Ordering::Relaxed;
    let counts = [
        GETS.swap(0, Relaxed),
        CALLS.swap(0, Relaxed),
        RESOLVES.swap(0, Relaxed),
        RESOLVES_INDIRECT.swap(0, Relaxed),
    ];
    let asked = Value(which).numeric().unwrap_or(0.0) as usize;
    Value::from_f64(counts.get(asked).copied().unwrap_or(0) as f64).bits()
}

/// Nothing at all: the call convention, and the return.
extern "C" fn step0(_e: u64, _this: u64, _index: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    Value::from_f64(0.0).bits()
}

/// Plus entering the context borrow.
extern "C" fn step1(_e: u64, _this: u64, _index: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    with_current(|_context| Value::from_f64(0.0).bits())
}

/// Plus `length_of` — one heap lookup of the receiver's text.
extern "C" fn step2(_e: u64, this: u64, _index: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    with_current(|context| {
        let Some(length) = super::length_of(context, this) else {
            return nothing(context);
        };
        Value::from_f64(length as f64).bits()
    })
}

/// Plus decoding the argument.
extern "C" fn step3(_e: u64, this: u64, index: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    with_current(|context| {
        let Some(length) = super::length_of(context, this) else {
            return nothing(context);
        };
        let at = Value(index).numeric().unwrap_or(0.0);
        if at < 0.0 || at >= length as f64 {
            return Value::from_f64(f64::NAN).bits();
        }
        Value::from_f64(at).bits()
    })
}

/// Plus the second heap lookup, `indexed` — the whole of `charCodeAt`.
extern "C" fn step4(_e: u64, this: u64, index: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    with_current(|context| {
        let Some(length) = super::length_of(context, this) else {
            return nothing(context);
        };
        let at = Value(index).numeric().unwrap_or(0.0);
        if at < 0.0 || at >= length as f64 {
            return Value::from_f64(f64::NAN).bits();
        }
        match super::indexed(context, this, at as usize) {
            Some(unit) => Value::from_f64(f64::from(unit)).bits(),
            None => Value::from_f64(f64::NAN).bits(),
        }
    })
}
