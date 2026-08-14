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

use super::with_current;

/// Every argument a variadic member was called with.
///
/// `Math.max(1, 5, 3, 8, 2)` is a program people write, and a member reading only
/// the four slots the convention carries answers 5 — a plausible number, which is
/// the expensive kind of wrong. The compiler already puts the rest in a vector the
/// runtime holds; [`super::array_proto::arguments_at`] is what reads it back, and
/// it records why it does that rather than calling `rest_arguments`.
///
/// The borrow is taken and given back HERE, before the first coercion, because
/// `to_number` takes one of its own. Nesting them is a panic on the re-entry,
/// which is the trap this whole authoring layer exists to make structural.
fn given(a0: u64, a1: u64, a2: u64, a3: u64) -> Vec<u64> {
    with_current(|context| super::array_proto::arguments_at(context, 0, [a0, a1, a2, a3]))
}

/// The comparison [`Math::max`] folds with.
///
/// Not `if x > y { x } else { y }` on its own: `-0.0 > 0.0` is `false` in IEEE
/// 754, so that comparison alone answers whichever operand arrived second for
/// a tie between the two zeros — `Math.max(-0, 0)` and `Math.max(0, -0)` would
/// disagree, and the language does not. The zero case is broken out and
/// answers `+0` regardless of which side it came from.
fn max2(x: f64, y: f64) -> f64 {
    if x > y {
        x
    } else if y > x {
        y
    } else if x == 0.0 && y == 0.0 {
        if x.is_sign_positive() { x } else { y }
    } else {
        y
    }
}

/// The comparison [`Math::min`] folds with. See [`max2`] for why the zero case
/// is separate; here it answers `-0` regardless of which side it came from.
fn min2(x: f64, y: f64) -> f64 {
    if x < y {
        x
    } else if y < x {
        y
    } else if x == 0.0 && y == 0.0 {
        if x.is_sign_negative() { x } else { y }
    } else {
        y
    }
}

/// The fold `max` and `min` are, over however many arguments arrived.
///
/// The identity is what the language folds from — `-Infinity` for `max`,
/// `Infinity` for `min` — so a call with no arguments answers it, and one with
/// a single argument answers that argument rather than a comparison against
/// zero.
fn folded(values: &[u64], identity: f64, better: fn(f64, f64) -> f64) -> f64 {
    let mut answer = identity;
    for value in values {
        let number = super::class_support::to_number(*value);
        if number.is_nan() {
            return f64::NAN;
        }
        answer = better(answer, number);
    }
    answer
}

