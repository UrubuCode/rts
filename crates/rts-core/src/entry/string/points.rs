//! Code points, and the `String` statics that build a string out of numbers.
//!
//! # Why `codePointAt` sits with `fromCharCode` and `fromCodePoint`
//!
//! They are the same decision seen from both ends. `String.fromCharCode` builds
//! a string out of numbers and `s.codePointAt` takes one apart, and all of them
//! have to agree about what a surrogate pair is — a file each is where one of
//! them would learn about pairs and the other would not, which is
//! `"😀".codePointAt(0)` answering 55357 while `String.fromCodePoint(128512)`
//! answers two units. [`point_at`] is the one place that agreement is written.
//!
//! `String.raw` is here because it is the third static, and a list of what
//! `String` itself holds that is split across two files is a list that will be
//! installed from one of them.

use super::super::native::Native;
use super::super::with_current;
use super::{answer, arg_units, nothing, units_of};
use crate::value::Value;

/// What a string's prototype holds that reads whole code points.
pub(super) const NATIVES: &[(&str, Native)] = &[("codePointAt", code_point_at)];

/// What `String` itself holds, each with the arity the specification pins.
///
/// Statics, because they build a string out of nothing — there is no receiver
/// for them to be a method on, which is exactly why the language put them on the
/// constructor rather than on the prototype.
///
/// # Why the number is carried here rather than defaulted at the install
///
/// All three are variadic and all three have `length` **1** — ECMA-262 gives
/// `String.fromCharCode`, `String.fromCodePoint` and `String.raw` one declared
/// parameter each, the rest being a rest element, which `SetFunctionLength`
/// does not count. A shared `0` would have replaced `undefined` with a number
/// that is wrong for every one of them, and a wrong `length` is worse than an
/// absent one: a program that forwards a function by arity (`fn.length ? … : …`)
/// takes the other branch and keeps running.
///
/// It matters because these are read as VALUES. `const cc = String.fromCharCode`
/// is what the fixture that found this does, and `cc.length` answered
/// `undefined` where every engine answers 1.
pub(super) const STATICS: &[(&str, Native, u32)] = &[
    ("fromCharCode", from_char_code, 1),
    ("fromCodePoint", from_code_point, 1),
    ("raw", raw, 1),
];

/// `s.codePointAt(i)` — the whole character at a position, not half of one.
///
/// The difference from `charCodeAt` is the entire reason this method exists: at
/// index 0 of `"😀"` that one answers the high surrogate 55357 and this answers
/// 128512. An implementation that shared their bodies would have to pick one,
/// and either choice makes the other method pointless.
extern "C" fn code_point_at(_e: u64, this: u64, index: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    with_current(|context| {
        let Some(units) = units_of(context, this) else {
            return nothing(context);
        };
        let asked = super::integer_arg(context, index);
        // `undefined` out of range, where `charCodeAt` answers `NaN`. The
        // language says which is which, and they are not interchangeable: only
        // one of them survives `typeof`.
        if asked < 0.0 || asked >= units.len() as f64 {
            return nothing(context);
        }
        let at = asked as usize;
        Value::from_f64(f64::from(point_at(&units, at))).bits()
    })
}

/// `String.fromCharCode(…)` — units, taken one at a time.
///
/// Each argument is truncated to sixteen bits the way `ToUint16` does, so
/// `String.fromCharCode(65601)` is `"A"`. That wrap is not a quirk to be
/// defended against: it is what makes the method total, and a version that
/// rejected out-of-range numbers would throw where the language wraps.
extern "C" fn from_char_code(_e: u64, _this: u64, a0: u64, a1: u64, a2: u64, a3: u64) -> u64 {
    with_current(|context| {
        // Past four the compiler spilled the rest into a vector the runtime
        // holds; `arguments_at` is the one place that reads it back. Folding
        // over `carried`'s four slots alone answered `"Hell"` for
        // `String.fromCharCode(72,101,108,108,111)` — the fifth argument was
        // never read, not merely truncated by `ToUint16`.
        let mut units = Vec::new();
        for given in super::super::array_proto::arguments_at(context, 0, [a0, a1, a2, a3]) {
            units.push(as_unit(context, given));
        }
        answer(context, &units)
    })
}

/// `String.fromCodePoint(…)` — characters, which may be two units each.
///
/// The pair with `fromCharCode`, and the reason both exist: `fromCharCode` is
/// given units and this is given characters, so `String.fromCodePoint(128512)`
/// is one emoji of length two where `String.fromCharCode(128512)` is one unit of
/// the wrong character.
extern "C" fn from_code_point(_e: u64, _this: u64, a0: u64, a1: u64, a2: u64, a3: u64) -> u64 {
    with_current(|context| {
        let mut units = Vec::new();
        // Same fix as `from_char_code`: past four, read the spilled vector
        // rather than the four convention slots alone.
        for given in super::super::array_proto::arguments_at(context, 0, [a0, a1, a2, a3]) {
            let number =
                super::super::operators::as_number(context, Value(given)).unwrap_or(f64::NAN);
            // A code point that is not a whole number in range is a `RangeError`.
            // The empty string is the same stated gap `repeat` takes for the same
            // reason — throwing cannot find a handler here — and it is bounded,
            // where letting the number through would build lone surrogates that
            // compare equal to nothing.
            if !(0.0..=1_114_111.0).contains(&number) || number.fract() != 0.0 {
                return answer(context, &[]);
            }
            let point = number as u32;
            if point < 0x10000 {
                units.push(point as u16);
            } else {
                let shifted = point - 0x10000;
                units.push(0xD800 + (shifted >> 10) as u16);
                units.push(0xDC00 + (shifted & 0x3FF) as u16);
            }
        }
        answer(context, &units)
    })
}

