//! `Intl` — the ECMA-402 surface, over real CLDR data.
//!
//! # Why this exists at all, having once been refused
//!
//! It was refused, in as many words: "a surface that cannot do what its name
//! means does not ship", the rule the `sync` namespace was deleted for. An
//! `Intl` that formatted every locale in English while answering that it had
//! formatted `pt-BR` is that rule's own example — `1,234.50` shown to a reader
//! who writes `1.234,50` is a wrong answer that looks right.
//!
//! What changed is not the rule but the FACT it was applied to. The refusal
//! assumed this crate could not carry locale data, because carrying it means
//! megabytes that change several times a year — the same argument
//! [`super::date`] makes for having no timezone database. ICU4X carries it, in
//! Rust, compiled into the binary, and it exists precisely for engines that
//! cannot ship a C library or read files at run time. Half of it was already
//! linked here before this module existed: `idna` pulls `icu_normalizer` and
//! `icu_properties` in through `url`.
//!
//! So the data question has an answer, and with it the refusal has no premise.
//!
//! # What this module is, and what ICU4X is
//!
//! Everything here is a shell: read the arguments the language defines, hand
//! them to ICU4X, and put what comes back into a JavaScript value. **No
//! formatting rule is written in this folder.** A separator, a currency symbol,
//! a date order, a plural category and a word boundary are all CLDR's, and a
//! second copy of any of them here would be a second answer to a question the
//! data already answers — which is how `en-GB` would come to write the day
//! first in one place and second in another.
//!
//! The first draft of this folder DID write those rules, for three English
//! tags, and it is worth keeping that written down: it looked reasonable, it
//! passed the fixtures, and it was the hollow surface the rule above forbids.
//!
//! # Where a program sees the limit
//!
//! `resolvedOptions()` answers the locale that was actually selected rather
//! than the one that was asked for. That is the specification's own mechanism
//! for a fallback and it is the honest one: a program asking for a locale the
//! data does not carry reads back the one it got.
//!
//! # What is deliberately absent
//!
//! `Intl.DisplayNames`, `Intl.DurationFormat` and `Intl.Locale`'s full
//! property surface. Each is more shell over the same data rather than a new
//! question, and they wait for a program that needs them rather than being
//! anticipated.

mod collate;
mod datetime;
mod list;
mod locale;
mod number;
mod plural;
mod relative;
mod segment;

// The declarations `rts emit-types` prints. Named from here rather than from
// each service, because `declared::CLASSES` reads ONE path per class and the
// services are a folder — so this is the folder's own index of them.
pub(in crate::entry) use collate::COLLATOR_TYPES;
pub(in crate::entry) use datetime::DATE_TIME_FORMAT_TYPES;
pub(in crate::entry) use list::LIST_FORMAT_TYPES;
pub(in crate::entry) use number::NUMBER_FORMAT_TYPES;
pub(in crate::entry) use plural::PLURAL_RULES_TYPES;
pub(in crate::entry) use relative::RELATIVE_TIME_FORMAT_TYPES;
pub(in crate::entry) use segment::SEGMENTER_TYPES;

use super::{Context, with_current};
use crate::value::Value;

/// The property a formatter keeps its resolved options in.
///
/// An ordinary property, for the reason [`super::date`]'s `__dateValue` is one:
/// compiled code reading a property that is in the shape's layout never asks
/// the runtime at all, so a value only a side table knows about is one the fast
/// path disagrees with the moment it starts working. The divergence is the same
/// one that module names — `Object.keys(new Intl.NumberFormat())` shows it —
/// and it is the honest trade until this crate has a slot kind that is present
/// in the layout and absent from enumeration.
pub(in crate::entry) const RESOLVED: &str = "__intlResolved";

/// `Intl`.
#[rtse::class("Intl", namespace)]
impl Intl {
    /// `Intl.getCanonicalLocales(tags)` — the canonical spelling of each tag.
    ///
    /// Answers an array, and refuses a tag ICU4X cannot read with a
    /// `RangeError`, which is what the specification says: a malformed tag is
    /// the program's mistake, where an unsupported one is a fallback the
    /// program can observe through `resolvedOptions()`.
    #[js("getCanonicalLocales")]
    fn get_canonical_locales(tags: u64) -> u64 {
        let asked = requested(tags);
        let mut canonical = Vec::with_capacity(asked.len());
        for tag in &asked {
            let Some(spelled) = locale::canonical(tag) else {
                super::throw::range_error(&format!("Incorrect locale information provided: {tag}"));
                return with_current(|context| super::objects::undefined_of(context));
            };
            canonical.push(spelled);
        }
        let values = with_current(|context| {
            canonical
                .into_iter()
                .map(|text| context.intern_value(crate::text::Str::from_str(&text)).bits())
                .collect::<Vec<u64>>()
        });
        with_current(|context| super::array::built_in(context, values))
    }
}

/// The locale tags an argument names: a string is one, an array is many, and
/// `undefined` is none.
///
/// Shared by every constructor here, because the first argument of all of them
/// is the same argument — and reading it in seven places is seven chances to
/// disagree about whether a bare string counts.
pub(super) fn requested(value: u64) -> Vec<String> {
    with_current(|context| {
        if value == super::objects::undefined_of(context) {
            return Vec::new();
        }
        let Some(cell) = Value(value).as_slot() else {
            return Vec::new();
        };
        if let Some(text) = context.text_at(cell) {
            return text.to_rust().into_iter().collect();
        }
        let Some(elements) = context.elements_at(cell).cloned() else {
            return Vec::new();
        };
        elements
            .iter()
            .filter_map(|held| {
                let cell = Value(*held).as_slot()?;
                context.text_at(cell)?.to_rust()
            })
            .collect()
    })
}