/// `Math`.
#[rtse::class("Math", namespace, tag)]
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
    /// **Not** `(x + 0.5).floor()`, which looks like "half up" and is wrong
    /// twice over. First the sign: `-0.5 + 0.5` is `+0.0`, so `floor` answers
    /// `+0` where the language requires `-0` — the specification carves that
    /// case out explicitly (`x < 0` and `x >= -0.5` answers `-0`) rather than
    /// leaving it to the arithmetic. Second the rounding itself: adding `0.5`
    /// is not exact for every double, so `0.49999999999999994 + 0.5` **rounds
    /// up to `1.0`** in double arithmetic even though the true sum is below it,
    /// and `floor` then answers `1` where the language answers `0`. Working from
    /// `floor(x)` and the fractional remainder avoids that addition entirely.
    fn round(x: f64) -> f64 {
        if x.is_nan() || x.is_infinite() || x == 0.0 {
            return x;
        }
        if x > 0.0 && x < 0.5 {
            return 0.0;
        }
        if x < 0.0 && x >= -0.5 {
            return -0.0;
        }
        let floor = x.floor();
        let fraction = x - floor;
        // A tie rounds toward +Infinity, which `< 0.5` (not `<=`) gives: a
        // fraction of exactly 0.5 falls through to `floor + 1.0`.
        if fraction < 0.5 { floor } else { floor + 1.0 }
    }

    /// `Math.trunc(x)`.
    fn trunc(x: f64) -> f64 {
        x.trunc()
    }

    /// `Math.clz32(x)` — leading zero bits of the value as a 32-bit integer.
    ///
    /// The conversion is `ToUint32`, which is what makes `clz32(-1)` zero rather
    /// than an error: the language defines this over the 32-bit view of a double,
    /// and `as u32` on a negative or fractional one is not that view. `NaN` and
    /// infinity convert to zero, whose 32-bit form has all 32 bits clear.
    fn clz32(x: f64) -> f64 {
        f64::from(to_uint32(x).leading_zeros())
    }

    /// `Math.imul(a, b)` — the 32-bit integer product, wrapping.
    ///
    /// SIGNED, and that is the whole point of it: `imul(0xffffffff, 5)` is `-5`,
    /// where a double multiply answers 21474836475. The operands convert through
    /// `ToUint32` and the product is read back as `i32`, which is `ToInt32` of
    /// the wrapped result.
    fn imul(a: f64, b: f64) -> f64 {
        let product = to_uint32(a).wrapping_mul(to_uint32(b));
        f64::from(product as i32)
    }

    /// `Math.sign(x)`.
    ///
    /// Not `f64::signum`, which answers `1` for `+0` and `-1` for `-0` where the
    /// language answers the zero itself.
    fn sign(x: f64) -> f64 {
        if x.is_nan() || x == 0.0 { x } else { x.signum() }
    }

    /// `Math.random()` — a double in `[0, 1)`.
    ///
    /// # Why a generator written here rather than a dependency
    ///
    /// This crate has no source of randomness and adding one is a dependency on
    /// every target it must run on, wasm included. `Math.random` is specified as
    /// "implementation-dependent, chosen pseudo-randomly with approximately
    /// uniform distribution" and is explicitly NOT cryptographic — the language
    /// has `crypto.getRandomValues` for that, which is a different surface with
    /// a different guarantee. So a small generator satisfies the whole contract,
    /// and pretending otherwise by reaching for a CSPRNG would state a promise
    /// this member does not make.
    ///
    /// xorshift64*, seeded once per thread from the clock. Per THREAD rather
    /// than per process, so two workers do not walk the same sequence in step —
    /// which is what a shared static would produce and what a program spawning
    /// workers to sample would silently get wrong.
    ///
    /// The 53 bits are taken from the TOP of the word. The low bits of an
    /// xorshift have the weakest distribution, so `(x % 2^53) / 2^53` — the
    /// obvious spelling — is the one that shows structure.
    fn random() -> f64 {
        draw()
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

    /// `Math.hypot(…)` — the square root of the sum of the squares.
    ///
    /// Not folded pairwise with `f64::hypot`, which was tried and is not the
    /// same function over three or more arguments: folding computes
    /// `hypot(hypot(a, b), c)`, an extra rounding step `hypot(a, b, c)` never
    /// takes, so it can be off by a ULP from the value the specification's
    /// single sum-of-squares defines. It also gets the NaN/Infinity precedence
    /// backwards through the fold order: **the specification checks every
    /// argument for `+Infinity`/`-Infinity` FIRST and answers `Infinity`
    /// regardless of a `NaN` elsewhere in the list** —
    /// `Math.hypot(Infinity, NaN)` is `Infinity` — but a left-to-right fold
    /// that meets the `NaN` before the `Infinity` returns `NaN` from
    /// [`folded`]'s early-out and never looks further.
    ///
    /// So this scans for infinity and NaN up front, in that order, and only
    /// then sums: scaled by the largest magnitude, the same technique
    /// `f64::hypot` itself uses internally, so a lone pair still answers what
    /// `f64::hypot` would and `Math.hypot(1e200, 1e200)` stays finite.
    ///
    /// Zero is the identity, which is also the answer the language gives for no
    /// arguments at all.
    fn hypot(a: u64, b: u64, c: u64, d: u64) -> f64 {
        let numbers: Vec<f64> = given(a, b, c, d)
            .into_iter()
            .map(super::class_support::to_number)
            .collect();
        if numbers.iter().any(|n| n.is_infinite()) {
            return f64::INFINITY;
        }
        if numbers.iter().any(|n| n.is_nan()) {
            return f64::NAN;
        }
        let largest = numbers.iter().fold(0.0f64, |acc, n| acc.max(n.abs()));
        if largest == 0.0 {
            return 0.0;
        }
        let sum_of_scaled_squares: f64 = numbers.iter().map(|n| (n / largest).powi(2)).sum();
        largest * sum_of_scaled_squares.sqrt()
    }

    /// `Math.pow(base, exponent)`.
    fn pow(base: f64, exponent: f64) -> f64 {
        base.powf(exponent)
    }

    /// `Math.fround(x)` — the nearest single-precision value.
    fn fround(x: f64) -> f64 {
        x as f32 as f64
    }

    /// `Math.max(…)`, over however many arguments arrived.
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
    /// arrived rather than coerced: `given` drops the padding, so an argument
    /// that was never written is never coerced.
    fn max(a: u64, b: u64, c: u64, d: u64) -> f64 {
        folded(&given(a, b, c, d), f64::NEG_INFINITY, max2)
    }

    /// `Math.min(…)`, with the same two rules as [`Math::max`].
    fn min(a: u64, b: u64, c: u64, d: u64) -> f64 {
        folded(&given(a, b, c, d), f64::INFINITY, min2)
    }
}

