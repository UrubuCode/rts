//! The functions the global object holds, which belong to no class.
//!
//! # Why these are not a namespace
//!
//! `Math` is an object with methods on it. `parseInt` is a **function that is
//! itself the global's property**, so there is nothing for `#[rtse::class]` to
//! install onto — the attribute makes an object and hangs members on it, and
//! here the member IS the value the name reads.
//!
//! So this is a list of names and the natives behind them, and [`super::global`]
//! makes a callable the first time one is read — the same lazily-made,
//! recorded-as-a-property shape every other provided name has, which is what
//! makes `parseInt === parseInt` true and `globalThis.parseInt` reach it.
//!
//! # Why `isNaN` and `Number.isNaN` are different functions
//!
//! Because the language says so, and the difference is the whole reason the
//! second exists. `isNaN("abc")` converts first and is **true**;
//! `Number.isNaN("abc")` does not convert and is **false**. Implementing one in
//! terms of the other is the mistake that makes the strict spelling useless.
//!
//! # What is deliberately absent
//!
//! `eval`. It needs a compiler at run time, which is a capability rather than a
//! function.

use super::native::Native;
use crate::value::Value;

/// The natives the global object holds, by the name a program reads.
///
/// `None` for anything else, which is what lets [`super::global`] ask one
/// question of this module rather than carry an arm per function.
pub(super) fn provided(name: &str) -> Option<Native> {
    Some(match name {
        "parseInt" => parse_int,
        "parseFloat" => parse_float,
        "isNaN" => is_nan,
        "isFinite" => is_finite,
        _ => return None,
    })
}

/// `parseInt(s, radix)`, and `Number.parseInt` — one function object, not two.
///
/// The specification says they are the same one, and a program can see it. So
/// this is the only body, and [`super::number::register_number`] hangs the cell
/// made from it on the constructor as well. Two copies is where one of them
/// would come to read a leading sign differently.
extern "C" fn parse_int(_e: u64, _this: u64, value: u64, radix: u64, _a2: u64, _a3: u64) -> u64 {
    // Zero for "not given", which is what `integer_prefix` means by it and what
    // `parseInt(s, 0)` means too — the one argument where the sentinel and the
    // value genuinely say the same thing.
    let base = super::number::radix_of(radix);
    let parsed = super::number::leading(value, move |text| {
        super::number::integer_prefix(text, base)
    });
    Value::from_f64(parsed).bits()
}

/// `parseFloat(s)`, and `Number.parseFloat` — see [`parse_int`].
extern "C" fn parse_float(_e: u64, _this: u64, value: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let parsed = super::number::leading(value, super::number::float_of);
    Value::from_f64(parsed).bits()
}

/// `isNaN(x)` — after `ToNumber`, which is what separates it from
/// `Number.isNaN`.
extern "C" fn is_nan(_e: u64, _this: u64, value: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    Value::from_bool(super::class_support::to_number(value).is_nan()).bits()
}

/// `isFinite(x)` — after `ToNumber`, for the same reason.
extern "C" fn is_finite(_e: u64, _this: u64, value: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    Value::from_bool(super::class_support::to_number(value).is_finite()).bits()
}
