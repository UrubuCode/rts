//! `Intl.DateTimeFormat` — a moment written the way a language writes dates.
//!
//! Not a `printf` over the fields. `en-GB` writes `10/05/2024` and `en-US`
//! writes `5/10/2024` from the same instant; `de` puts a dot after each number
//! and `ja` writes the year first with its own character after it. Which order,
//! which separator, which month name and whether a comma stands before the time
//! are all CLDR's, and nothing in this file knows any of them.
//!
//! # `timeZone` is refused rather than ignored
//!
//! [`super::super::date`] has no timezone database and states why: it is
//! megabytes that change several times a year, and an approximation of it is a
//! wrong answer that looks right for eleven months out of twelve. So local time
//! here *is* UTC, and a `timeZone` naming anything else is a `RangeError`.
//!
//! Formatting in UTC while answering that `America/Sao_Paulo` had been honoured
//! is the failure this whole folder exists to refuse — the program would print
//! a time three hours off and have no way to find out. An absent `timeZone`
//! IS honoured, because absent means "this runtime's local time", which is UTC
//! and is what `getTimezoneOffset()` already reports.
//!
//! # Why the field set is built at run time
//!
//! ICU4X 2.x names a field set by TYPE — `fieldsets::YMD` — so a binary links
//! only the patterns it reaches. A JavaScript program picks the fields at run
//! time, which is what `fieldsets::builder` exists for, and it is the only
//! shape that can answer `{ year, month, day }` and `{ hour, minute }` from one
//! compiled function. The cost is stated: `CompositeDateTimeFieldSet` links the
//! patterns for every combination it can represent.
//!
//! # Where a semantic field set and an ECMA-402 request disagree
//!
//! ECMA-402 names a WIDTH per field and matches a skeleton; ICU4X names a field
//! SET and a length and reads CLDR's pattern for it. Mostly the same answer, and
//! not always: `ja` with year, month and day is `2024/05/10` here and
//! `2024/5/10` in a skeleton engine, and a weekday beside a date is `Fri 10/05`
//! rather than `Fri, 10/05`. Both are CLDR's answers to two slightly different
//! questions, which is why neither is corrected here — correcting one means
//! writing a pattern in this file, and that is what [`super`] forbids.

use icu::datetime::fieldsets::builder::{DateFields, FieldSetBuilder};
use icu::datetime::fieldsets::enums::CompositeDateTimeFieldSet;
use icu::datetime::input::{DateTime, ZonedDateTime};
use icu::datetime::options::{Alignment, Length, TimePrecision, YearStyle};
use icu::datetime::preferences::HourCycle;
use icu::datetime::{DateTimeFormatter, DateTimeFormatterPreferences};
use icu::time::zone::UtcOffset;

use super::super::{Context, with_current};
use crate::value::Value;

/// `Intl.DateTimeFormat`.
#[rtse::class("DateTimeFormat")]
impl DateTimeFormat {
    /// `new Intl.DateTimeFormat(locales, options)`.
    ///
    /// Every option is read here and written back through [`super::resolved`],
    /// and the combination is BUILT here as well rather than only at `format`:
    /// a set of fields ICU4X cannot express — a year and an hour with no month,
    /// say — is the program's mistake, and learning it at construction is what
    /// the specification's `RangeError` is for.
    #[construct]
    fn build(this: u64, locales: u64, options: u64) -> u64 {
        let tags = super::requested(locales);
        let selected = super::selected(&tags);
        let zone = super::option(options, "timeZone");
        if let Some(zone) = zone.as_deref() {
            if !is_utc(zone) {
                super::super::throw::range_error(&format!(
                    "Intl.DateTimeFormat: this runtime has no timezone database, so \
                     {zone} cannot be honoured; local time here is UTC"
                ));
                return with_current(|context| super::super::objects::undefined_of(context));
            }
        }
        let cycle = match flag(options, "hour12") {
            Some(true) => Some("h12".to_owned()),
            Some(false) => Some("h23".to_owned()),
            None => super::option(options, "hourCycle"),
        };
        let locale_text = selected.to_string();

        let mut fields: Vec<(&str, u64)> = Vec::with_capacity(12);
        let mut asked = Asked::default();
        for (position, name) in NAMED.iter().enumerate() {
            let Some(found) = super::option(options, name) else {
                continue;
            };
            let value = with_current(|context| {
                context
                    .intern_value(crate::text::Str::from_str(&found))
                    .bits()
            });
            fields.push((name, value));
            asked.fields[position] = Some(found);
        }
        asked.hour_cycle = cycle.clone();
        if field_set(&asked).is_none() {
            super::super::throw::range_error(
                "Intl.DateTimeFormat: those date and time fields are not a shape the \
                 locale data can be asked for",
            );
            return with_current(|context| super::super::objects::undefined_of(context));
        }

        with_current(|context| {
            let mut written = |text: &str| {
                context
                    .intern_value(crate::text::Str::from_str(text))
                    .bits()
            };
            let locale_value = written(&locale_text);
            // Always UTC, and written out rather than echoed back: an absent
            // `timeZone` resolves to this runtime's, which is UTC, so a program
            // reading `resolvedOptions().timeZone` learns the same fact the
            // refusal above would have told it.
            let zone_value = written("UTC");
            let cycle_value = cycle.as_deref().map(&mut written);
            fields.push(("locale", locale_value));
            fields.push(("timeZone", zone_value));
            if let Some(cycle_value) = cycle_value {
                fields.push(("hourCycle", cycle_value));
            }
            super::resolved(context, this, "DateTimeFormat", &fields)
        })
    }

