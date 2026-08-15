//! The string methods that neither search nor take a pattern.
//!
//! # Why every one of these counts code units
//!
//! Because a JavaScript string is a sequence of UTF-16 code units, and every
//! index in this file is a position in that sequence. Working in bytes would
//! make `"é".slice(1)` cut a character in half and `"😀".length` disagree with
//! `"😀".charAt(1)`; working in `char`s would make both of those *look* right
//! and answer differently from every other engine.
//!
//! So the receiver becomes `Vec<u16>` at the top of each method and the answer
//! is built from units. That is a copy per call, which was called "a real cost
//! and the honest starting point" here, with the note that indexing the two
//! layouts a `Str` can have belonged "after something measures it".
//!
//! **Something measured it.** For the three methods that read ONE unit —
//! `charAt`, `charCodeAt`, `at` — the copy is the whole cost, and it makes a
//! scan quadratic: 10 000 characters took 0.86 s, 20 000 took 3.17 s, 40 000
//! took 14.4 s, 80 000 took 63.1 s. Four times per doubling, and the suite file
//! that scans 100 000 never finished at all.
//!
//! Those three now use `super::indexed`, which is `Str::unit_at` — constant time
//! for both layouts, and it already existed. The methods that genuinely walk the
//! whole string still take the copy, because they read all of it anyway.
//!
//! # Where the searches went
//!
//! `indexOf` and its four neighbours are in [`super::search`]. This file was 596
//! lines against a 500-line ceiling, and those five are the half that shares a
//! question rather than merely a home.

use super::super::native::Native;
use super::super::with_current;
use super::{absent, answer, answer_owned, arg_units, nothing, relative, text_of, units_of};
use crate::value::Value;

/// What a string's prototype holds, apart from the searches and the patterns.
pub(super) const NATIVES: &[(&str, Native)] = &[
    ("charAt", char_at),
    ("charCodeAt", char_code_at),
    ("at", at),
    ("slice", slice),
    ("substring", substring),
    ("toUpperCase", to_upper_case),
    ("toLowerCase", to_lower_case),
    ("toLocaleUpperCase", to_upper_case),
    ("toLocaleLowerCase", to_lower_case),
    ("repeat", repeat),
    ("concat", concat),
    ("padStart", pad_start),
    ("padEnd", pad_end),
];

/// `s.charAt(i)`.
///
/// Out of range is the **empty string**, where `s[i]` is `undefined`. That
/// difference is the whole reason both spellings exist, and an implementation
/// that answered the same for both would make `s.charAt(9) === ""` false.
extern "C" fn char_at(_e: u64, this: u64, index: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    with_current(|context| {
        let Some(length) = super::length_of(context, this) else {
            return nothing(context);
        };
        let at = super::integer_arg(context, index);
        if at < 0.0 || at >= length as f64 {
            return answer(context, &[]);
        }
        match super::indexed(context, this, at as usize) {
            Some(unit) => answer(context, &[unit]),
            None => answer(context, &[]),
        }
    })
}

/// `s.charCodeAt(i)` — the code unit as a number.
extern "C" fn char_code_at(_e: u64, this: u64, index: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    with_current(|context| {
        let Some(length) = super::length_of(context, this) else {
            return nothing(context);
        };
        let at = super::integer_arg(context, index);
        if at < 0.0 || at >= length as f64 {
            // `NaN`, not `undefined`. A program comparing the result to a number
            // gets false either way; one doing arithmetic with it gets `NaN`
            // where `undefined` would also give `NaN` — but `typeof` tells them
            // apart, and the language says which it is.
            return Value::from_f64(f64::NAN).bits();
        }
        match super::indexed(context, this, at as usize) {
            Some(unit) => Value::from_f64(f64::from(unit)).bits(),
            None => Value::from_f64(f64::NAN).bits(),
        }
    })
}

