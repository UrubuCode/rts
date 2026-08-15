//! `Date` — a time value in milliseconds, and the class over it.
//!
//! # Where the clock comes from, and what a wasm target will owe
//!
//! [`std::time::SystemTime::now`] is called here, directly. The alternative
//! considered was a `now` hook the host installs, defaulting to a clock that
//! answers zero — and it was rejected because the default is the whole problem:
//! a host that forgot to install it would produce a `Date` that *works*, dates
//! that format, arithmetic that subtracts, and every one of them in 1970. A
//! wrong answer that looks right is the failure mode this crate spends its
//! documentation avoiding, and a link error is the honest version of it.
//!
//! What that costs is stated rather than hidden. `SystemTime::now` exists on
//! every target this workspace builds for today and **panics on
//! `wasm32-unknown-unknown`**, where there is no clock behind it. That target is
//! not built here yet; when it is, exactly one function — [`support::now_ms`] — needs a
//! host-supplied answer, and it is one function rather than a class-shaped hole
//! because the calendar, the parser and the formatter beside it are pure
//! arithmetic that runs anywhere. README rule 1 asks "does this exist here?":
//! the calendar does, everywhere, and reading a clock is the single line that
//! does not.
//!
//! # Everything is UTC
//!
//! There is no timezone database in this crate and there will not be one — it is
//! megabytes of data that changes several times a year, and a hand-written
//! approximation of it is a wrong answer that looks right for eleven months out
//! of twelve. So `getHours` **is** `getUTCHours`, every `get*`/`getUTC*` pair
//! answers the same number, and `Date.prototype.getTimezoneOffset` answers `0` —
//! which is the truthful statement that this runtime's local time *is* UTC,
//! rather than a claim to have converted anything.
//!
//! # Why the time value is an ordinary property
//!
//! A real engine keeps it in an internal slot a program cannot see. Here it is a
//! property named `__dateValue`, for the reason [`super::regex`]'s `describe`
//! records: compiled code reading a property that is in the shape's layout never
//! asks the runtime at all, so a value only a side table knows about is one the
//! fast path disagrees with the moment it starts working. The divergence is real
//! and visible — `Object.keys(new Date())` shows it and a real engine's does not
//! — and it is the honest trade until this crate has a slot kind that is present
//! in the layout and absent from enumeration.

mod civil;
mod class;
mod fields;
mod hint;
mod parse;
mod support;

// The clock, reachable from `intl`. This module's own header promises that a
// wasm target owes a host answer to EXACTLY ONE function; a second copy of
// these four lines beside `Intl.DateTimeFormat` would have made that sentence
// false the day someone read it.
pub(in crate::entry) use support::now_ms;

// The declared-type const travels with the registration, as `collections` does
// it: `entry::declared` names one item per class and the class is in `class.rs`.
pub(in crate::entry) use class::DATE_TYPES;

use super::objects::{read_property, undefined_of};
use super::{Context, with_current};
use crate::value::Value;

/// The property the time value lives in. See the module documentation.
pub(in crate::entry) const TIME: &str = "__dateValue";

/// Installs `Date`, and the one member the attribute cannot spell.
///
/// `Date.prototype[Symbol.toPrimitive]` is keyed by a SYMBOL, and
/// `#[rtse::class]` names a member with a string — the same limit
/// `collections::register_map` works around for `Map.prototype[Symbol.iterator]`
/// and for the same reason: teaching the attribute symbol keys buys one line
/// here and a second way to name a member everywhere.
///
/// Idempotent, because it has to be: [`class::register_date`] answers early once
/// `Date` is made, and this still runs afterwards. [`hint::install`] answers
/// early for the same reason and says there why it cannot simply write twice.
pub(in crate::entry) fn register_date(context: &mut Context) -> u64 {
    let made = class::register_date(context);
    if let Some(prototype) = super::class_support::prototype(context, "Date")
        && let Some(cell) = Value(prototype).as_slot()
    {
        hint::install(context, cell);
    }
    made
}
