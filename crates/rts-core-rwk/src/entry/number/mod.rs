//! `Number` and `Boolean`: the conversions, and the facts about a double.
//!
//! # How a method on a primitive is reached at all
//!
//! `Number.isNaN(x)` is on the constructor and needs nothing. `(5).toFixed(2)`
//! is a read on a *primitive receiver*, and a number is not a cell — it is the
//! encoding itself — so the chain walk that substitutes `String.prototype` for a
//! text cell has nothing to walk from.
//!
//! [`super::objects::get_property`] therefore starts the lookup on this
//! prototype directly when the receiver is a double or a boolean, and calls
//! whatever it finds with the primitive as the receiver. That is why every
//! member here reads its own `this` through `ToNumber` rather than expecting an
//! object: there is no wrapper, and building one would make
//! `(5).valueOf() === 5` depend on unwrapping it again.
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

mod format;

use super::objects::undefined_of;
use super::with_current;
use crate::text::Str;
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

    /// `n.toString(radix)`.
    ///
    /// Base ten is the shortest round-tripping decimal, which is a different
    /// problem from writing digits in a base and is why the two are separate
    /// paths here rather than one loop with a special case.
    fn to_string(this: u64, radix: u64) -> u64 {
        let base = radix_of(radix);
        let number = super::class_support::to_number(this);
        with_current(|context| {
            let text = match base {
                0 | 10 => crate::coerce::number_to_string(number),
                base if (2..=36).contains(&base) => Str::from_str(&in_radix(number, base as u32)),
                // The specification throws a `RangeError`; this cannot yet, and
                // answering the decimal form is the least wrong of the values
                // available.
                _ => crate::coerce::number_to_string(number),
            };
            context.intern_value(text).bits()
        })
    }

    /// `n.valueOf()` — the number itself.
    fn value_of(this: u64) -> f64 {
        super::class_support::to_number(this)
    }

    /// `n.toFixed(digits)`.
    ///
    /// Rounds **half away from zero**, which is what the specification says and
    /// what Rust's own `{:.*}` does not: `{:.0}` of `2.5` is `2`, because Rust
    /// formats to nearest-even. `(2.5).toFixed(0)` is `"3"`.
    fn to_fixed(this: u64, digits: f64) -> u64 {
        let number = super::class_support::to_number(this);
        let places = match digits.is_nan() {
            true => 0,
            false => digits.trunc().clamp(0.0, 100.0) as usize,
        };
        with_current(|context| {
            let text = match number.is_finite() && number.abs() < 1e21 {
                true => Str::from_str(&fixed(number, places)),
                // Past 1e21 the specification falls back to the ordinary
                // `ToString`, which is why this is not a formatting width but a
                // branch.
                false => crate::coerce::number_to_string(number),
            };
            context.intern_value(text).bits()
        })
    }

    /// `n.toExponential(digits)`.
    ///
    /// The argument left out is not the same as zero digits: `(12).toExponential()`
    /// is `"1.2e+1"` and `(12).toExponential(0)` is `"1e+1"`. So it arrives as
    /// the value it was passed rather than as an `f64`.
    fn to_exponential(this: u64, digits: u64) -> u64 {
        let number = super::class_support::to_number(this);
        let places = places_of(digits);
        with_current(|context| {
            let text = Str::from_str(&format::exponential(number, places));
            context.intern_value(text).bits()
        })
    }

    /// `n.toPrecision(digits)`.
    ///
    /// With no argument it is `toString`, which the specification states and
    /// which is not the same as one significant digit.
    fn to_precision(this: u64, digits: u64) -> u64 {
        let number = super::class_support::to_number(this);
        // Asked BEFORE the borrow: `places_of` takes one of its own, and
        // nesting them aborts the process rather than failing a test — the trap
        // `radix_of` records paying for once already.
        let asked = places_of(digits);
        with_current(|context| {
            let text = match asked {
                // The specification's range is 1 to 100 and outside it throws a
                // `RangeError`; this cannot yet, so the decimal form is the
                // least wrong of the values available — the same answer
                // `toString` settles on for a bad radix.
                Some(places) if (1..=100).contains(&places) => {
                    Str::from_str(&format::precision(number, places))
                }
                _ => crate::coerce::number_to_string(number),
            };
            context.intern_value(text).bits()
        })
    }

    /// `n.toLocaleString()`.
    ///
    /// The plain decimal form, with no grouping. This crate carries no locale
    /// data — the same wall `normalize` and `localeCompare` stop at — and
    /// inventing one locale's separators would make the answer wrong for every
    /// program running under another. `"1,234"` is a claim about the reader, not
    /// about the number.
    fn to_locale_string(this: u64) -> u64 {
        let number = super::class_support::to_number(this);
        with_current(|context| {
            context.intern_value(crate::coerce::number_to_string(number)).bits()
        })
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
        let base = radix_of(radix);
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

    /// `b.toString()` — `"true"` or `"false"`.
    ///
    /// Reached because [`super::objects::get_property`] starts a read on a
    /// boolean here. Without it `true.toString()` was `undefined`, which is a
    /// method a program can see is missing rather than one it never looks for.
    fn to_string(this: u64) -> u64 {
        let text = match super::class_support::to_boolean(this) {
            true => "true",
            false => "false",
        };
        with_current(|context| context.intern_value(Str::from_str(text)).bits())
    }

    /// `b.valueOf()`.
    fn value_of(this: u64) -> bool {
        super::class_support::to_boolean(this)
    }
}