/// `s.at(i)` — like the index, but negative counts from the end.
extern "C" fn at(_e: u64, this: u64, index: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    with_current(|context| {
        let Some(length) = super::length_of(context, this) else {
            return nothing(context);
        };
        // Truncated BEFORE the end is added, which is the whole of the
        // difference `at(-1.5)` shows: the language reads the argument as `-1`
        // and answers the last character, where adding first and casting after
        // is a floor and answers the one before it.
        let asked = super::integer_arg(context, index);
        let at = if asked < 0.0 {
            length as f64 + asked
        } else {
            asked
        };
        // Out of range is `undefined` here and the empty string for `charAt`.
        // Not a nicety: `at` was added to the language precisely because the
        // older method could not say "there is nothing there".
        if at < 0.0 || at >= length as f64 {
            return nothing(context);
        }
        match super::indexed(context, this, at as usize) {
            Some(unit) => answer(context, &[unit]),
            None => nothing(context),
        }
    })
}

/// `s.slice(from, to)` — negative counts from the end.
extern "C" fn slice(_e: u64, this: u64, from: u64, to: u64, _a2: u64, _a3: u64) -> u64 {
    with_current(|context| {
        // Narrow in and narrow out: the receiver is borrowed rather than copied
        // and widened, and the result is built without re-deciding a layout the
        // caller already knows. `units_of` did both — 512 bytes and 256
        // widenings to hand back sixteen characters.
        //
        // The bounds are computed twice, once per path, rather than hoisted:
        // hoisting needs the length, and asking for the length is what the
        // borrow is for.
        // Through `receiver`, like every other read of `this` in this file: a
        // wrapper object has no text of its own, and reaching for the cell
        // directly is what made the narrow path miss it and the wide path — the
        // one below, which goes through `units_of` — answer for it.
        let held = super::receiver(context, this);
        if let Some(text) = Value(held).as_slot().and_then(|cell| context.text_at(cell))
            && let Some(bytes) = text.narrow()
        {
            let len = bytes.len();
            let start = relative(super::integer_arg(context, from), len);
            let end = match absent(context, to) {
                true => len,
                false => relative(super::integer_arg(context, to), len),
            };
            // Crossed rather than swapped, exactly as the wide path below.
            let taken = match start >= end {
                true => Vec::new(),
                false => bytes[start..end].to_vec(),
            };
            return answer_owned(context, taken);
        }
        let Some(units) = units_of(context, this) else {
            return nothing(context);
        };
        let start = relative(super::integer_arg(context, from), units.len());
        let end = if absent(context, to) {
            units.len()
        } else {
            relative(super::integer_arg(context, to), units.len())
        };
        // Crossed rather than swapped. `"abc".slice(2, 1)` is empty, where
        // `substring` swaps and answers "b" — the one difference between the two
        // methods, and the reason both are here.
        if start >= end {
            return answer(context, &[]);
        }
        answer(context, &units[start..end])
    })
}

/// `s.substring(from, to)` — negative clamps to zero, and the two swap.
extern "C" fn substring(_e: u64, this: u64, from: u64, to: u64, _a2: u64, _a3: u64) -> u64 {
    with_current(|context| {
        let Some(units) = units_of(context, this) else {
            return nothing(context);
        };
        let length = units.len() as f64;
        let clamp = |value: f64| value.clamp(0.0, length) as usize;
        let start = clamp(super::integer_arg(context, from).max(0.0));
        let end = if absent(context, to) {
            units.len()
        } else {
            clamp(super::integer_arg(context, to).max(0.0))
        };
        let (start, end) = if start > end { (end, start) } else { (start, end) };
        answer(context, &units[start..end])
    })
}

/// `s.toUpperCase()`.
///
/// Through Rust's own case mapping, which is the full Unicode one — so `"ß"`
/// uppercases to `"SS"` and the result is longer than the receiver. That is
/// what the language says too, and it is why this converts through text rather
/// than mapping units in place.
extern "C" fn to_upper_case(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    mapped(this, str::to_uppercase, u8::to_ascii_uppercase)
}

