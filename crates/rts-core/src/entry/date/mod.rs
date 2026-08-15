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
//! answers the same number, and [`Date::get_timezone_offset`] answers `0` —
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
mod fields;
mod parse;
mod support;

// The clock, reachable from `intl`. This module's own header promises that a
// wasm target owes a host answer to EXACTLY ONE function; a second copy of
// these four lines beside `Intl.DateTimeFormat` would have made that sentence
// false the day someone read it.
pub(in crate::entry) use support::now_ms;

use civil::{
    clip, date_string, iso_text, locale_string, locale_time_string, time_string, utc_string,
};
use parse::parse_iso;
use support::{
    absent, commit, field, null_of, receiver, store, text_value, time_of,
};

use super::objects::{read_property, undefined_of};
use super::{Context, with_current};
use crate::value::Value;

/// The property the time value lives in. See the module documentation.
pub(in crate::entry) const TIME: &str = "__dateValue";

/// `Date`.
#[rtse::class("Date")]
impl Date {
    /// `new Date()`, `new Date(ms)`, `new Date(string)`, and the seven-field
    /// `new Date(y, m, d, h, min, s, ms)`.
    ///
    /// # Where the seven fields arrive
    ///
    /// Not in the parameter list: a compiled call carries a receiver and four
    /// slots, so three of the seven have nowhere to be declared. They arrive in
    /// the argument vector instead, which [`fields::given`] reads — see that
    /// module for why this was two fields for as long as it was.
    ///
    /// # `Date()` without `new`
    ///
    /// The specification answers a *string* there — a different type from what
    /// `new` answers, decided by how the function was called. The wrapper does
    /// not tell a body which of the two happened, so this answers a `Date` object
    /// either way, making the receiver it was not given the way
    /// [`super::error`]'s `written` does. Stated rather than guessed at: the
    /// information genuinely is not here to branch on.
    #[construct]
    fn build(this: u64, a: u64, b: u64, c: u64, d: u64) -> u64 {
        let given = fields::given([a, b, c, d]);
        let ms = match given.len() {
            0 => now_ms(),
            1 => {
                let only = given[0];
                // Read inside the borrow because `to_text` is a plain function.
                // Every coercion below is not, and runs after it is given back.
                let text = with_current(|context| {
                    super::text::to_text(context, Value(only)).and_then(|text| text.to_rust())
                });
                match Value(only).as_f64() {
                    Some(number) => clip(number),
                    // Parsed first, converted second. The other order loses every
                    // date literal there is, because `ToNumber("2020-01-01")` is
                    // `NaN` — while this order still handles `new Date(true)`,
                    // whose text does not parse and whose conversion is 1.
                    None => match text.as_deref().map(parse_iso) {
                        Some(parsed) if !parsed.is_nan() => parsed,
                        _ => clip(super::class_support::to_number(only)),
                    },
                }
            }
            _ => fields::constructed(&given),
        };

        with_current(|context| {
            let Some(cell) = receiver(context, this) else {
                return undefined_of(context);
            };
            store(context, cell, ms);
            Value::from_slot(cell).bits()
        })
    }

    /// `Date.now()` — milliseconds since the epoch, whole ones.
    #[stat]
    fn now() -> f64 {
        now_ms()
    }

    /// `Date.parse(text)` — the same parser `new Date(string)` runs.
    #[stat]
    fn parse(text: u64) -> f64 {
        let read = with_current(|context| {
            super::text::to_text(context, Value(text)).and_then(|text| text.to_rust())
        });
        read.as_deref().map_or(f64::NAN, parse_iso)
    }

    /// `Date.UTC(y, m, d, h, min, s, ms)` — the same arithmetic the constructor
    /// runs, because everything here is UTC and the module documentation says
    /// why.
    ///
    /// `Date.UTC()` with no arguments is `NaN` and not the epoch: the
    /// specification reads a missing year as `NaN`, and the constructor's
    /// "no arguments means now" is the constructor's rule alone.
    #[stat]
    #[js("UTC")]
    fn utc(a: u64, b: u64, c: u64, d: u64) -> f64 {
        let given = fields::given([a, b, c, d]);
        match given.is_empty() {
            true => f64::NAN,
            false => fields::constructed(&given),
        }
    }