/// A number written in a base other than ten.
///
/// The integer part by repeated division and the fraction by repeated
/// multiplication, to twenty places — which is where a base-2 expansion of a
/// double stops carrying information rather than an arbitrary cut. The
/// specification leaves the exact digit count implementation-defined here, and
/// saying so is better than implying a precision that is not there.
fn in_radix(number: f64, base: u32) -> String {
    if !number.is_finite() {
        return crate::coerce::number_to_string(number)
            .to_rust()
            .unwrap_or_default();
    }
    let negative = number < 0.0;
    let number = number.abs();
    let mut whole = number.trunc();
    let mut fraction = number.fract();

    let digit = |value: u32| char::from_digit(value, base).unwrap_or('0');
    let mut left = String::new();
    if whole == 0.0 {
        left.push('0');
    }
    while whole >= 1.0 {
        let rest = whole % f64::from(base);
        left.push(digit(rest as u32));
        whole = (whole / f64::from(base)).trunc();
    }
    let mut out: String = left.chars().rev().collect();

    if fraction > 0.0 {
        out.push('.');
        for _ in 0..20 {
            if fraction == 0.0 {
                break;
            }
            fraction *= f64::from(base);
            out.push(digit(fraction.trunc() as u32));
            fraction = fraction.fract();
        }
    }
    match negative {
        true => format!("-{out}"),
        false => out,
    }
}

/// A number to a fixed number of decimal places, rounding half away from zero.
///
/// Written rather than delegated to `format!("{:.*}")`, which rounds to nearest
/// **even**: `format!("{:.0}", 2.5)` is `"2"` and `(2.5).toFixed(0)` is `"3"`.
/// That difference is invisible in every test whose values are not exactly half.
fn fixed(number: f64, places: usize) -> String {
    let scale = 10f64.powi(places as i32);
    let scaled = number * scale;
    // `f64::round` is half-away-from-zero, which is the rule here — the one
    // place in this crate where it is the right function rather than the wrong
    // one. `Math.round` wanted half-UP and had to be written out.
    let rounded = scaled.round();
    let value = rounded / scale;
    match places {
        0 => format!("{}", value.trunc() as i64),
        _ => format!("{value:.places$}"),
    }
}

/// A digit count an argument names, with `None` for "not given".
///
/// Its own function because two methods need the distinction and each answers
/// something different without it — `toExponential` shortens and `toPrecision`
/// becomes `toString`.
fn places_of(digits: u64) -> Option<usize> {
    let absent = with_current(|context| undefined_of(context));
    if digits == absent {
        return None;
    }
    let asked = super::class_support::to_number(digits);
    match asked.is_nan() {
        true => Some(0),
        false => Some(asked.trunc().clamp(0.0, 100.0) as usize),
    }
}

/// The double a value holds, when it genuinely holds one.
///
/// `None` for everything else, which is what makes `Number.isNaN` answer false
/// for a string rather than converting it.
fn as_double(value: u64) -> Option<f64> {
    Value(value).as_f64()
}

/// The radix an argument names, with zero meaning "not given".
///
/// The borrow is taken and given back before `to_number` takes one of its own.
/// Nesting them is a panic on the re-entry, and an `extern "C"` frame cannot
/// unwind — so it aborts the process rather than failing a test, which is how
/// this one was found.
pub(super) fn radix_of(radix: u64) -> i64 {
    let absent = with_current(|context| undefined_of(context));
    match radix == absent {
        true => 0,
        false => super::class_support::to_number(radix) as i64,
    }
}

/// Runs a parse over an argument's text, after `ToString` of it.
///
/// The borrow is given back before the parse runs, which costs a copy of the
/// text and buys the rule this crate keeps: nothing calls out from inside one.
pub(super) fn leading(value: u64, parse: impl FnOnce(&str) -> f64) -> f64 {
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
pub(super) fn float_prefix(text: &str) -> usize {
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
pub(super) fn integer_prefix(text: &str, radix: i64) -> f64 {
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