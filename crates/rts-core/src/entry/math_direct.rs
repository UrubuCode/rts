//! `Math`'s library members, reached by compiled code without the object.
//!
//! # What this is, and what it is not
//!
//! A sine is a library call on every target this engine has — no hardware does
//! it in an instruction — so `Math.sin(x)` can never be what `Math.sqrt(x)` is.
//! What it can stop being is a PROPERTY READ plus a DISPATCH: the chain cache,
//! the callable resolution, two borrows of the context, three stacks pushed
//! and popped, an argument coerced through a wrapper. `docs/codegen/
//! native-call-floor.md` measured that toll at about 35 ns, against a body
//! that costs 5. Where the whole program proves `Math` is untouched and the
//! operand is already a double, the compiler calls HERE instead, with the
//! operand unboxed and the answer unboxed — the same door `Math.random` took
//! first, for the same reason.
//!
//! # Why a selector and one entry, rather than an entry per member
//!
//! Twenty-three entry points is twenty-three numbered rows in `table.rs`,
//! twenty-three arms in the host's wiring and twenty-three `RuntimeOp`s. Two
//! rows and a table of function pointers say the same thing, and the indirect
//! call through the table is a nanosecond against the thirty this removes.
//!
//! The table is indexed by a number the COMPILER chose, which makes it an
//! agreement between two crates that never see each other's source — the same
//! kind as `ARGUMENT_SLOTS`. Both sides state it as a list of names in one
//! order, and `rts-host` asserts the two lists are equal. A name is what the
//! compiler has when it decides, so the list is of names and not of enums.
//!
//! # What each member means is decided once
//!
//! Every function below is the one `Math`'s own member calls: `atanh_odd`,
//! `hypot_of` and `exponentiate` are shared with `math.rs` rather than
//! rewritten, because the member and this door must answer the same bits, and
//! `atanh` and `hypot` each carry a correction the obvious spelling lacks.

use super::bitwise::exponentiate;
use super::math::{atanh_odd, f16_bits_to_f64, f64_to_f16_bits, hypot_of};

/// The one-operand members, by the number the compiler passes.
///
/// Order is the agreement: `rts_codegen::runtime::math_direct::UNARY_NAMES`
/// lists the same names in the same order, and the host refuses to build when
/// they differ.
pub const UNARY_NAMES: [&str; 20] = [
    "sin", "cos", "tan", "asin", "acos", "atan", "sinh", "cosh", "tanh", "asinh", "acosh", "atanh",
    "exp", "expm1", "log", "log1p", "log2", "log10", "cbrt", "f16round",
];

/// What each name in [`UNARY_NAMES`] computes, in the same order.
static UNARY: [fn(f64) -> f64; 20] = [
    f64::sin,
    f64::cos,
    f64::tan,
    f64::asin,
    f64::acos,
    f64::atan,
    f64::sinh,
    f64::cosh,
    f64::tanh,
    f64::asinh,
    f64::acosh,
    atanh_odd,
    f64::exp,
    f64::exp_m1,
    f64::ln,
    f64::ln_1p,
    f64::log2,
    f64::log10,
    f64::cbrt,
    |x| f16_bits_to_f64(f64_to_f16_bits(x)),
];

/// The two-operand members, by number. Same agreement as [`UNARY_NAMES`].
pub const BINARY_NAMES: [&str; 3] = ["atan2", "pow", "hypot"];

/// What each name in [`BINARY_NAMES`] computes, in the same order.
static BINARY: [fn(f64, f64) -> f64; 3] = [f64::atan2, exponentiate, |a, b| hypot_of(&[a, b])];

/// `Math.<UNARY_NAMES[which]>(x)`, over a double that is already one.
///
/// A number out of range answers `NaN`: it cannot happen from the compiler
/// this ships with, and a wrong answer is the loudest thing a pure function
/// can do without raising.
#[rtse::entry]
pub fn math_direct1(which: i64, x: f64) -> f64 {
    match usize::try_from(which).ok().and_then(|at| UNARY.get(at)) {
        Some(f) => f(x),
        None => f64::NAN,
    }
}

/// `Math.<BINARY_NAMES[which]>(a, b)`, over two doubles that are already ones.
#[rtse::entry]
pub fn math_direct2(which: i64, a: f64, b: f64) -> f64 {
    match usize::try_from(which).ok().and_then(|at| BINARY.get(at)) {
        Some(f) => f(a, b),
        None => f64::NAN,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_answers_what_the_member_answers() {
        assert_eq!(math_direct1(0, 0.0), 0.0);
        let atanh = UNARY_NAMES.iter().position(|n| *n == "atanh").unwrap() as i64;
        assert!(math_direct1(atanh, -0.0).is_sign_negative());
        assert_eq!(math_direct1(atanh, -0.5), -math_direct1(atanh, 0.5));
        let hypot = BINARY_NAMES.iter().position(|n| *n == "hypot").unwrap() as i64;
        assert_eq!(math_direct2(hypot, 3.0, 4.0), 5.0);
        assert!(math_direct2(hypot, 1e200, 1e200).is_finite());
        assert!(math_direct1(99, 1.0).is_nan());
        assert_eq!(UNARY_NAMES.len(), UNARY.len());
        assert_eq!(BINARY_NAMES.len(), BINARY.len());
    }
}