    /// `date.getTime()`.
    fn get_time(this: u64) -> f64 {
        time_of(this)
    }

    /// `date.valueOf()` — the same number, which is what makes `end - start`
    /// answer a duration through the ordinary `ToPrimitive` path rather than
    /// needing `Date` to be known to the operator.
    fn value_of(this: u64) -> f64 {
        time_of(this)
    }

    /// `date.setTime(ms)`, answering the value it stored.
    fn set_time(this: u64, ms: f64) -> f64 {
        commit(this, clip(ms))
    }

    /// `date.setFullYear(year, month?, date?)`.
    ///
    /// An invalid receiver does not refuse the call — the specification reads
    /// `t` as `+0` first — so `new Date(NaN).setFullYear(2020)` answers a real
    /// date rather than propagating the `NaN` forever, which is the one place
    /// this method's behaviour surprises next to every getter in this file.
    /// A field the caller left out keeps the value it had, which is
    /// [`fields::written`]'s rule and the reason all fourteen setters here are
    /// two lines.
    #[js("setFullYear")]
    fn set_full_year(this: u64, a: u64, b: u64, c: u64) -> f64 {
        fields::written(this, 0, 3, &fields::given([a, b, c, absent()]))
    }

    /// `date.setUTCFullYear(year, month?, date?)` — the same number, because
    /// local time here *is* UTC. See the module documentation; the day the two
    /// differ, this pair is where the difference is written.
    #[js("setUTCFullYear")]
    fn set_utc_full_year(this: u64, a: u64, b: u64, c: u64) -> f64 {
        fields::written(this, 0, 3, &fields::given([a, b, c, absent()]))
    }

    /// `date.setMonth(month, date?)` — zero-based, as `getMonth` answers it.
    #[js("setMonth")]
    fn set_month(this: u64, a: u64, b: u64) -> f64 {
        let none = absent();
        fields::written(this, 1, 2, &fields::given([a, b, none, none]))
    }

    /// `date.setUTCMonth(month, date?)`.
    #[js("setUTCMonth")]
    fn set_utc_month(this: u64, a: u64, b: u64) -> f64 {
        let none = absent();
        fields::written(this, 1, 2, &fields::given([a, b, none, none]))
    }

    /// `date.setDate(date)` — the day of the month, one-based.
    #[js("setDate")]
    fn set_date(this: u64, a: u64) -> f64 {
        let none = absent();
        fields::written(this, 2, 1, &fields::given([a, none, none, none]))
    }

    /// `date.setUTCDate(date)`.
    #[js("setUTCDate")]
    fn set_utc_date(this: u64, a: u64) -> f64 {
        let none = absent();
        fields::written(this, 2, 1, &fields::given([a, none, none, none]))
    }

    /// `date.setHours(hour, minute?, second?, ms?)` — all four, where this
    /// stopped at three because a fourth had no slot to arrive in.
    #[js("setHours")]
    fn set_hours(this: u64, a: u64, b: u64, c: u64, d: u64) -> f64 {
        fields::written(this, 3, 4, &fields::given([a, b, c, d]))
    }

    /// `date.setUTCHours(hour, minute?, second?, ms?)`.
    #[js("setUTCHours")]
    fn set_utc_hours(this: u64, a: u64, b: u64, c: u64, d: u64) -> f64 {
        fields::written(this, 3, 4, &fields::given([a, b, c, d]))
    }

    /// `date.setMinutes(minute, second?, ms?)`.
    #[js("setMinutes")]
    fn set_minutes(this: u64, a: u64, b: u64, c: u64) -> f64 {
        fields::written(this, 4, 3, &fields::given([a, b, c, absent()]))
    }

