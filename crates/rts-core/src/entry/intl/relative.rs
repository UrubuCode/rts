//! `Intl.RelativeTimeFormat` — "yesterday", "in 3 months", and whatever each
//! language writes instead of them.
//!
//! Every word here is CLDR's, including the ones that are not a number and a
//! unit at all: English writes "yesterday" for -1 day, Spanish writes
//! "anteayer" for -2, and nothing in this file knows either. What this file
//! decides is one thing — which of ICU4X's twenty-four constructors a
//! `(style, unit)` pair names — and that is a mapping between two ARGUMENT
//! spaces rather than a formatting rule.
//!
//! # Why there are twenty-four constructors and not one
//!
//! ICU4X carries the relative-time patterns per unit and per width, and its API
//! makes each pair a separate function so a binary links only the patterns it
//! reaches. A JavaScript program picks both at run time, so this file names all
//! of them. The alternative — one dynamic entry point — does not exist in
//! ICU4X 2.x, and inventing one here would mean holding the pattern data
//! ourselves, which is the thing [`super`] exists to refuse.
//!
//! # Why the formatter is rebuilt per call
//!
//! [`super::plural`] states it in full: an ICU4X formatter is not a value this
//! crate can keep beside a cell, so a formatter keeps its RESOLVED OPTIONS —
//! which `resolvedOptions()` has to answer anyway — and is built again from
//! them.

use icu::experimental::relativetime::options::Numeric;
use icu::experimental::relativetime::{RelativeTimeFormatter, RelativeTimeFormatterOptions};

use super::super::with_current;
use crate::value::Value;

/// `Intl.RelativeTimeFormat`.
#[rtse::class("RelativeTimeFormat")]
impl RelativeTimeFormat {
    /// `new Intl.RelativeTimeFormat(locales, options)`.
    ///
    /// `numeric: "auto"` is what lets a language answer a word instead of a
    /// count, and it is the option programs actually reach for: `format(-1,
    /// "day")` is "1 day ago" under the default and "yesterday" under `auto`.
    /// `style` picks how much of the unit is written.
    ///
    /// The locale is checked here rather than at `format`, against the `second`
    /// unit — every locale ICU4X carries relative-time data for carries all
    /// eight units, so one probe answers for all of them and a program learns
    /// at construction that it asked for a locale the data does not have.
    #[construct]
    fn build(this: u64, locales: u64, options: u64) -> u64 {
        let tags = super::requested(locales);
        let selected = super::selected(&tags);
        let style = super::option(options, "style").unwrap_or_else(|| "long".to_owned());
        let numeric = super::option(options, "numeric").unwrap_or_else(|| "always".to_owned());
        let locale_text = selected.to_string();
        if formatter(&selected, &style, "second", &numeric).is_none() {
            super::super::throw::range_error(&format!(
                "Intl.RelativeTimeFormat: no relative time data for {locale_text}"
            ));
            return with_current(|context| super::super::objects::undefined_of(context));
        }
        with_current(|context| {
            let mut written = |text: &str| {
                context
                    .intern_value(crate::text::Str::from_str(text))
                    .bits()
            };
            let locale_value = written(&locale_text);
            let style_value = written(&style);
            let numeric_value = written(&numeric);
            super::resolved(
                context,
                this,
                "RelativeTimeFormat",
                &[
                    ("locale", locale_value),
                    ("style", style_value),
                    ("numeric", numeric_value),
                ],
            )
        })
    }

    /// `rtf.format(value, unit)` — the phrase for that many of that unit.
    ///
    /// An ordinary method, unlike `NumberFormat`'s and `DateTimeFormat`'s
    /// `format`: the specification makes those two accessors answering a bound
    /// function and this one a plain method, and the difference is observable —
    /// `rtf.format` twice is the same function object here and two different
    /// ones there.
    fn format(this: u64, value: f64, unit: u64) -> u64 {
        let Some(unit) = text_of(unit) else {
            super::super::throw::range_error(
                "Intl.RelativeTimeFormat.prototype.format: the unit must be a string",
            );
            return with_current(|context| super::super::objects::undefined_of(context));
        };
        let locale = super::locale::parse(super::stored_text(this, "locale").as_deref());
        let style = super::stored_text(this, "style").unwrap_or_default();
        let numeric = super::stored_text(this, "numeric").unwrap_or_default();
        let Some(formatter) = formatter(&locale, &style, &singular(&unit), &numeric) else {
            super::super::throw::range_error(&format!(
                "Intl.RelativeTimeFormat.prototype.format: {unit} is not a relative time unit"
            ));
            return with_current(|context| super::super::objects::undefined_of(context));
        };
        let written = formatter.format(super::decimal_of(value)).to_string();
        with_current(|context| {
            context
                .intern_value(crate::text::Str::from_str(&written))
                .bits()
        })
    }