thread_local! {
    /// The generator's state, per thread. Zero means "not seeded yet", which is
    /// also the one state xorshift cannot leave — so the check that seeds it is
    /// the same check that keeps it out of the fixed point.
    static RANDOM: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// One draw of the generator, for the two callers that need it.
///
/// A free function rather than only the member, because compiled code reaches
/// it BOTH ways: through `Math.random` as a property, and through
/// `math_random` when the whole program proves that name still means this.
/// Two copies of a generator would be two sequences, and a program sampling
/// through both spellings would see the seam.
pub(super) fn draw() -> f64 {
    RANDOM.with(|state| {
        let mut word = state.get();
        if word == 0 {
            word = seed();
        }
        word ^= word << 13;
        word ^= word >> 7;
        word ^= word << 17;
        state.set(word);
        let scrambled = word.wrapping_mul(0x2545_f491_4f6c_dd1d);
        (scrambled >> 11) as f64 / (1u64 << 53) as f64
    })
}

/// `Math.random()`, reached directly.
///
/// The generator is a thread-local xorshift and costs a handful of
/// instructions; the 40 ns a call cost was the PATH — a property read through
/// the chain cache, then the generic call machinery, to reach it. This entry
/// point is what the emitter calls once the whole program proves `Math` is
/// still the primordial, and it takes no context borrow because a draw needs
/// no heap.
///
/// Not an instruction: there is no opcode for a generator, and an instruction
/// that expanded to a call would make the cost of emitting one unreadable —
/// which is the property the machine's vocabulary exists for.
#[rtse::entry]
pub fn math_random() -> f64 {
    draw()
}

/// A starting word, from the clock and this thread's identity.
///
/// Neither alone is enough: the clock alone gives two threads started in the
/// same nanosecond the same stream, and the thread id alone repeats exactly
/// across runs. Mixed, they differ in both directions — which is all
/// `Math.random` promises.
fn seed() -> u64 {
    let since = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|held| held.as_nanos() as u64)
        .unwrap_or(0x9e37_79b9_7f4a_7c15);
    let thread = {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        std::thread::current().id().hash(&mut hasher);
        hasher.finish()
    };
    // Never zero: that is xorshift's fixed point, and a seed landing on it would
    // make every draw the same number forever.
    (since ^ thread.rotate_left(32)) | 1
}

/// `ToUint32`, which is what the two bitwise members of `Math` are defined over.
///
/// Not `as u32`. That saturates a negative double to zero and an out-of-range one
/// to the maximum, where the language wraps modulo 2^32 — so `Math.imul(-1, 1)`
/// would answer 0 instead of -1, and every hash function written with it would
/// quietly produce different numbers than everywhere else.
fn to_uint32(value: f64) -> u32 {
    if !value.is_finite() || value == 0.0 {
        return 0;
    }
    let truncated = value.trunc();
    let wrapped = truncated.rem_euclid(4_294_967_296.0);
    wrapped as u32
}