    /// `date.setUTCMinutes(minute, second?, ms?)`.
    #[js("setUTCMinutes")]
    fn set_utc_minutes(this: u64, a: u64, b: u64, c: u64) -> f64 {
        fields::written(this, 4, 3, &fields::given([a, b, c, absent()]))
    }

    /// `date.setSeconds(second, ms?)`.
    #[js("setSeconds")]
    fn set_seconds(this: u64, a: u64, b: u64) -> f64 {
        let none = absent();
        fields::written(this, 5, 2, &fields::given([a, b, none, none]))
    }

    /// `date.setUTCSeconds(second, ms?)`.
    #[js("setUTCSeconds")]
    fn set_utc_seconds(this: u64, a: u64, b: u64) -> f64 {
        let none = absent();
        fields::written(this, 5, 2, &fields::given([a, b, none, none]))
    }

    /// `date.setMilliseconds(ms)`.
    #[js("setMilliseconds")]
    fn set_milliseconds(this: u64, a: u64) -> f64 {
        let none = absent();
        fields::written(this, 6, 1, &fields::given([a, none, none, none]))
    }

    /// `date.setUTCMilliseconds(ms)`.
    #[js("setUTCMilliseconds")]
    fn set_utc_milliseconds(this: u64, a: u64) -> f64 {
        let none = absent();
        fields::written(this, 6, 1, &fields::given([a, none, none, none]))
    }

    /// `date.getFullYear()`.
    fn get_full_year(this: u64) -> f64 {
        field(this, |parts| parts.year as f64)
    }

    /// `date.getUTCFullYear()` — the same number. See the module documentation.
    #[js("getUTCFullYear")]
    fn get_utc_full_year(this: u64) -> f64 {
        field(this, |parts| parts.year as f64)
    }

    /// `date.getMonth()` — zero-based, as the language spells it.
    fn get_month(this: u64) -> f64 {
        field(this, |parts| parts.month as f64)
    }

    /// `date.getUTCMonth()`.
    #[js("getUTCMonth")]
    fn get_utc_month(this: u64) -> f64 {
        field(this, |parts| parts.month as f64)
    }

    /// `date.getDate()` — the day of the month, one-based.
    fn get_date(this: u64) -> f64 {
        field(this, |parts| parts.day as f64)
    }

    /// `date.getUTCDate()`.
    #[js("getUTCDate")]
    fn get_utc_date(this: u64) -> f64 {
        field(this, |parts| parts.day as f64)
    }

    /// `date.getDay()` — the day of the week, Sunday zero.
    fn get_day(this: u64) -> f64 {
        field(this, |parts| parts.weekday as f64)
    }

    /// `date.getUTCDay()`.
    #[js("getUTCDay")]
    fn get_utc_day(this: u64) -> f64 {
        field(this, |parts| parts.weekday as f64)
    }

    /// `date.getHours()`.
    fn get_hours(this: u64) -> f64 {
        field(this, |parts| parts.hour as f64)
    }

    /// `date.getUTCHours()`.
    #[js("getUTCHours")]
    fn get_utc_hours(this: u64) -> f64 {
        field(this, |parts| parts.hour as f64)
    }

    /// `date.getMinutes()`.
    fn get_minutes(this: u64) -> f64 {
        field(this, |parts| parts.minute as f64)
    }

    /// `date.getUTCMinutes()`.
    #[js("getUTCMinutes")]
    fn get_utc_minutes(this: u64) -> f64 {
        field(this, |parts| parts.minute as f64)
    }

    /// `date.getSeconds()`.
    fn get_seconds(this: u64) -> f64 {
        field(this, |parts| parts.second as f64)
    }

    /// `date.getUTCSeconds()`.
    #[js("getUTCSeconds")]
    fn get_utc_seconds(this: u64) -> f64 {
        field(this, |parts| parts.second as f64)
    }

