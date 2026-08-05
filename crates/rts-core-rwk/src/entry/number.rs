//! `Number` and `Boolean`: the conversions, and the facts about a double.
//!
//! # Why the members are static and the prototypes are empty
//!
//! `Number.isNaN(x)` is reached on the constructor. `(5).toFixed(2)` is reached
//! on a *primitive receiver*, and a primitive number has no cell — so the chain
//! walk that substitutes `String.prototype` for a text cell has nothing to
//! substitute against here, because there is nothing to walk from. Writing
//! `toFixed` would produce a method no expression can reach.
//!
//! That is a gap in property access rather than in this module, and it is named
//! rather than papered over: the fix is for a read of a property on a double to
//! reach a substituted prototype the way a read on a string does, and it belongs
//! in [`super::objects::inherited_from`] beside the two substitutions already
//! there.
//!
//! # Why `Number.isNaN` does not coerce and `Number(x)` does
//!
//! They are different questions, and the language spells the difference
//! deliberately. The global `isNaN("abc")` is `true` because it converts first;
//! `Number.isNaN("abc")` is `false`, because the argument is not a number at
//! all. An implementation that coerced in both would make the second useless —
//! it exists precisely to be the one that does not.
//!
//! So these take `u64` and ask what arrived, where `Math`'s members take `f64`
//! and let the wrapper convert. The parameter type is the statement.

use super::objects::undefined_of;
use super::with_current;
use crate::value::Value;

/// `Number`.
#[rtse::class("Number")]
impl Number {
    /// The largest integer a double represents exactly.
    #[stat]
    const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;
    /// The smallest.
    #[stat]
    const MIN_SAFE_INTEGER: f64 = -9_007_199_254_740_991.0;
    /// The largest finite double.
    #[stat]
    const MAX_VALUE: f64 = f64::MAX;
    /// The smallest positive double, subnormals included.
    #[stat]
    const MIN_VALUE: f64 = 5e-324;
    /// The difference between 1 and the next double above it.
    #[stat]
    const EPSILON: f64 = f64::EPSILON;
    /// `Number.POSITIVE_INFINITY`.
    #[stat]
    const POSITIVE_INFINITY: f64 = f64::INFINITY;
    /// `Number.NEGATIVE_INFINITY`.
    #[stat]
    const NEGATIVE_INFINITY: f64 = f64::NEG_INFINITY;
    /// `Number.NaN`.
    #[stat]
    const NaN: f64 = f64::NAN;

    /// `Number(x)` — the numeric value of an argument.
    ///
    /// `new Number(x)` answers the plain object `construct` made, because a
    /// primitive is not an object and a constructor returning one does not win.
    /// The same stated divergence `String` has, and the less wrong of the two:
    /// a wrapper that compared equal to a primitive everywhere except where it
    /// did not is the kind of wrong that is hard to find.
    #[construct]
    fn convert(this: u64, value: u64) -> u64 {
        let _ = this;
        let absent = with_current(|context| undefined_of(context));
        match value == absent {
            // `Number()` is `0`, not `NaN`. The argument being left out is not
            // the same as `undefined` being passed, and this is the one place
            // the difference is visible.
            true => Value::from_f64(0.0).bits(),
            false => Value::from_f64(super::class_support::to_number(value)).bits(),
        }
    }

    /// `Number.isNaN(x)` — without converting. See the module documentation.
    #[stat]
    #[js("isNaN")]
    fn is_nan(value: u64) -> bool {
        as_double(value).is_some_and(f64::is_nan)
    }

    /// `Number.isFinite(x)` — without converting.
    #[stat]
    fn is_finite(value: u64) -> bool {
        as_double(value).is_some_and(f64::is_finite)
    }

    /// `Number.isInteger(x)`.
    #[stat]
    fn is_integer(value: u64) -> bool {
        as_double(value).is_some_and(|number| number.is_finite() && number.fract() == 0.0)
    }

    /// `Number.isSafeInteger(x)`.
    #[stat]
    fn is_safe_integer(value: u64) -> bool {
        as_double(value).is_some_and(|number| {
            number.is_finite() && number.fract() == 0.0 && number.abs() <= 9_007_199_254_740_991.0
        })
    }

    /// `Number.parseFloat(s)`.
    ///
    /// The same function as the global `parseFloat`, which is what the
    /// specification says it is — not a second implementation that would come to
    /// disagree about a leading sign.
    #[stat]
    fn parse_float(value: u64) -> f64 {
        leading(value, |text| {
            let end = float_prefix(text);
            match end {
                0 => f64::NAN,
                _ => text[..end].parse().unwrap_or(f64::NAN),
            }
        })
    }

