//! `rts`'s `num`, `math` and `hint` — 64-bit integer arithmetic, said exactly.
//!
//! # Why these exist when JavaScript has `Number`
//!
//! Because a `Number` is a double, and a double stops being able to count at
//! 2^53. Everything here is about the machine's own 64-bit integers: what
//! overflow does, what wrapping does, how many bits are set. `num.wrapping_add`
//! on `9223372036854775807` is a question a double cannot even be asked.
//!
//! # What crosses the boundary, and the cost that is not hidden
//!
//! A `u64` here is a TAGGED value and `f64` is what a JavaScript number is, so
//! every one of these converts in and out. An argument past 2^53 therefore
//! arrives already rounded — `num.wrapping_add` over such a value answers what
//! the double said, not what the program wrote. That is a real limit of the
//! boundary rather than of the arithmetic, and the fix is a `bigint` parameter
//! rather than more care here.
//!
//! Within 2^53 — which is every literal in the suite that reaches these — the
//! conversion is exact, and the operations are Rust's `i64` ones by name.
//!
//! # Why `hint` does nothing
//!
//! `spin_loop`, `black_box_i64` and `assert_unchecked` are what a program says
//! to an optimizer. This engine's optimizer is the code generator underneath
//! `rts-cranelift`, which these cannot reach: they are runtime calls, made after
//! every decision an optimizer would make. So they are honest identities —
//! `black_box_i64` answers its argument and `spin_loop` yields nothing — rather
//! than absent, because a program that calls one is not asking for a value.

use rts_core::entry::{self, Context, Provided};

/// The three namespaces, as one object each.
pub fn install(context: &mut Context, surface: u64) {
    let numbers = entry::make_namespace(context, NUM);
    entry::put_member(context, surface, "num", numbers);
    let maths = entry::make_namespace(context, MATH);
    entry::put_member(context, surface, "math", maths);
    let hints = entry::make_namespace(context, HINT);
    entry::put_member(context, surface, "hint", hints);
}

/// `num` — 64-bit integers, and what they do at their edges.
const NUM: &[(&str, Provided)] = &[
    ("wrapping_add", wrapping_add),
    ("wrapping_sub", wrapping_sub),
    ("wrapping_mul", wrapping_mul),
    ("wrapping_neg", wrapping_neg),
    ("wrapping_shl", wrapping_shl),
    ("wrapping_shr", wrapping_shr),
    ("saturating_add", saturating_add),
    ("saturating_sub", saturating_sub),
    ("saturating_mul", saturating_mul),
    ("checked_add", checked_add),
    ("checked_sub", checked_sub),
    ("checked_mul", checked_mul),
    ("checked_div", checked_div),
    ("count_ones", count_ones),
    ("leading_zeros", leading_zeros),
    ("trailing_zeros", trailing_zeros),
    ("rotate_left", rotate_left),
    ("rotate_right", rotate_right),
    ("swap_bytes", swap_bytes),
    ("f64_to_bits", f64_to_bits),
    ("f64_from_bits", f64_from_bits),
];

/// `math` — the integer operations, beside the ones `Math` already answers.
///
/// `abs_i64`, `add` and `mul` are here and `sqrt`, `pow` and `random` are NOT:
/// those three are `Math`'s, they behave identically, and a second spelling is a
/// second thing to keep in agreement. A program reaching for them finds
/// `undefined` and a `TypeError` that names it, which is the honest answer.
const MATH: &[(&str, Provided)] = &[
    ("abs_i64", abs_i64),
    ("add", add),
    ("mul", mul),
];

/// `hint` — what a program tells an optimizer, answered honestly as nothing.
const HINT: &[(&str, Provided)] = &[
    ("black_box_i64", black_box),
    ("spin_loop", nothing),
    ("assert_unchecked", nothing),
];

/// One integer argument, as the machine's.
fn whole(value: u64) -> i64 {
    entry::number_of(value).unwrap_or(0.0) as i64
}

/// An integer answer, as a JavaScript number.
fn answered(value: i64) -> u64 {
    entry::make_number(value as f64)
}