/// The locale a constructor's first argument selects.
pub(super) fn selected(tags: &[String]) -> icu::locale::Locale {
    locale::parse(tags.first().map(String::as_str))
}

/// Reads one option off the bag a constructor was given, as text.
///
/// `None` for an absent option, which every caller turns into the
/// specification's own default — kept out of here because the defaults differ
/// per service and a shared one would be a table of them.
pub(super) fn option(bag: u64, name: &str) -> Option<String> {
    let key = with_current(|context| context.well_known_text(name));
    let found = super::computed::get_indexed(bag, key);
    if super::throw::in_flight() {
        return None;
    }
    with_current(|context| {
        let cell = Value(found).as_slot()?;
        context.text_at(cell)?.to_rust()
    })
}

/// Writes the resolved options onto a formatter, and answers the formatter.
///
/// Every constructor ends this way: whatever it resolved becomes an object the
/// program can read back, which is what makes the fallback observable.
pub(super) fn resolved(context: &mut Context, this: u64, class: &str, fields: &[(&str, u64)]) -> u64 {
    let Some(cell) = super::class_support::receiver(context, this, class) else {
        return super::objects::undefined_of(context);
    };
    let Some(holder) = super::native::plain(context) else {
        return super::objects::undefined_of(context);
    };
    for (name, value) in fields {
        let key = context.well_known(name);
        super::objects::put(context, holder, key, *value);
    }
    let key = context.well_known(RESOLVED);
    let held = Value::from_slot(holder).bits();
    super::objects::put(context, cell, key, held);
    Value::from_slot(cell).bits()
}

/// Installs `Intl` with its services on it.
///
/// A wrapper around the registration the attribute derives, for the reason
/// [`super::collections`]'s registrations are wrapped: the attribute makes an
/// object and hangs MEMBERS on it, and these are constructors — objects of
/// their own, each with a prototype, which nothing in the attribute's shape can
/// express.
pub(in crate::entry) fn register_intl_namespace(context: &mut Context) -> u64 {
    let object = register_intl(context);
    let Some(cell) = Value(object).as_slot() else {
        return object;
    };
    for (name, register) in SERVICES {
        let made = register(context);
        let key = context.well_known(name);
        super::objects::put(context, cell, key, made);
    }
    object
}

/// What a formatter resolved, as the object `resolvedOptions()` answers.
///
/// The same object [`resolved`] wrote, handed back rather than rebuilt: a
/// second construction would be a second answer to "what did this settle on",
/// and the two would differ the first time a service grew a field.
/// The decimal every ICU4X formatter reads, from a JavaScript number.
///
/// Through the engine's OWN `ToString`, not through `Decimal::try_from_f64`.
/// Two reasons, and the second decided it: that constructor is behind a
/// `fixed_decimal` feature this crate does not turn on, and it would be a
/// SECOND float-to-decimal conversion beside the one `coerce` already owns — so
/// `(1.0).toString()` and what a formatter sees could come to disagree about how
/// many fraction digits a number has, which is exactly what a plural rule reads
/// them for.
///
/// Here rather than in one service because three of them ask it — `number`,
/// `plural` and `relative`. It lived in `number.rs` saying "the day a third
/// needs it, that is where it goes"; a third already did, on the same day.
pub(super) fn decimal_of(number: f64) -> icu::decimal::input::Decimal {
    let printed = crate::coerce::number_to_string(number)
        .to_rust()
        .unwrap_or_default();
    printed.parse().unwrap_or_else(|_| 0.into())
}

pub(super) fn stored(this: u64) -> u64 {
    with_current(|context| {
        let absent = super::objects::undefined_of(context);
        let Some(cell) = Value(this).as_slot() else {
            return absent;
        };
        let key = context.well_known(RESOLVED);
        super::objects::read_property(context, cell, key).map_or(absent, |found| found.bits())
    })
}

/// One text field of what a formatter resolved.
///
/// For a member that has to rebuild the ICU4X object it was constructed with —
/// which is all of them, because an ICU4X formatter is not a value this crate
/// can keep beside a cell.
pub(super) fn stored_text(this: u64, name: &str) -> Option<String> {
    let held = stored(this);
    with_current(|context| {
        let cell = Value(held).as_slot()?;
        let key = context.well_known(name);
        let found = super::objects::read_property(context, cell, key)?;
        let cell = Value(found.bits()).as_slot()?;
        context.text_at(cell)?.to_rust()
    })
}

/// The services `Intl` holds, by the name a program reads.
///
/// A list rather than seven lines, so adding one is one row — and so that the
/// TypeScript declarations and this agree by construction.
const SERVICES: &[(&str, fn(&mut Context) -> u64)] = &[
    ("Collator", collate::register_collator_class),
    ("DateTimeFormat", datetime::register_date_time_format_class),
    ("ListFormat", list::register_list_format),
    ("NumberFormat", number::register_number_format_class),
    ("PluralRules", plural::register_plural_rules),
    ("RelativeTimeFormat", relative::register_relative_time_format),
    ("Segmenter", segment::register_segmenter),
];