    /// `date.getMilliseconds()`.
    fn get_milliseconds(this: u64) -> f64 {
        field(this, |parts| parts.milli as f64)
    }

    /// `date.getUTCMilliseconds()`.
    #[js("getUTCMilliseconds")]
    fn get_utc_milliseconds(this: u64) -> f64 {
        field(this, |parts| parts.milli as f64)
    }

    /// `date.getTimezoneOffset()` — zero, because local time here *is* UTC.
    ///
    /// `NaN` for an invalid date, which the language keeps and which matters:
    /// the offset of a date that does not exist is not zero minutes.
    fn get_timezone_offset(this: u64) -> f64 {
        match time_of(this).is_nan() {
            true => f64::NAN,
            false => 0.0,
        }
    }

    /// `date.toISOString()`.
    ///
    /// An invalid date answers the text `"Invalid Date"` where the specification
    /// throws a `RangeError`. Not a preference: raising from a native still ends
    /// the program rather than reaching a `catch`, which [`super::error`]
    /// records, so throwing here would turn a formatting mistake into a process
    /// exit. It comes back the day `try` around a call does.
    #[js("toISOString")]
    fn to_iso(this: u64) -> u64 {
        text_value(iso_text(time_of(this)))
    }

    /// `date.toJSON()` — the ISO text, or `null` for an invalid date.
    ///
    /// `null` rather than the `"Invalid Date"` text above, because this one is
    /// read by a serialiser rather than by a person, and the language guarantees
    /// an unrepresentable date serialises as `null`.
    #[js("toJSON")]
    fn to_json(this: u64) -> u64 {
        let time = time_of(this);
        match time.is_nan() {
            true => with_current(|context| null_of(context)),
            false => text_value(iso_text(time)),
        }
    }

    /// `date.toString()` — the ISO text.
    ///
    /// A real engine answers `"Wed Jan 01 2020 00:00:00 GMT+0000 (…)"`, whose
    /// last field is a timezone name this runtime does not have. ISO-8601 is
    /// chosen over inventing one because it round-trips through `Date.parse`,
    /// which the human-readable form is not required to.
    fn to_string(this: u64) -> u64 {
        text_value(iso_text(time_of(this)))
    }

    /// `date.toUTCString()` — the RFC 7231 form, e.g.
    /// `"Thu, 01 Jan 1970 00:00:00 GMT"`. Node and every other engine answer
    /// exactly this text for a UTC date, which is what every date here is —
    /// see the module documentation.
    #[js("toUTCString")]
    fn to_utc_string(this: u64) -> u64 {
        text_value(utc_string(time_of(this)))
    }

    /// `date.toDateString()` — `"Thu Jan 01 1970"`.
    #[js("toDateString")]
    fn to_date_string(this: u64) -> u64 {
        text_value(date_string(time_of(this)))
    }

    /// `date.toTimeString()` — `"00:00:00.000Z"`. See [`civil::time_string`]
    /// for why this is not the `GMT+offset (name)` form a real engine answers.
    #[js("toTimeString")]
    fn to_time_string(this: u64) -> u64 {
        text_value(time_string(time_of(this)))
    }

    /// `date.toLocaleDateString()` — the same text as `toDateString`, absent a
    /// locale database to format against. See `civil::locale_string`.
    #[js("toLocaleDateString")]
    fn to_locale_date_string(this: u64) -> u64 {
        text_value(date_string(time_of(this)))
    }

    /// `date.toLocaleTimeString()` — `"00:00:00"`, with no locale database.
    #[js("toLocaleTimeString")]
    fn to_locale_time_string(this: u64) -> u64 {
        text_value(locale_time_string(time_of(this)))
    }

    /// `date.toLocaleString()` — `"01/01/1970, 00:00:00"`.
    #[js("toLocaleString")]
    fn to_locale_string(this: u64) -> u64 {
        text_value(locale_string(time_of(this)))
    }
}