/// `s.toLowerCase()`.
///
/// `toLocaleUpperCase` and `toLocaleLowerCase` are installed as these same two
/// functions rather than as wrappers. They differ from them in exactly three
/// locales — Turkish, Azeri and Lithuanian dotted `i` — and this crate carries
/// no locale data, so a separate body would be the same mapping under a second
/// name and one more place for the two to drift. The divergence is the locale
/// data, not the dispatch.
extern "C" fn to_lower_case(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    mapped(this, str::to_lowercase, u8::to_ascii_lowercase)
}

/// `s.repeat(n)`.
extern "C" fn repeat(_e: u64, this: u64, count: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let asked = with_current(|context| super::integer_arg(context, count));
    // A negative count, or one that never terminates, is a `RangeError` —
    // raising is possible now that rule 8's discipline is in place (`repeat`
    // calls no user code, so there is nothing to check first). Raised OUTSIDE
    // the borrow below: `range_error` takes the context's own `RefCell`
    // borrow to build the error object, and taking it a second time while the
    // first is still held is the nested borrow this crate's whole entry layer
    // is arranged to make unreachable — an `extern "C"` frame cannot unwind
    // the panic that follows, so it aborted the process instead of throwing.
    if asked < 0.0 || asked.is_infinite() {
        super::super::throw::range_error("Invalid count value");
        return with_current(|context| nothing(context));
    }
    with_current(|context| {
        let Some(units) = units_of(context, this) else {
            return nothing(context);
        };
        // An absurdly large but finite count is still bounded rather than
        // thrown for: the specification has no upper bound, but multiplying by
        // a number a program chose would let a script exhaust memory, and that
        // is a stated gap this change does not close.
        if asked > 4096.0 {
            return answer(context, &[]);
        }
        let mut out = Vec::with_capacity(units.len() * asked as usize);
        for _ in 0..asked as usize {
            out.extend_from_slice(&units);
        }
        answer(context, &out)
    })
}

/// `s.concat(t, …)` — however many arguments the call carried.
///
/// # Why this reads the spilled vector and coerces outside the borrow
///
/// Two bugs, one shape. The four slots the convention carries were folded over
/// directly, so `"x".concat("a","b","c","d","e")` answered `"xabcd"` — the fifth
/// argument was never READ, not merely dropped. And each one was converted with
/// `text_of`, which answers nothing for an object, so `"".concat([1, 2])` was
/// `undefined` where the language runs the array's own `toString`.
///
/// Both are fixed by asking the two questions this crate already has answers
/// for: `arguments_at` for what the call carried, and `to_primitive` for what a
/// value's text is — the second OUTSIDE the borrow, because it may run a
/// `toString` a program wrote.
///
/// The receiver goes through the same conversion, which is what makes
/// `String.prototype.concat.call(5, "!")` answer `"5!"`. It is not an object
/// here — `this` for a string method is the primitive — but it may be a number
/// or a boolean when the method is borrowed.
///
/// The divergence, stated: a TRAILING `undefined` argument is dropped rather
/// than becoming `"undefined"`, so `"v=".concat(undefined)` is `"v="`. That is
/// `arguments_at`'s own rule — below five arguments there is no vector, and the
/// convention pads the unused slots with `undefined`, so a native cannot tell
/// padding from something a program wrote.
extern "C" fn concat(_e: u64, this: u64, a0: u64, a1: u64, a2: u64, a3: u64) -> u64 {
    let given =
        with_current(|context| super::super::array_proto::arguments_at(context, 0, [a0, a1, a2, a3]));
    let mut pieces = Vec::with_capacity(given.len() + 1);
    for value in std::iter::once(this).chain(given) {
        pieces.push(super::super::primitive::to_primitive(
            value,
            crate::coerce::Hint::String,
        ));
        // Rule 8: `to_primitive` may have run a `toString` a program wrote, and
        // one that threw answered `undefined` — which is a VALUE, so carrying on
        // would concatenate the word "undefined" into a string the program never
        // receives anyway.
        if super::super::throw::in_flight() {
            return with_current(|context| nothing(context));
        }
    }
    with_current(|context| {
        let mut units = Vec::new();
        for piece in pieces {
            let Some(text) = text_of(context, piece) else {
                return nothing(context);
            };
            units.extend(text.units());
        }
        answer(context, &units)
    })
}