/// `String.raw(strings, …)` — a tagged template's text, escapes unread.
///
/// The `raw` property of the first argument, joined with the substitutions
/// between.
///
/// # Why the pieces are read by INDEX rather than out of an elements vector
///
/// Because `raw` need not be an array. The specification reads it with
/// `LengthOfArrayLike` and then `Get(raw, ToString(i))`, so
/// `String.raw({ raw: { 0: "a", 1: "b", length: 2 } }, x)` is a legal call that
/// every engine answers — and reading `elements_at` answered the empty list for
/// it, so the whole call produced `""`. The array case still goes through the
/// same two reads; an array's `length` and its indices are ordinary properties.
///
/// The substitutions come from [`super::super::array_proto::arguments_at`], so
/// a template with more than three of them is no longer cut off at the
/// convention's four slots — which was `String.raw` of any template with four
/// interpolations losing the fourth.
extern "C" fn raw(_e: u64, _this: u64, strings: u64, a: u64, b: u64, c: u64) -> u64 {
    let (raw_key, substitutions) = with_current(|context| {
        (
            context.intern_value(crate::text::Str::from_str("raw")).bits(),
            // From slot ONE: the first argument is the call site's strings, not
            // a substitution, which is what `from` exists for.
            super::super::array_proto::arguments_at(context, 1, [strings, a, b, c]),
        )
    });
    let parts = super::super::computed::get_indexed(strings, raw_key);
    let length = with_current(|context| context.intern_value(crate::text::Str::from_str("length")).bits());
    let count = super::super::computed::get_indexed(parts, length);
    let count = with_current(|context| {
        super::super::operators::as_number(context, Value(count)).unwrap_or(0.0)
    });
    // Bounded the way `repeat` and `padStart` are, and for the same reason: the
    // length is a number a program chose, and an array-like claiming a billion
    // pieces would be a billion property reads before anything else went wrong.
    let count = count.clamp(0.0, 4096.0) as usize;

    let mut out: Vec<u16> = Vec::new();
    for at in 0..count {
        let key = with_current(|context| {
            context
                .intern_value(crate::coerce::number_to_string(at as f64))
                .bits()
        });
        let piece = super::super::computed::get_indexed(parts, key);
        with_current(|context| {
            if let Some(units) = arg_units(context, piece) {
                out.extend(units);
            }
        });
        // A substitution goes BETWEEN parts, so the last part has none after it
        // — which is what makes `String.raw` of a template with no substitutions
        // the template itself rather than a trailing `undefined`.
        if at + 1 >= count {
            continue;
        }
        let Some(value) = substitutions.get(at).copied() else {
            continue;
        };
        // `ToString` of the substitution, which for an object means calling its
        // `toString` — user code, therefore OUTSIDE the borrow. `arg_units`
        // alone answers `None` for an object, so a substitution that was not
        // already a primitive was silently DROPPED: `` String.raw`a${obj}b` ``
        // produced `"ab"`, losing a piece rather than converting it.
        let value = super::super::primitive::to_primitive(value, crate::coerce::Hint::String);
        // Rule 8: a `toString` that threw did not answer, and appending
        // `undefined` would put the word into the result of a call that never
        // happened.
        if super::super::throw::in_flight() {
            return with_current(|context| super::super::objects::undefined_of(context));
        }
        with_current(|context| {
            if let Some(units) = arg_units(context, value) {
                out.extend(units);
            }
        });
    }
    with_current(|context| answer(context, &out))
}

/// The code point beginning at a position, joining a surrogate pair.
fn point_at(units: &[u16], at: usize) -> u32 {
    let first = units[at];
    let paired = (0xD800..0xDC00).contains(&first)
        && at + 1 < units.len()
        && (0xDC00..0xE000).contains(&units[at + 1]);
    if paired {
        0x10000 + ((u32::from(first) - 0xD800) << 10) + (u32::from(units[at + 1]) - 0xDC00)
    } else {
        u32::from(first)
    }
}

/// One argument as a code unit, the way `ToUint16` produces one.
fn as_unit(context: &super::Context, value: u64) -> u16 {
    let number = super::super::operators::as_number(context, Value(value)).unwrap_or(f64::NAN);
    if !number.is_finite() {
        // `NaN` and the infinities are zero under `ToUint16`, which is the
        // language rather than a fallback: the conversion truncates first, and
        // there is nothing to truncate.
        return 0;
    }
    (number.trunc() as i64).rem_euclid(65536) as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_surrogate_pair_is_one_code_point_and_two_units() {
        // The corner that makes `codePointAt` a different method from
        // `charCodeAt` rather than a longer spelling of it.
        let units: Vec<u16> = "😀".encode_utf16().collect();
        assert_eq!(units.len(), 2);
        assert_eq!(point_at(&units, 0), 128_512);
        // At the second unit there is no pair to join, so the low surrogate
        // stands for itself — which is what the language answers too.
        assert_eq!(point_at(&units, 1), u32::from(units[1]));
    }

    #[test]
    fn a_lone_high_surrogate_at_the_end_is_not_read_past() {
        // A string may legally end mid-pair. Joining without the bounds check is
        // a read one past the end, which is the difference between a wrong
        // answer and a crash.
        let units = [0xD83Du16];
        assert_eq!(point_at(&units, 0), 0xD83D);
    }
}