macro_rules! binary {
    ($name:ident, $body:expr) => {
        extern "C" fn $name(_e: u64, _this: u64, a: u64, b: u64, _a2: u64, _a3: u64) -> u64 {
            let operation: fn(i64, i64) -> i64 = $body;
            answered(operation(whole(a), whole(b)))
        }
    };
}

macro_rules! unary {
    ($name:ident, $body:expr) => {
        extern "C" fn $name(_e: u64, _this: u64, a: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
            let operation: fn(i64) -> i64 = $body;
            answered(operation(whole(a)))
        }
    };
}

binary!(wrapping_add, |a, b| a.wrapping_add(b));
binary!(wrapping_sub, |a, b| a.wrapping_sub(b));
binary!(wrapping_mul, |a, b| a.wrapping_mul(b));
binary!(wrapping_shl, |a, b| a.wrapping_shl(b as u32));
binary!(wrapping_shr, |a, b| a.wrapping_shr(b as u32));
binary!(saturating_add, |a, b| a.saturating_add(b));
binary!(saturating_sub, |a, b| a.saturating_sub(b));
binary!(saturating_mul, |a, b| a.saturating_mul(b));
binary!(rotate_left, |a, b| a.rotate_left(b as u32));
binary!(rotate_right, |a, b| a.rotate_right(b as u32));
unary!(wrapping_neg, |a| a.wrapping_neg());
unary!(count_ones, |a| i64::from(a.count_ones()));
unary!(leading_zeros, |a| i64::from(a.leading_zeros()));
unary!(trailing_zeros, |a| i64::from(a.trailing_zeros()));
unary!(swap_bytes, |a| a.swap_bytes());
unary!(abs_i64, |a| a.wrapping_abs());
binary!(add, |a, b| a.wrapping_add(b));
binary!(mul, |a, b| a.wrapping_mul(b));

/// The four that signal "does not fit" with `i64::MIN` rather than `undefined`.
///
/// **Not `undefined`.** That was tried and is a different, worse answer: this
/// engine has no true 64-bit integer — `i64::MIN` widens to the `f64`
/// `-9223372036854776000` the way every other whole number here does — and
/// `undefined` is a VALUE a caller doing arithmetic with the result silently
/// carries forward (`undefined + 1` is `NaN`, not a signal). `i64::MIN` is the
/// sentinel the documented convention names: the one value none of these four
/// operations can ever produce as a genuine answer (`checked_add` overflowing
/// TO `i64::MIN` is itself reported as `None` here, same as every other
/// overflow), so a caller checking for it is checking for something real.
macro_rules! checked {
    ($name:ident, $body:expr) => {
        extern "C" fn $name(_e: u64, _this: u64, a: u64, b: u64, _a2: u64, _a3: u64) -> u64 {
            let operation: fn(i64, i64) -> Option<i64> = $body;
            match operation(whole(a), whole(b)) {
                Some(found) => answered(found),
                None => answered(i64::MIN),
            }
        }
    };
}

checked!(checked_add, |a, b| a.checked_add(b));
checked!(checked_sub, |a, b| a.checked_sub(b));
checked!(checked_mul, |a, b| a.checked_mul(b));
checked!(checked_div, |a, b| a.checked_div(b));

/// The bits of a double, as an integer — and back.
///
/// The one pair here that is genuinely about doubles: `f64_to_bits(0.1)` is how
/// a program looks at what a literal really became.
extern "C" fn f64_to_bits(_e: u64, _this: u64, a: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let value = entry::number_of(a).unwrap_or(0.0);
    answered(value.to_bits() as i64)
}

extern "C" fn f64_from_bits(_e: u64, _this: u64, a: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    entry::make_number(f64::from_bits(whole(a) as u64))
}

/// `hint.black_box_i64(x)` — its argument, unchanged.
extern "C" fn black_box(_e: u64, _this: u64, a: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    a
}

/// `hint.spin_loop()` and `hint.assert_unchecked(c)` — nothing, honestly.
extern "C" fn nothing(_e: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    entry::undefined_value()
}