    /// `dtf.resolvedOptions()`.
    #[js("resolvedOptions")]
    fn resolved_options(this: u64) -> u64 {
        super::stored(this)
    }
}

/// The options that name a field or a style, in the spelling a program writes.
///
/// A list rather than eleven reads, because the constructor stores them and
/// `format` reads them back and the two have to agree on the set — which is
/// what a second hand-written list would eventually stop doing.
const NAMED: &[&str] = &[
    "weekday", "year", "month", "day", "hour", "minute", "second", "dateStyle", "timeStyle",
];

/// What a formatter was asked to show.
///
/// An array in [`NAMED`]'s order rather than nine named fields, so the list
/// that STORES the options and the one that reads them back cannot drift apart:
/// there is one list, and both walk it.
#[derive(Default)]
struct Asked {
    fields: [Option<String>; NAMED.len()],
    hour_cycle: Option<String>,
}

impl Asked {
    /// One field, by the name a program wrote; `None` for one it did not.
    fn at(&self, name: &str) -> Option<&str> {
        let position = NAMED.iter().position(|held| *held == name)?;
        self.fields[position].as_deref()
    }
}

/// What a formatter resolved, read back off the object it was stored on.
fn asked_of(this: u64) -> Asked {
    let mut asked = Asked::default();
    for (position, name) in NAMED.iter().enumerate() {
        asked.fields[position] = super::stored_text(this, name);
    }
    asked.hour_cycle = super::stored_text(this, "hourCycle");
    asked
}

/// Whether a `timeZone` names the only zone this runtime has.
///
/// The five spellings a program actually writes for it. Not a zone database in
/// miniature: every one of these is UTC itself under a different name, so
/// accepting them decides nothing — where accepting a sixth would mean claiming
/// an offset this crate cannot compute.
fn is_utc(zone: &str) -> bool {
    matches!(
        zone.trim().to_ascii_uppercase().as_str(),
        "UTC" | "ETC/UTC" | "ETC/GMT" | "GMT" | "Z"
    )
}

/// The date fields a request names, or `None` when it names no date at all.
fn date_fields(asked: &Asked) -> Option<DateFields> {
    if let Some(style) = asked.at("dateStyle") {
        // A `dateStyle` names a whole date rather than a set of fields, and
        // `full` is the one that carries the weekday — which is the only
        // difference between the four in what they SHOW rather than how long
        // they are, the rest being [`length_of`]'s question.
        return Some(match style {
            "full" => DateFields::YMDE,
            _ => DateFields::YMD,
        });
    }
    let fields = match (
        asked.at("year").is_some(),
        asked.at("month").is_some(),
        asked.at("day").is_some(),
        asked.at("weekday").is_some(),
    ) {
        (false, false, false, false) => return None,
        (false, false, false, true) => DateFields::E,
        (false, false, true, false) => DateFields::D,
        (false, false, true, true) => DateFields::DE,
        (false, true, false, _) => DateFields::M,
        (false, true, true, false) => DateFields::MD,
        (false, true, true, true) => DateFields::MDE,
        (true, false, false, _) => DateFields::Y,
        (true, true, false, _) => DateFields::YM,
        (true, _, _, false) => DateFields::YMD,
        (true, _, _, true) => DateFields::YMDE,
    };
    Some(fields)
}

/// How far down the clock a request reads, or `None` when it names no time.
fn precision(asked: &Asked) -> Option<TimePrecision> {
    if let Some(style) = asked.at("timeStyle") {
        return Some(match style {
            "short" => TimePrecision::Minute,
            _ => TimePrecision::Second,
        });
    }
    match (
        asked.at("hour").is_some(),
        asked.at("minute").is_some(),
        asked.at("second").is_some(),
    ) {
        (_, _, true) => Some(TimePrecision::Second),
        (_, true, false) => Some(TimePrecision::Minute),
        (true, false, false) => Some(TimePrecision::Hour),
        (false, false, false) => None,
    }
}

