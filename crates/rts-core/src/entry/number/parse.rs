//! Reading a number OUT of text, and the radix argument that says in which base.
//!
//! # Why this is not inside the class
//!
//! `parseInt` and `parseFloat` are **global functions**, and `Number.parseInt`
//! is the *same function object* rather than a second one with the same body —
//! the specification says so and a program can see it, which is what
//! [`super::register_number`] is about. So the bodies live in
//! [`crate::entry::global_fns`], where the global list that owns them is, and
//! the parsing itself lives here where both spellings and
//! `Number.prototype.toString` reach it.

use super::super::objects::undefined_of;
use super::super::with_current;
use crate::value::Value;

/// The radix an argument names, with `None` for "not given".
///
/// # Why an `Option` and not a zero sentinel
///
/// Because zero is a legal thing to write and it does not mean the same as
/// leaving the argument out. `parseInt(s, 0)` means "you decide", while
/// `(5).toString(0)` is a `RangeError`. The sentinel spelling this replaces
/// could not express the second at all: it answered `"5"`, which is a right
/// answer to a question the program did not ask.
///
/// The borrow is taken and given back before `to_number` takes one of its own.
/// Nesting them is a panic on the re-entry, and an `extern "C"` frame cannot
/// unwind — so it aborts the process rather than failing a test, which is how
/// this one was found.
pub(super) fn radix_argument(radix: u64) -> Option<i64> {
    let absent = with_current(|context| undefined_of(context));
    match radix == absent {
        true => None,
        // `ToIntegerOrInfinity`, which is the truncation Rust's saturating cast
        // already performs: `NaN as i64` is `0` and `Infinity as i64` is
        // `i64::MAX`, and both land outside 2..=36 exactly where the
        // specification puts them.
        false => Some(super::super::class_support::to_number(radix) as i64),
    }
}

/// The radix an argument names, with zero meaning "not given".
///
/// [`radix_argument`] with its absence already spent, for the callers where
/// zero and absence genuinely say the same thing: `parseInt(s, 0)` means "you
/// decide", and so does leaving it out. `Number.prototype.toString` is the one
/// where they do not — zero there is a `RangeError` — which is why the `Option`
/// is what the reading answers and this is the narrowing rather than the
/// reverse.
pub(in crate::entry) fn radix_of(radix: u64) -> i64 {
    radix_argument(radix).unwrap_or(0)
}

/// Runs a parse over an argument's text, after `ToString` of it.
///
/// # Why the conversion is the full protocol and not [`super::super::text::to_text`]
///
/// Because `parseInt` is specified over `ToString(argument)`, and an object's
/// `ToString` runs its own `toString` method: `parseInt([7])` is `7` and
/// `parseInt([1, 2])` is `1`, because an array joins to `"7"` and `"1,2"`.
/// Reading the value's text directly answered `NaN` for both — the conversion
/// that never calls anything cannot see a method.
///
/// The conversion runs OUTSIDE the borrow, because it is user code, and rule 8
/// applies to what it leaves behind: a `toString` that threw must not have its
/// `undefined` spent as though it were an answer.
pub(in crate::entry) fn leading(value: u64, mut parse: impl FnMut(&str) -> f64) -> f64 {
    let value = super::super::primitive::to_primitive(value, crate::coerce::Hint::String);
    if super::super::throw::in_flight() {
        return f64::NAN;
    }
    // A SYMBOL has no text form: `ToString(sym)` is a `TypeError`, which is what
    // both parsers inherit. It answered `NaN` — and `NaN` is what these two
    // answer for ordinary unparseable text, so a symbol reaching `parseInt` was
    // indistinguishable from the string `"abc"` reaching it. `Number(sym)`
    // already refused; these did not, which made the three disagree about one
    // value.
    if with_current(|context| super::super::symbol::is_symbol(context, value)) {
        super::super::throw::type_error("Cannot convert a Symbol value to a string");
        return f64::NAN;
    }

    // A primitive ASCII string already is the parser's input. `to_text` below
    // would clone its `Str`, convert it to a Rust `String`, and only then scan
    // the digits. Borrowing the narrow bytes and viewing them as UTF-8 performs
    // no allocation and is exact for ASCII; every non-ASCII or non-string value
    // keeps the complete conversion path below.
    let direct = with_current(|context| {
        let text = context.text_at(Value(value).as_slot()?)?;
        let bytes = text.narrow()?;
        if !bytes.is_ascii() {
            return None;
        }
        let text = std::str::from_utf8(bytes).expect("ASCII bytes are UTF-8");
        Some(parse(text.trim_start()))
    });
    if let Some(parsed) = direct {
        return parsed;
    }

    let text = with_current(|context| {
        super::super::text::to_text(context, Value(value)).and_then(|text| text.to_rust())
    });
    match text {
        // Leading whitespace only. `TrimString(s, start)` is what the
        // specification hands both parsers, and trailing text is what they are
        // defined to stop at rather than to reject.
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
    // `Infinity` is part of the grammar, not a name — `StrDecimalLiteral` lists
    // it beside the digits, which is why `parseFloat(" Infinity ")` is infinite
    // and not `NaN`. Only this spelling, and only capitalised: the language does
    // not accept `inf`, which Rust's own float parser does, so the prefix has to
    // be recognised HERE rather than left to `parse` further down.
    if text[at..].starts_with("Infinity") {
        return at + "Infinity".len();
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

/// The float a text starts with, or `NaN` when it starts with none.
///
/// One function rather than the four lines written twice, because `parseFloat`
/// and `Number.parseFloat` are the same function object and the shared body is
/// what makes that more than a claim.
pub(in crate::entry) fn float_of(text: &str) -> f64 {
    match float_prefix(text) {
        0 => f64::NAN,
        end => text[..end].parse().unwrap_or(f64::NAN),
    }
}

/// The integer a text starts with, in the radix the language picks.
///
/// A radix of zero means "not given", which is the one case where the text
/// decides: `0x` selects sixteen and everything else is ten. Written as a
/// sentinel rather than an `Option` because `parseInt(s, 0)` means exactly the
/// same thing — which is what separates this argument from
/// [`radix_argument`]'s, where zero is a value the language refuses.
pub(in crate::entry) fn integer_prefix(text: &str, radix: i64) -> f64 {
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