    /// `Number.parseInt(s, radix)`.
    #[stat]
    fn parse_int(value: u64, radix: u64) -> f64 {
        // The borrow is given back before `to_number` takes one of its own.
        // Nesting them is a panic on the re-entry, and an `extern "C"` frame
        // cannot unwind — so it aborts the process rather than failing a test,
        // which is how this one was found.
        let absent = with_current(|context| undefined_of(context));
        let base = match radix == absent {
            true => 0,
            false => super::class_support::to_number(radix) as i64,
        };
        leading(value, move |text| integer_prefix(text, base))
    }
}

/// `Boolean`.
#[rtse::class("Boolean")]
impl Boolean {
    /// `Boolean(x)` — `ToBoolean` of an argument.
    ///
    /// `new Boolean(x)` answers the object, for the reason [`Number::convert`]
    /// records.
    #[construct]
    fn convert(this: u64, value: u64) -> bool {
        let _ = this;
        super::class_support::to_boolean(value)
    }
}

/// The double a value holds, when it genuinely holds one.
///
/// `None` for everything else, which is what makes `Number.isNaN` answer false
/// for a string rather than converting it.
fn as_double(value: u64) -> Option<f64> {
    Value(value).as_f64()
}

/// Runs a parse over an argument's text, after `ToString` of it.
///
/// The borrow is given back before the parse runs, which costs a copy of the
/// text and buys the rule this crate keeps: nothing calls out from inside one.
fn leading(value: u64, parse: impl FnOnce(&str) -> f64) -> f64 {
    let text = with_current(|context| {
        super::text::to_text(context, Value(value)).and_then(|text| text.to_rust())
    });
    match text {
        Some(text) => parse(text.trim_start()),
        None => f64::NAN,
    }
}

/// How many bytes of a decimal number the text starts with.
///
/// A prefix rather than the whole string, which is the whole difference between
/// `parseFloat` and `Number`: `parseFloat("3.5px")` is `3.5` and `Number("3.5px")`
/// is `NaN`.
fn float_prefix(text: &str) -> usize {
    let bytes = text.as_bytes();
    let mut at = 0;
    if matches!(bytes.first(), Some(b'+' | b'-')) {
        at += 1;
    }
    let mut seen_digit = false;
    while at < bytes.len() && bytes[at].is_ascii_digit() {
        at += 1;
        seen_digit = true;
    }
    if at < bytes.len() && bytes[at] == b'.' {
        at += 1;
        while at < bytes.len() && bytes[at].is_ascii_digit() {
            at += 1;
            seen_digit = true;
        }
    }
    if !seen_digit {
        return 0;
    }
    // An exponent only counts when it is complete: `"1e"` is `1`, not a parse
    // error, because the prefix that parsed is what the answer is.
    let mantissa = at;
    if at < bytes.len() && matches!(bytes[at], b'e' | b'E') {
        let mut after = at + 1;
        if matches!(bytes.get(after), Some(b'+' | b'-')) {
            after += 1;
        }
        let start = after;
        while after < bytes.len() && bytes[after].is_ascii_digit() {
            after += 1;
        }
        if after > start {
            return after;
        }
    }
    mantissa
}

/// The integer a text starts with, in the radix the language picks.
///
/// A radix of zero means "not given", which is the one case where the text
/// decides: `0x` selects sixteen and everything else is ten. Written as a
/// sentinel rather than an `Option` because the argument arrives as a number and
/// `parseInt(s, 0)` means exactly the same thing.
fn integer_prefix(text: &str, radix: i64) -> f64 {
    let mut rest = text;
    let mut negative = false;
    if let Some(stripped) = rest.strip_prefix('-') {
        negative = true;
        rest = stripped;
    } else if let Some(stripped) = rest.strip_prefix('+') {
        rest = stripped;
    }

    let mut base = radix;
    if base == 16 || base == 0 {
        if let Some(stripped) = rest.strip_prefix("0x").or_else(|| rest.strip_prefix("0X")) {
            rest = stripped;
            base = 16;
        }
    }
    if base == 0 {
        base = 10;
    }
    if !(2..=36).contains(&base) {
        return f64::NAN;
    }

    let mut digits = 0;
    let mut answer = 0.0f64;
    for character in rest.chars() {
        let Some(digit) = character.to_digit(base as u32) else {
            break;
        };
        // Accumulated as a double rather than an integer, because that is the
        // type the answer has: `parseInt` of a hundred digits is a finite
        // double with the precision a double has, not an overflow.
        answer = answer * f64::from(base as u32) + f64::from(digit);
        digits += 1;
    }
    if digits == 0 {
        return f64::NAN;
    }
    match negative {
        true => -answer,
        false => answer,
    }
}
