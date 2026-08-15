//! `Intl.ListFormat` — "a, b, and c" in whatever the language does instead.
//!
//! The joining word, whether a comma precedes it, and whether there is a comma
//! at all are CLDR's answers and not this file's. English writes
//! "a, b, and c"; Spanish writes "a, b y c" with no comma before the
//! conjunction, and nothing here knows that.

use icu::list::options::{ListFormatterOptions, ListLength};
use icu::list::ListFormatter;

use super::super::with_current;
use crate::value::Value;

/// `Intl.ListFormat`.
#[rtse::class("ListFormat")]
impl ListFormat {
    /// `new Intl.ListFormat(locales, options)`.
    ///
    /// `type` picks the relation — a conjunction is "and", a disjunction is
    /// "or", and a unit list is neither — and `style` picks how much of the
    /// word is written. Both default the way the specification defaults them.
    #[construct]
    fn build(this: u64, locales: u64, options: u64) -> u64 {
        let tags = super::requested(locales);
        let selected = super::selected(&tags);
        let kind = super::option(options, "type").unwrap_or_else(|| "conjunction".to_owned());
        let style = super::option(options, "style").unwrap_or_else(|| "long".to_owned());
        let locale_text = selected.to_string();
        if formatter(&selected, &kind, &style).is_none() {
            super::super::throw::range_error(&format!(
                "Intl.ListFormat: no list data for {locale_text}"
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
            let kind_value = written(&kind);
            let style_value = written(&style);
            super::resolved(
                context,
                this,
                "ListFormat",
                &[
                    ("locale", locale_value),
                    ("type", kind_value),
                    ("style", style_value),
                ],
            )
        })
    }

    /// `lf.format(list)` — the elements joined the way the language joins them.
    fn format(this: u64, list: u64) -> u64 {
        let parts = strings_of(list);
        let locale = super::locale::parse(super::stored_text(this, "locale").as_deref());
        let kind = super::stored_text(this, "type").unwrap_or_default();
        let style = super::stored_text(this, "style").unwrap_or_default();
        let Some(formatter) = formatter(&locale, &kind, &style) else {
            return with_current(|context| super::super::objects::undefined_of(context));
        };
        let joined = formatter.format_to_string(parts.iter());
        with_current(|context| {
            context
                .intern_value(crate::text::Str::from_str(&joined))
                .bits()
        })
    }

    /// `lf.resolvedOptions()`.
    #[js("resolvedOptions")]
    fn resolved_options(this: u64) -> u64 {
        super::stored(this)
    }
}

/// The formatter for a locale and a pair of options.
///
/// `None` for a combination ICU4X has no data for, which the constructor turns
/// into a `RangeError` and every method turns into `undefined` — the method
/// cannot raise usefully, because the constructor already refused the same
/// combination and a program reaching here built its formatter some other way.
fn formatter(locale: &icu::locale::Locale, kind: &str, style: &str) -> Option<ListFormatter> {
    let length = match style {
        "short" => ListLength::Short,
        "narrow" => ListLength::Narrow,
        _ => ListLength::Wide,
    };
    let options = ListFormatterOptions::default().with_length(length);
    let made = match kind {
        "disjunction" => ListFormatter::try_new_or(locale.into(), options),
        "unit" => ListFormatter::try_new_unit(locale.into(), options),
        _ => ListFormatter::try_new_and(locale.into(), options),
    };
    made.ok()
}

/// The elements of a list argument, as text.
///
/// Every element is `ToString`'d by the specification; this reads the ones that
/// are already strings and skips the rest, which is the same limit
/// [`super::requested`] has and for the same reason: converting one calls user
/// code, and the conversion belongs where the argument is read rather than
/// inside a borrow.
fn strings_of(list: u64) -> Vec<String> {
    with_current(|context| {
        let Some(cell) = Value(list).as_slot() else {
            return Vec::new();
        };
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