/// How long a formatted date is allowed to be.
///
/// ECMA-402 spells this per FIELD — a month is `numeric`, `short` or `long` —
/// and ICU4X spells it once for the whole pattern, because a locale decides
/// together whether it writes `10/05/2024` or `10 May 2024`. The month is what
/// carries the distinction where it is present, and the weekday where it is
/// not; a request with neither is numeric, and numeric is `Short`.
fn length_of(asked: &Asked) -> Length {
    if let Some(style) = asked.at("dateStyle").or(asked.at("timeStyle")) {
        return match style {
            "full" | "long" => Length::Long,
            "medium" => Length::Medium,
            _ => Length::Short,
        };
    }
    match asked.at("month").or(asked.at("weekday")) {
        Some("long") => Length::Long,
        Some("short") | Some("narrow") => Length::Medium,
        _ => Length::Short,
    }
}

/// Whether the numbers should be padded, which is what `2-digit` asks for.
///
/// ICU4X has no per-field width. It has `Alignment::Column`, which pads every
/// numeric field at once, and that is what a program writing `2-digit` on one
/// field and `numeric` on another does not get: it gets both padded. Named as
/// the divergence it is — the alternative is a hand-written pattern, which is
/// the thing this folder refuses.
fn column(asked: &Asked) -> bool {
    ["month", "day", "hour", "minute", "second"]
        .iter()
        .any(|field| asked.at(field) == Some("2-digit"))
}

/// The field set a request builds, or `None` when it builds none.
///
/// Built twice on purpose. `Alignment` is an option not every field set can
/// honour — a standalone weekday has no numeric field to pad — and ICU4X
/// answers `SuperfluousOptions` rather than ignoring it. Asking and falling
/// back keeps the knowledge of WHICH sets take it in ICU4X, where it belongs,
/// instead of in a table here that would go stale on the next release.
fn field_set(asked: &Asked) -> Option<CompositeDateTimeFieldSet> {
    let fields = date_fields(asked);
    let time = precision(asked);
    // The specification's own default for an empty bag: year, month and day,
    // all numeric. Applied here rather than at each caller so that
    // `new Intl.DateTimeFormat()` and `{ timeZone: "UTC" }` mean the same thing.
    let fields = match (fields, time) {
        (None, None) => Some(DateFields::YMD),
        (fields, _) => fields,
    };
    // Only where the program named the FIELDS. A `dateStyle` names a whole
    // date, and how many digits its year has is part of what the locale decided
    // when it defined that style: `en-US` writes `5/10/24` for `short` and
    // forcing the century there would be this file overruling CLDR.
    let dated = asked.at("dateStyle").is_none()
        && matches!(
            fields,
            Some(DateFields::Y | DateFields::YM | DateFields::YMD | DateFields::YMDE)
        );
    let mut builder = FieldSetBuilder::new();
    builder.date_fields = fields;
    builder.time_precision = time;
    builder.length = Some(length_of(asked));
    if dated {
        // `Full` where the program said `numeric`, which is what it means: the
        // default `Auto` writes a recent year as two digits, and `2024` was
        // asked for. `2-digit` takes `Auto` — the closest ICU4X has, and it
        // differs for a year far enough in the past to be unambiguous.
        builder.year_style = Some(match asked.at("year") {
            Some("2-digit") => YearStyle::Auto,
            _ => YearStyle::Full,
        });
    }
    if !column(asked) {
        return builder.build_composite_datetime().ok();
    }
    let mut padded = builder.clone();
    padded.alignment = Some(Alignment::Column);
    padded
        .build_composite_datetime()
        .ok()
        .or_else(|| builder.build_composite_datetime().ok())
}

/// The hour cycle a request names, when it names one.
///
/// `h24` maps onto `H23`: ICU4X carries three cycles and midnight-as-24 is not
/// one of them. Stated rather than silently dropped — a program asking for it
/// gets a zero-based day where it wanted a one-based one, and no locale data
/// disagrees about anything else.
fn hour_cycle(asked: &Asked) -> Option<HourCycle> {
    match asked.hour_cycle.as_deref()? {
        "h11" => Some(HourCycle::H11),
        "h12" => Some(HourCycle::H12),
        _ => Some(HourCycle::H23),
    }
}

