//! The class itself: every member `Date` declares, and nothing else.
//!
//! # Why the block is not in `mod.rs`
//!
//! `#[rtse::class]` derives the registration function's NAME from the type —
//! `impl Date` gives `register_date` — and `Date.prototype` needs one member the
//! attribute has no spelling for: `Symbol.toPrimitive`, whose key is a symbol
//! rather than a string. So `mod.rs` owns a `register_date` that calls this one
//! and then installs it, which is the shape `collections::register_map` already
//! uses for `Map.prototype[Symbol.iterator]`, and for the same reason.
//!
//! The seam is also what README rule 6 asks for: the member list was two thirds
//! of a file already at the 500-line ceiling, and it is the part a reader opens
//! this folder to find.

use super::civil::{
    clip, date_string, iso_text, locale_string, locale_time_string, time_string, utc_string,
};
use super::fields;
use super::parse::parse_iso;
use super::support::{
    absent, commit, field, null_of, now_ms, receiver, store, text_value, time_of,
};
use super::{undefined_of, with_current};
use crate::value::Value;

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
    /// [`super::super::error`]'s `written` does. Stated rather than guessed at:
    /// the information genuinely is not here to branch on.
    #[construct]
    #[arity(7)]
    fn build(this: u64, a: u64, b: u64, c: u64, d: u64) -> u64 {
        let given = fields::given([a, b, c, d]);
        let ms = match given.len() {
            0 => now_ms(),
            1 => {
                let only = given[0];
                // Read inside the borrow because `to_text` is a plain function.
                // Every coercion below is not, and runs after it is given back.
                let text = with_current(|context| {
                    super::super::text::to_text(context, Value(only)).and_then(|text| text.to_rust())
                });
                match Value(only).as_f64() {
                    Some(number) => clip(number),
                    // Parsed first, converted second. The other order loses every
                    // date literal there is, because `ToNumber("2020-01-01")` is
                    // `NaN` — while this order still handles `new Date(true)`,
                    // whose text does not parse and whose conversion is 1.
                    None => match text.as_deref().map(parse_iso) {
                        Some(parsed) if !parsed.is_nan() => parsed,
                        _ => clip(super::super::class_support::to_number(only)),
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
            super::super::text::to_text(context, Value(text)).and_then(|text| text.to_rust())
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
    #[arity(7)]
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

    /// `date.getYear()` — the year MINUS 1900, and Annex B rather than an
    /// oversight.
    ///
    /// Present because a program that predates `getFullYear` still runs, and
    /// because its absence is the loud kind: `d.getYear()` is a `TypeError` on
    /// a name the language does define, which reads as a broken engine rather
    /// than as a deprecated method. It is normative in Annex B, so an engine
    /// that claims the web has it.
    ///
    /// The offset is the whole method — `getYear` on a date in 2026 answers
    /// `126`, not `26` — and getting that wrong is the bug the method is famous
    /// for. An invalid date answers `NaN`, which falls out of `field` rather
    /// than being special-cased: `NaN - 1900` is `NaN`.
    #[js("getYear")]
    fn get_year(this: u64) -> f64 {
        field(this, |parts| parts.year as f64 - 1900.0)
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

    /// `date.toISOString()`, and a `RangeError` for a date that has no text.
    ///
    /// It answered the string `"Invalid Date"` instead, and the comment here
    /// explained why: raising from a native ended the program rather than
    /// reaching a `catch`, so throwing would have turned a formatting mistake
    /// into a process exit. That is no longer true — `throw::range_error` builds
    /// the program's own `RangeError` and records it for the call site above to
    /// re-raise — and the old answer was visible to a program as a date that
    /// serialised where the language says it must not.
    ///
    /// `toJSON` below does NOT go through this, and that is the specification's
    /// own shape rather than an avoidance: it tests the time value first and
    /// answers `null`, so a non-finite date serialises rather than throwing.
    #[js("toISOString")]
    fn to_iso(this: u64) -> u64 {
        let time = time_of(this);
        if time.is_nan() {
            // Outside every borrow, because building the error constructs one of
            // the program's own objects. The generated wrapper holds none here,
            // which is the whole argument of `#[rtse::class]`'s expansion.
            super::super::throw::range_error("Invalid time value");
            return with_current(|context| undefined_of(context));
        }
        text_value(iso_text(time))
    }

    /// `date.toJSON()` — the ISO text, or `null` for an invalid date.
    ///
    /// `null` rather than the `RangeError` above, because this one is read by a
    /// serialiser rather than by a person, and the language guarantees an
    /// unrepresentable date serialises as `null`.
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

    /// `date.toTimeString()` — `"00:00:00.000Z"`. See [`super::civil::time_string`]
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
