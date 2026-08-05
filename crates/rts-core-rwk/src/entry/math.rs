//! `Math` — a namespace of numeric functions, and the constants beside them.
//!
//! # Why this is an object and not a compile-time lowering
//!
//! Recorded in full at `docs/engine/authoring-natives.md` §8; the short of it:
//! The old engine folds `Math.floor(x)` into an instruction, which is a real win
//! and is also the language layer knowing about a built-in — the thing
//! `emit/globals.rs` deliberately avoids.
//!
//! It is an object here, and the deciding argument is neither of those. **A
//! lowering is not observably equivalent**: `Math.floor` is a writable property
//! of a mutable object, so a program may replace it, pass it, or read it, and
//! code folded at compile time answers the original for all three. An
//! optimisation that is wrong for legal programs has to be *guarded*, and the
//! guard — proving nothing wrote to `Math` — is a whole-program fact this
//! compiler cannot establish today.
//!
//! What the measurement actually said is also narrower than "lower it". The
//! 124× was moving a body out of `.ts` into a native symbol, and that is exactly
//! what this is. Folding on top of it is a further, smaller win available later
//! behind a guard, and nothing here forecloses it: the emitter would recognise a
//! call whose callee it can prove is this object's, which is a decision at the
//! call site rather than a different runtime.
//!
//! # Why a namespace rather than a class with everything static
//!
//! `Math` has no `[[Construct]]` and no `prototype`. `new Math()` is a
//! `TypeError`, and a class flavour would have produced a constructor answering
//! something — a question the language refuses, answered anyway.
//!
//! # What the members do not do
//!
//! Coerce twice. Each parameter is `f64`, so the generated wrapper runs
//! `ToNumber` once on the way in, which is what the specification says and what
//! a body reading a `u64` and converting itself would have got subtly different
//! per member.

use std::f64::consts;

use super::objects::undefined_of;
use super::with_current;

/// The two-argument fold `max` and `min` are, over arguments that may be absent.
///
/// The identity is what the language folds from — `-Infinity` for `max`,
/// `Infinity` for `min` — so a call with no arguments answers it, and one with
/// a single argument answers that argument rather than a comparison against
/// zero.
fn folded(a: u64, b: u64, identity: f64, better: fn(f64, f64) -> f64) -> f64 {
    // The borrow is taken and given back before the first coercion, because
    // `to_number` takes one of its own. Nesting them is a panic on the re-entry,
    // which is the trap this whole authoring layer exists to make structural.
    let absent = with_current(|context| undefined_of(context));
    let mut answer = identity;
    for value in [a, b] {
        if value == absent {
            continue;
        }
        let number = super::class_support::to_number(value);
        if number.is_nan() {
            return f64::NAN;
        }
        answer = better(answer, number);
    }
    answer
}

/// `Math`.
#[rtse::class("Math", namespace)]
impl Math {
    /// The base of the natural logarithm.
    const E: f64 = consts::E;
    /// The natural logarithm of 10.
    const LN10: f64 = consts::LN_10;
    /// The natural logarithm of 2.
    const LN2: f64 = consts::LN_2;
    /// The base-10 logarithm of e.
    const LOG10E: f64 = consts::LOG10_E;
    /// The base-2 logarithm of e.
    const LOG2E: f64 = consts::LOG2_E;
    /// π.
    const PI: f64 = consts::PI;
    /// The square root of ½.
    const SQRT1_2: f64 = consts::FRAC_1_SQRT_2;
    /// The square root of 2.
    const SQRT2: f64 = consts::SQRT_2;

    /// `Math.abs(x)`.
    fn abs(x: f64) -> f64 {
        x.abs()
    }

    /// `Math.floor(x)`.
    fn floor(x: f64) -> f64 {
        x.floor()
    }

    /// `Math.ceil(x)`.
    fn ceil(x: f64) -> f64 {
        x.ceil()
    }

    /// `Math.round(x)`.
    ///
    /// **Not** `f64::round`, which rounds a half away from zero. JavaScript
    /// rounds a half *up*, so `Math.round(-0.5)` is `-0` and not `-1` — the one
    /// place this function differs from the obvious call, and the one that
    /// passes every test written with positive numbers.
    fn round(x: f64) -> f64 {
        if x.is_nan() || x.is_infinite() {
            return x;
        }
        (x + 0.5).floor()
    }

    /// `Math.trunc(x)`.
    fn trunc(x: f64) -> f64 {
        x.trunc()
    }