    /// `rtf.resolvedOptions()`.
    #[js("resolvedOptions")]
    fn resolved_options(this: u64) -> u64 {
        super::stored(this)
    }
}

/// The unit a caller wrote, in the one spelling this file matches on.
///
/// The language accepts `"day"` and `"days"` alike, so the plural is stripped
/// here rather than doubling every arm below. Not a general de-pluralisation:
/// the eight unit names are ASCII and all form their plural with a trailing
/// `s`, which is a fact about this argument's grammar and not about English.
fn singular(unit: &str) -> String {
    unit.strip_suffix('s').unwrap_or(unit).to_owned()
}

/// A value's text, when it genuinely is a string.
///
/// The same limit [`super::collate`]'s reader has and for the same reason: the
/// specification's `ToString` calls user code, and that conversion belongs at
/// the call site rather than inside a borrow.
fn text_of(value: u64) -> Option<String> {
    with_current(|context| {
        let cell = Value(value).as_slot()?;
        context.text_at(cell)?.to_rust()
    })
}

/// The formatter for a style and a unit, or `None` when the pair names nothing.
///
/// `None` covers two different failures on purpose — a locale with no data and
/// a unit that is not a unit — because the caller can tell them apart and this
/// cannot: the constructor probes with a unit it knows, so a `None` there is
/// the locale, and a `None` at `format` is the unit.
fn formatter(
    locale: &icu::locale::Locale,
    style: &str,
    unit: &str,
    numeric: &str,
) -> Option<RelativeTimeFormatter> {
    let mut options = RelativeTimeFormatterOptions::default();
    if numeric == "auto" {
        options.numeric = Numeric::Auto;
    }
    let prefs = locale.into();
    // `long` for anything else, which is the specification's own default and
    // what an unrecognised `style` falls back to.
    let width = match style {
        "short" => "short",
        "narrow" => "narrow",
        _ => "long",
    };
    let made = match (width, unit) {
        ("short", "second") => RelativeTimeFormatter::try_new_short_second(prefs, options),
        ("short", "minute") => RelativeTimeFormatter::try_new_short_minute(prefs, options),
        ("short", "hour") => RelativeTimeFormatter::try_new_short_hour(prefs, options),
        ("short", "day") => RelativeTimeFormatter::try_new_short_day(prefs, options),
        ("short", "week") => RelativeTimeFormatter::try_new_short_week(prefs, options),
        ("short", "month") => RelativeTimeFormatter::try_new_short_month(prefs, options),
        ("short", "quarter") => RelativeTimeFormatter::try_new_short_quarter(prefs, options),
        ("short", "year") => RelativeTimeFormatter::try_new_short_year(prefs, options),
        ("narrow", "second") => RelativeTimeFormatter::try_new_narrow_second(prefs, options),
        ("narrow", "minute") => RelativeTimeFormatter::try_new_narrow_minute(prefs, options),
        ("narrow", "hour") => RelativeTimeFormatter::try_new_narrow_hour(prefs, options),
        ("narrow", "day") => RelativeTimeFormatter::try_new_narrow_day(prefs, options),
        ("narrow", "week") => RelativeTimeFormatter::try_new_narrow_week(prefs, options),
        ("narrow", "month") => RelativeTimeFormatter::try_new_narrow_month(prefs, options),
        ("narrow", "quarter") => RelativeTimeFormatter::try_new_narrow_quarter(prefs, options),
        ("narrow", "year") => RelativeTimeFormatter::try_new_narrow_year(prefs, options),
        (_, "second") => RelativeTimeFormatter::try_new_long_second(prefs, options),
        (_, "minute") => RelativeTimeFormatter::try_new_long_minute(prefs, options),
        (_, "hour") => RelativeTimeFormatter::try_new_long_hour(prefs, options),
        (_, "day") => RelativeTimeFormatter::try_new_long_day(prefs, options),
        (_, "week") => RelativeTimeFormatter::try_new_long_week(prefs, options),
        (_, "month") => RelativeTimeFormatter::try_new_long_month(prefs, options),
        (_, "quarter") => RelativeTimeFormatter::try_new_long_quarter(prefs, options),
        (_, "year") => RelativeTimeFormatter::try_new_long_year(prefs, options),
        _ => return None,
    };
    made.ok()
}
