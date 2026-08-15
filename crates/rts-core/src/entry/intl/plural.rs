//! `Intl.PluralRules` — which plural form a number takes in a language.
//!
//! The first service written here, and the shape every other one copies: read
//! the arguments, hand them to ICU4X, put the answer back. The categories are
//! CLDR's — English has two and Arabic has six — and nothing in this file
//! knows which.
//!
//! # Why the formatter is rebuilt per call
//!
//! An ICU4X `PluralRules` is a Rust value and there is nowhere beside a cell to
//! keep one: the aside tables this crate has hold values the collector traces,
//! and a `PluralRules` is neither. So what the cell keeps is the *resolved
//! options* — which is what `resolvedOptions()` has to answer anyway — and the
//! rules are built again from them. Named rather than hidden: it is a data
//! lookup per `select`, and the alternative is a table of live formatters keyed
//! by cell, with the lifetime question that brings.

use icu::plurals::{PluralCategory, PluralRuleType, PluralRules as Rules};

use super::super::with_current;

/// `Intl.PluralRules`.
#[rtse::class("PluralRules")]
impl PluralRules {
    /// `new Intl.PluralRules(locales, options)`.
    ///
    /// The locale is resolved by ICU4X and written back through
    /// [`super::resolved`], so a program asking for one the data does not carry
    /// reads what it actually got.
    #[construct]
    fn build(this: u64, locales: u64, options: u64) -> u64 {
        let tags = super::requested(locales);
        let selected = super::selected(&tags);
        // `cardinal` unless the program said `ordinal`, which is the
        // specification's default and ICU4X's separate rule set: "1st, 2nd"
        // and "1 item, 2 items" are different questions about the same number.
        let ordinal = super::option(options, "type").as_deref() == Some("ordinal");
        let locale_text = selected.to_string();
        if rules_for(&selected, ordinal).is_none() {
            super::super::throw::range_error(&format!(
                "Intl.PluralRules: no plural data for {locale_text}"
            ));
            return with_current(|context| super::super::objects::undefined_of(context));
        }
        with_current(|context| {
            let locale_value = context
                .intern_value(crate::text::Str::from_str(&locale_text))
                .bits();
            let kind_value = context
                .intern_value(crate::text::Str::from_str(match ordinal {
                    true => "ordinal",
                    false => "cardinal",
                }))
                .bits();
            super::resolved(
                context,
                this,
                "PluralRules",
                &[("locale", locale_value), ("type", kind_value)],
            )
        })
    }

    /// `pr.select(n)` — the category `n` takes.
    fn select(this: u64, number: f64) -> u64 {
        let locale = super::locale::parse(super::stored_text(this, "locale").as_deref());
        let ordinal = super::stored_text(this, "type").as_deref() == Some("ordinal");
        let Some(rules) = rules_for(&locale, ordinal) else {
            return with_current(|context| super::super::objects::undefined_of(context));
        };
        // The operands a rule reads are DECIMAL — `1.0` and `1` are different
        // plural inputs in some languages — so the number crosses as its own
        // decimal rather than as an integer.
        let named = match rules.category_for(&super::decimal_of(number)) {
            PluralCategory::Zero => "zero",
            PluralCategory::One => "one",
            PluralCategory::Two => "two",
            PluralCategory::Few => "few",
            PluralCategory::Many => "many",
            PluralCategory::Other => "other",
        };
        with_current(|context| {
            context
                .intern_value(crate::text::Str::from_str(named))
                .bits()
        })
    }

    /// `pr.resolvedOptions()` — the locale and type this one settled on.
    #[js("resolvedOptions")]
    fn resolved_options(this: u64) -> u64 {
        super::stored(this)
    }
}

/// The rules for a locale, or `None` when the data does not carry it.
fn rules_for(locale: &icu::locale::Locale, ordinal: bool) -> Option<Rules> {
    let kind = match ordinal {
        true => PluralRuleType::Ordinal,
        false => PluralRuleType::Cardinal,
    };
    Rules::try_new(locale.into(), kind.into()).ok()
}

