//! Which number names which `Math` library member, for `MathDirect1`/`2`.
//!
//! An agreement between this crate and `rts-core`, stated on both sides as a
//! list of names in one order and asserted equal by `rts-host`, which is the
//! one crate that may read both — the same shape as `ARGUMENT_SLOTS`. The
//! runtime holds the functions; this crate holds only the names, because a
//! name is what the emitter has in hand when it decides.
//!
//! Why a number and not the name itself: the runtime would have to compare
//! text on every call, and the whole point of the door is that nothing is
//! looked up at run time.

/// The one-operand members `Math.f(x)` reaches directly, in the order the
/// runtime's table is indexed. `rts_core::entry::MATH_UNARY_NAMES` is the
/// other half.
pub const UNARY_NAMES: [&str; 20] = [
    "sin", "cos", "tan", "asin", "acos", "atan", "sinh", "cosh", "tanh", "asinh", "acosh", "atanh",
    "exp", "expm1", "log", "log1p", "log2", "log10", "cbrt", "f16round",
];

/// The two-operand members `Math.f(a, b)` reaches directly.
/// `rts_core::entry::MATH_BINARY_NAMES` is the other half.
pub const BINARY_NAMES: [&str; 3] = ["atan2", "pow", "hypot"];

/// The number a one-operand member goes by, or `None` for a name that is not
/// one — `sqrt`, which is an instruction, or `foo`, which is nothing.
pub fn unary_index(name: &str) -> Option<i64> {
    UNARY_NAMES.iter().position(|held| *held == name).map(|at| at as i64)
}

/// The number a two-operand member goes by.
pub fn binary_index(name: &str) -> Option<i64> {
    BINARY_NAMES.iter().position(|held| *held == name).map(|at| at as i64)
}