/// Reads a boolean option off the bag a constructor was given.
///
/// [`super::option`] reads the text options every service shares; `hour12` is
/// the one option in this folder that is a boolean, and `undefined` has to stay
/// distinguishable from `false` — `hour12: false` means h23 and an absent one
/// means whatever the locale says.
fn flag(bag: u64, name: &str) -> Option<bool> {
    let key = with_current(|context| context.well_known_text(name));
    let found = super::super::computed::get_indexed(bag, key);
    if super::super::throw::in_flight() {
        return None;
    }
    let absent = with_current(|context| super::super::objects::undefined_of(context));
    match found == absent {
        true => None,
        false => Some(super::super::class_support::to_boolean(found)),
    }
}


/// The time value an argument names, in milliseconds since the epoch.
///
/// A `Date` carries it in the property [`super::super::date::TIME`] names, read
/// here rather than through a getter for the reason that module gives: the time
/// value is an ordinary property precisely so the fast path can see it.
fn milliseconds(value: u64) -> f64 {
    let absent = with_current(|context| super::super::objects::undefined_of(context));
    if value == absent {
        return super::super::date::now_ms();
    }
    let held = with_current(|context| {
        let cell = Value(value).as_slot()?;
        let key = context.well_known(super::super::date::TIME);
        super::super::objects::read_property(context, cell, key)?.as_f64()
    });
    // Anything else is `ToNumber`'d, which is what the specification does and
    // what makes `dtf.format(1715299200000)` work. Outside the borrow above,
    // because a conversion can call user code.
    held.unwrap_or_else(|| super::super::class_support::to_number(value))
}

/// What `format` answers, or `None` when the locale carries no data for it.
fn written(this: u64, ms: f64) -> Option<String> {
    let asked = asked_of(this);
    let locale = super::locale::parse(super::stored_text(this, "locale").as_deref());
    let set = field_set(&asked)?;
    let mut prefs = DateTimeFormatterPreferences::from(&locale);
    if let Some(cycle) = hour_cycle(&asked) {
        prefs.hour_cycle = Some(cycle);
    }
    let formatter = DateTimeFormatter::try_new(prefs, set).ok()?;
    // The calendar arithmetic is ICU4X's, not this file's: a second
    // days-to-civil-date conversion beside `date::civil`'s would be two answers
    // to one question, and this one has to agree with `getUTCDate()` exactly.
    let moment = ZonedDateTime::from_epoch_milliseconds_and_utc_offset(ms as i64, UtcOffset::zero());
    let moment = DateTime {
        date: moment.date,
        time: moment.time,
    };
    Some(formatter.format(&moment).to_string())
}

/// `Intl.DateTimeFormat.prototype.format` is an ACCESSOR.
///
/// [`super::number`]'s own `install_format` states why in full, and the failure
/// is the same one: `days.map(dtf.format)` detaches the function from its
/// receiver, and a plain method would then format with the root locale and
/// answer a list that looks formatted.
fn install_format(context: &mut Context, prototype: u64) {
    let key = super::super::modules::member_key(context, "format");
    let getter =
        super::super::native::callable(context, bound_format as super::super::native::Native);
    super::super::native::name_of(context, getter, "get format");
    if let Some(cell) = Value(prototype).as_slot() {
        context.set_accessor(cell, key, Some(getter), None);
    }
}

/// The getter: a function that already knows which formatter it formats with.
extern "C" fn bound_format(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    with_current(|context| {
        let code = format_with as super::super::native::Native;
        let made = super::super::native::callable(context, code);
        if let Some(cell) = Value(made).as_slot() {
            context.mark_callable(cell, code as usize as u64, this);
        }
        super::super::native::name_of(context, made, "");
        made
    })
}

/// The bound function itself: the formatter arrives as the environment.
extern "C" fn format_with(
    formatter: u64,
    _this: u64,
    value: u64,
    _a1: u64,
    _a2: u64,
    _a3: u64,
) -> u64 {
    if super::stored_text(formatter, "locale").is_none() {
        super::super::throw::type_error(
            "Intl.DateTimeFormat.prototype.format called on a value that is not a date format",
        );
        return with_current(|context| super::super::objects::undefined_of(context));
    }
    let ms = milliseconds(value);
    if !ms.is_finite() {
        // What the specification says, and it is a real distinction: a date
        // that does not exist has no rendering, where `new Date(0)` has one.
        super::super::throw::range_error("Invalid time value");
        return with_current(|context| super::super::objects::undefined_of(context));
    }
    let Some(text) = written(formatter, ms) else {
        return with_current(|context| super::super::objects::undefined_of(context));
    };
    with_current(|context| {
        context
            .intern_value(crate::text::Str::from_str(&text))
            .bits()
    })
}

/// Installs `Intl.DateTimeFormat` with its accessor.
pub(super) fn register_date_time_format_class(context: &mut Context) -> u64 {
    let made = register_date_time_format(context);
    if let Some(prototype) = super::super::class_support::prototype(context, "DateTimeFormat") {
        install_format(context, prototype);
    }
    made
}