/// `s.padStart(n, fill)`.
extern "C" fn pad_start(_e: u64, this: u64, width: u64, fill: u64, _a2: u64, _a3: u64) -> u64 {
    padded(this, width, fill, true)
}

/// `s.padEnd(n, fill)`.
extern "C" fn pad_end(_e: u64, this: u64, width: u64, fill: u64, _a2: u64, _a3: u64) -> u64 {
    padded(this, width, fill, false)
}

/// Both padding methods, which differ only in which end grows.
fn padded(this: u64, width: u64, fill: u64, at_start: bool) -> u64 {
    with_current(|context| {
        let Some(units) = units_of(context, this) else {
            return nothing(context);
        };
        let wanted = super::integer_arg(context, width);
        if !(0.0..=4096.0).contains(&wanted) || wanted as usize <= units.len() {
            return answer(context, &units);
        }
        let filler = if absent(context, fill) {
            vec![u16::from(b' ')]
        } else {
            match arg_units(context, fill) {
                // An empty fill pads with nothing, which the specification says
                // leaves the string alone rather than looping forever.
                Some(units) if units.is_empty() => return answer(context, &units),
                Some(units) => units,
                None => return nothing(context),
            }
        };
        let missing = wanted as usize - units.len();
        let mut pad = Vec::with_capacity(missing);
        while pad.len() < missing {
            pad.push(filler[pad.len() % filler.len()]);
        }
        let mut out = Vec::with_capacity(wanted as usize);
        if at_start {
            out.extend_from_slice(&pad);
            out.extend_from_slice(&units);
        } else {
            out.extend_from_slice(&units);
            out.extend_from_slice(&pad);
        }
        answer(context, &out)
    })
}

/// A case change, over the valid runs of the receiver.
///
/// # Why not `to_rust()` and one `str::to_uppercase`
///
/// Because `Str::to_rust` answers nothing for a string holding a lone
/// surrogate, so `("a" + halfAnEmoji).toUpperCase()` answered `undefined` —
/// a missing method where the language answers `"A"` and the half-character.
///
/// # Why not one `char` at a time either
///
/// Because case mapping has rules that reach across characters. `"ΑΣ"`
/// lowercases to `"ας"` with a FINAL sigma, decided by what follows the sigma,
/// and `char::to_lowercase` cannot see it. [`crate::text::mapped_runs`] hands
/// each run of valid text to `str`'s own mapping whole, which is both the full
/// Unicode algorithm — `ß` uppercases to `SS`, one character becoming two — and
/// where the surrogate handling is stated once for this and for `normalize`.
///
/// # Why ASCII gets a byte path and the rest of the narrow form does not
///
/// Because a Latin-1 case change can LEAVE the narrow form. `ÿ` uppercases to
/// `Ÿ`, and `µ` to `Μ` — neither fits a byte, so a byte-wise map would silently
/// truncate. Below 128 there is no such case: the only mapping is the
/// twenty-six letters, and it stays below 128.
fn mapped(this: u64, body: fn(&str) -> String, ascii: fn(&u8) -> u8) -> u64 {
    with_current(|context| {
        let held = super::receiver(context, this);
        if let Some(text) = Value(held).as_slot().and_then(|cell| context.text_at(cell))
            && let Some(bytes) = text.narrow()
            && bytes.is_ascii()
        {
            let produced: Vec<u8> = bytes.iter().map(ascii).collect();
            return answer_owned(context, produced);
        }
        let Some(text) = text_of(context, held) else {
            return nothing(context);
        };
        context
            .intern_value(crate::text::mapped_runs(&text, body))
            .bits()
    })
}
