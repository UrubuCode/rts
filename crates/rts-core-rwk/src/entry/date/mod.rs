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
//! not built here yet; when it is, exactly one function — [`now_ms`] — needs a
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
mod parse;

use civil::{
    Parts, clip, date_string, from_parts, from_year_month, iso_text, locale_string,
    locale_time_string, parts_of, time_string, utc_string,
};
use parse::parse_iso;

use super::objects::{read_property, undefined_of};
use super::{Context, with_current};
use crate::text::Str;
use crate::value::Value;

/// The property the time value lives in. See the module documentation.
pub(in crate::entry) const TIME: &str = "__dateValue";

/// `Date`.
#[rtse::class("Date")]
impl Date {
    /// `new Date()`, `new Date(ms)`, `new Date(string)`, `new Date(year, month)`.
    ///
    /// # What is dropped, and where the limit is paid
    ///
    /// A call carries a receiver and four arguments, so the seven-argument form
    /// `new Date(y, m, d, h, min, s, ms)` **cannot be declared** — three of its
    /// parameters have nowhere to arrive. Rather than support a lopsided four of
    /// the seven, which would answer a wrong date for the other three with no
    /// sign that anything was ignored, this stops at `(year, month)` and builds
    /// the first of that month at midnight. A program wanting a day or an hour
    /// adds milliseconds to `Date.UTC(y, m)`, which is exact. The fix is an
    /// argument vector, not a wider signature.
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
    fn build(this: u64, a: u64, b: u64) -> u64 {
        let (absent, text) = with_current(|context| {
            let absent = undefined_of(context);
            let text = match a == absent {
                true => None,
                // Read inside the borrow because `to_text` is a plain function.
                // Every coercion below is not, and runs after it is given back.
                false => super::text::to_text(context, Value(a)).and_then(|text| text.to_rust()),
            };
            (absent, text)
        });

        let ms = if a == absent {
            now_ms()
        } else if b == absent {
            match Value(a).as_f64() {
                Some(number) => clip(number),
                // Parsed first, converted second. The other order loses every
                // date literal there is, because `ToNumber("2020-01-01")` is
                // `NaN` — while this order still handles `new Date(true)`, whose
                // text does not parse and whose conversion is 1.
                None => match text.as_deref().map(parse_iso) {
                    Some(parsed) if !parsed.is_nan() => parsed,
                    _ => clip(super::class_support::to_number(a)),
                },
            }
        } else {
            let year = super::class_support::to_number(a);
            let month = super::class_support::to_number(b);
            from_year_month(year, month)
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

    /// `Date.UTC(year, month)` — the same two-argument limit as the constructor.
    ///
    /// Kept rather than dropped because it is the only way left to name an
    /// instant arithmetically once the constructor stops at a month, and it is
    /// identical to the constructor here precisely because everything is UTC.
    #[stat]
    #[js("UTC")]
    fn utc(year: f64, month: f64) -> f64 {
        from_year_month(year, month)
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
    /// `month` and `date` fall back to the CURRENT date's fields when the
    /// caller left them out, which is why they are asked for by comparing
    /// against `absent` rather than by declaring `f64` and letting a missing
    /// argument coerce to `NaN` — that would silently roll the date to an
    /// invalid one instead of keeping the field unchanged.
    #[js("setFullYear")]
    fn set_full_year(this: u64, year: u64, month: u64, date: u64) -> f64 {
        let ms = time_of(this);
        let existing = parts_of(ms).unwrap_or(parts_of(0.0).expect("epoch has parts"));
        let absent = with_current(|context| undefined_of(context));

        let year = super::class_support::to_number(year);
        let month = match month == absent {
            true => existing.month as f64,
            false => super::class_support::to_number(month),
        };
        let date = match date == absent {
            true => existing.day as f64,
            false => super::class_support::to_number(date),
        };

        let stored = if !year.is_finite() || !month.is_finite() || !date.is_finite() {
            f64::NAN
        } else {
            let time_within_day = if ms.is_nan() {
                0.0
            } else {
                ms - (ms / civil::MS_PER_DAY).floor() * civil::MS_PER_DAY
            };
            let months = year.trunc() as i64 * 12 + month.trunc() as i64;
            let days = civil::days_from_civil(
                months.div_euclid(12),
                months.rem_euclid(12) + 1,
                date.trunc() as i64,
            );
            clip(days as f64 * civil::MS_PER_DAY + time_within_day)
        };

        with_current(|context| {
            if let Some(cell) = Value(this).as_slot() {
                store(context, cell, stored);
            }
        });
        stored
    }

    /// `date.setMonth(month, date?)` — zero-based, as `getMonth` answers it.
    ///
    /// `date` falls back to the current day of the month exactly as
    /// `setFullYear`'s does, for the same reason: a caller passing one
    /// argument must not have the other roll to the epoch's.
    #[js("setMonth")]
    fn set_month(this: u64, month: u64, date: u64) -> f64 {
        let existing = fields_of(this);
        let absent = with_current(|context| undefined_of(context));
        let month = super::class_support::to_number(month);
        let date = optional(date, absent, existing.day as f64);
        commit(this, from_parts(
            existing.year as f64, month, date,
            existing.hour as f64, existing.minute as f64, existing.second as f64, existing.milli as f64,
        ))
    }

    /// `date.setDate(date)` — the day of the month, one-based.
    #[js("setDate")]
    fn set_date(this: u64, date: u64) -> f64 {
        let existing = fields_of(this);
        let date = super::class_support::to_number(date);
        commit(this, from_parts(
            existing.year as f64, existing.month as f64, date,
            existing.hour as f64, existing.minute as f64, existing.second as f64, existing.milli as f64,
        ))
    }

    /// `date.setHours(hour, minute?, second?)`.
    ///
    /// Stops at three of the specification's four trailing arguments for the
    /// reason the constructor stops at two: a fourth has no slot to arrive in.
    #[js("setHours")]
    fn set_hours(this: u64, hour: u64, minute: u64, second: u64) -> f64 {
        let existing = fields_of(this);
        let absent = with_current(|context| undefined_of(context));
        let hour = super::class_support::to_number(hour);
        let minute = optional(minute, absent, existing.minute as f64);
        let second = optional(second, absent, existing.second as f64);
        commit(this, from_parts(
            existing.year as f64, existing.month as f64, existing.day as f64,
            hour, minute, second, existing.milli as f64,
        ))
    }

    /// `date.setMinutes(minute, second?)`.
    #[js("setMinutes")]
    fn set_minutes(this: u64, minute: u64, second: u64) -> f64 {
        let existing = fields_of(this);
        let absent = with_current(|context| undefined_of(context));
        let minute = super::class_support::to_number(minute);
        let second = optional(second, absent, existing.second as f64);
        commit(this, from_parts(
            existing.year as f64, existing.month as f64, existing.day as f64,
            existing.hour as f64, minute, second, existing.milli as f64,
        ))
    }

    /// `date.setSeconds(second, ms?)`.
    #[js("setSeconds")]
    fn set_seconds(this: u64, second: u64, milli: u64) -> f64 {
        let existing = fields_of(this);
        let absent = with_current(|context| undefined_of(context));
        let second = super::class_support::to_number(second);
        let milli = optional(milli, absent, existing.milli as f64);
        commit(this, from_parts(
            existing.year as f64, existing.month as f64, existing.day as f64,
            existing.hour as f64, existing.minute as f64, second, milli,
        ))
    }

    /// `date.setMilliseconds(ms)`.
    #[js("setMilliseconds")]
    fn set_milliseconds(this: u64, milli: u64) -> f64 {
        let existing = fields_of(this);
        let milli = super::class_support::to_number(milli);
        commit(this, from_parts(
            existing.year as f64, existing.month as f64, existing.day as f64,
            existing.hour as f64, existing.minute as f64, existing.second as f64, milli,
        ))
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

/// The clock. The one line a wasm target has to supply — see the module doc.
fn now_ms() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(f64::NAN, |since| since.as_millis() as f64)
}

/// The object to write onto: the one `new` made, or one made here.
fn receiver(context: &mut Context, this: u64) -> Option<u32> {
    if let Some(cell) = Value(this).as_slot() {
        return Some(cell);
    }
    let cell = super::native::plain(context)?;
    if let Some(prototype) = super::class_support::prototype(context, "Date") {
        context.set_prototype(cell, prototype);
    }
    Some(cell)
}

/// Writes the time value onto the object.
fn store(context: &mut Context, cell: u32, ms: f64) {
    let key = context.well_known(TIME);
    super::objects::put(context, cell, key, Value::from_f64(ms).bits());
}

/// The time value a receiver carries, `NaN` for anything carrying none.
///
/// `NaN` rather than a refusal, because every getter already answers `NaN` for
/// an invalid date — so a receiver that is not a date takes a path that exists
/// instead of adding a second kind of failure for callers to distinguish.
fn time_of(this: u64) -> f64 {
    with_current(|context| {
        let Some(cell) = Value(this).as_slot() else {
            return f64::NAN;
        };
        let key = context.well_known(TIME);
        read_property(context, cell, key)
            .and_then(|found| found.as_f64())
            .unwrap_or(f64::NAN)
    })
}

/// One calendar field of a receiver, `NaN` when the date is invalid.
fn field(this: u64, pick: fn(&Parts) -> f64) -> f64 {
    parts_of(time_of(this)).as_ref().map_or(f64::NAN, pick)
}

/// Every field of a receiver, falling back to the epoch's when the date is
/// invalid — the same fallback [`set_full_year`] already makes, so a setter
/// on an invalid date answers a real one instead of propagating `NaN` forever.
fn fields_of(this: u64) -> Parts {
    parts_of(time_of(this)).unwrap_or(parts_of(0.0).expect("epoch has parts"))
}

/// An argument a setter treats as "leave this field alone" when absent.
///
/// The comparison every trailing setter argument needs — `setMonth`'s `date`,
/// `setHours`' `minute` and `second` — pulled out once rather than repeated at
/// each call site, which is how [`set_full_year`] wrote it before there were
/// six more setters making the same comparison.
fn optional(value: u64, absent: u64, current: f64) -> f64 {
    match value == absent {
        true => current,
        false => super::class_support::to_number(value),
    }
}

/// Stores a computed time value onto the receiver and answers it — what every
/// setter in this file does with its result, pulled out once rather than
/// repeated at each call site.
fn commit(this: u64, ms: f64) -> f64 {
    with_current(|context| {
        if let Some(cell) = Value(this).as_slot() {
            store(context, cell, ms);
        }
    });
    ms
}

/// A `String` value holding text that was produced outside any borrow.
fn text_value(text: String) -> u64 {
    with_current(|context| context.intern_value(Str::from_str(&text)).bits())
}

/// `null`, spelled here as several modules in this folder spell it, for want of
/// one place low enough to hold it that knows the singleton numbering.
fn null_of(context: &Context) -> u64 {
    rts_cranelift::tags::encode(
        rts_cranelift::tags::TAG_SINGLETON,
        u64::from(context.singletons.null),
    )
}