    /// `Math.sign(x)`.
    ///
    /// Not `f64::signum`, which answers `1` for `+0` and `-1` for `-0` where the
    /// language answers the zero itself.
    fn sign(x: f64) -> f64 {
        if x.is_nan() || x == 0.0 { x } else { x.signum() }
    }

    /// `Math.sqrt(x)`.
    fn sqrt(x: f64) -> f64 {
        x.sqrt()
    }

    /// `Math.cbrt(x)`.
    fn cbrt(x: f64) -> f64 {
        x.cbrt()
    }

    /// `Math.exp(x)`.
    fn exp(x: f64) -> f64 {
        x.exp()
    }

    /// `Math.expm1(x)`.
    fn expm1(x: f64) -> f64 {
        x.exp_m1()
    }

    /// `Math.log(x)`.
    fn log(x: f64) -> f64 {
        x.ln()
    }

    /// `Math.log1p(x)`.
    fn log1p(x: f64) -> f64 {
        x.ln_1p()
    }

    /// `Math.log2(x)`.
    fn log2(x: f64) -> f64 {
        x.log2()
    }

    /// `Math.log10(x)`.
    fn log10(x: f64) -> f64 {
        x.log10()
    }

    /// `Math.sin(x)`.
    fn sin(x: f64) -> f64 {
        x.sin()
    }

    /// `Math.cos(x)`.
    fn cos(x: f64) -> f64 {
        x.cos()
    }

    /// `Math.tan(x)`.
    fn tan(x: f64) -> f64 {
        x.tan()
    }

    /// `Math.asin(x)`.
    fn asin(x: f64) -> f64 {
        x.asin()
    }

    /// `Math.acos(x)`.
    fn acos(x: f64) -> f64 {
        x.acos()
    }

    /// `Math.atan(x)`.
    fn atan(x: f64) -> f64 {
        x.atan()
    }

    /// `Math.atan2(y, x)`.
    fn atan2(y: f64, x: f64) -> f64 {
        y.atan2(x)
    }

    /// `Math.sinh(x)`.
    fn sinh(x: f64) -> f64 {
        x.sinh()
    }

    /// `Math.cosh(x)`.
    fn cosh(x: f64) -> f64 {
        x.cosh()
    }

    /// `Math.tanh(x)`.
    fn tanh(x: f64) -> f64 {
        x.tanh()
    }

    /// `Math.asinh(x)`.
    fn asinh(x: f64) -> f64 {
        x.asinh()
    }

    /// `Math.acosh(x)`.
    fn acosh(x: f64) -> f64 {
        x.acosh()
    }

    /// `Math.atanh(x)`.
    fn atanh(x: f64) -> f64 {
        x.atanh()
    }

    /// `Math.hypot(x, y)`.
    ///
    /// Two arguments rather than any number, because a call carries four slots
    /// and a variadic member would have to read the argument vector — which is
    /// the same trade every other built-in here makes, named where it is paid.
    fn hypot(x: f64, y: f64) -> f64 {
        x.hypot(y)
    }

    /// `Math.pow(base, exponent)`.
    fn pow(base: f64, exponent: f64) -> f64 {
        base.powf(exponent)
    }

    /// `Math.fround(x)` — the nearest single-precision value.
    fn fround(x: f64) -> f64 {
        x as f32 as f64
    }

    /// `Math.max(a, b)`, either of which may be left out.
    ///
    /// Two things the obvious spelling gets wrong, and both are silent.
    ///
    /// **`f64::max` answers the other operand for a `NaN`.** The language
    /// propagates it — `Math.max(NaN, 1)` is `NaN` — and the other way round
    /// makes a comparison over unknown data quietly succeed.
    ///
    /// **The identity is not zero.** `Math.max()` is `-Infinity`, so a member
    /// declaring two `f64` parameters would coerce a missing argument to `NaN`
    /// and answer `NaN` for `Math.max(1)`, which is a wrong answer to a call
    /// programs really write. That is why these two take the values as they
    /// arrived rather than coerced.
    fn max(a: u64, b: u64) -> f64 {
        folded(a, b, f64::NEG_INFINITY, |x, y| if x > y { x } else { y })
    }

    /// `Math.min(a, b)`, with the same two rules as [`Math::max`].
    fn min(a: u64, b: u64) -> f64 {
        folded(a, b, f64::INFINITY, |x, y| if x < y { x } else { y })
    }
}
