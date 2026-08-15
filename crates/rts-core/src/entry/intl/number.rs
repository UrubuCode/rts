//! `Intl.NumberFormat` — a number written the way a language writes numbers.
//!
//! Not `toFixed` with commas inserted. The grouping separator, the decimal
//! separator, how far apart the groups are, the currency symbol, which side of
//! the number it goes on and whether a space separates them are all CLDR's:
//! `pt-BR` writes `1.234,50`, `de-CH` writes `1’234.50`, and `fr-FR` puts the
//! euro sign last. Nothing in this file knows any of that.
//!
//! # What this file DOES decide, and why that is not the same thing
//!
//! Two arithmetic questions the specification owns rather than CLDR:
//! how many fraction digits to keep, and how a tie rounds. Both are stated by
//! ECMA-402 in the same words for every locale — "the default maximum is 3",
//! "the default rounding mode is `halfExpand`" — so writing them here is
//! writing the language, which is this crate's job. A separator would not be.
//!
//! # Why the formatter is rebuilt per call
//!
//! [`super::plural`] states it in full: an ICU4X formatter is not a value this
//! crate can keep beside a cell, so the cell keeps the RESOLVED OPTIONS and the
//! formatter is built again from them.

use icu::decimal::input::Decimal;
use icu::decimal::options::DecimalFormatterOptions;
use icu::decimal::DecimalFormatter;
use icu::experimental::dimension::currency::formatter::CurrencyFormatter;
use icu::experimental::dimension::currency::options::{CurrencyFormatterOptions, CurrencyUsage};
use icu::experimental::dimension::currency::CurrencyType;
use icu::experimental::dimension::percent::formatter::PercentFormatter;
use icu::experimental::dimension::percent::options::PercentFormatterOptions;

use super::super::{Context, with_current};
use crate::value::Value;

/// `Intl.NumberFormat`.
#[rtse::class("NumberFormat")]
impl NumberFormat {
    /// `new Intl.NumberFormat(locales, options)`.
    ///
    /// `style` picks which of ICU4X's three formatters answers — a plain
    /// decimal, a percent, or a currency — and the rest of the bag configures
    /// the one that was picked. A currency style with no `currency` is a
    /// `TypeError` and a `currency` ICU4X cannot read is a `RangeError`, which
    /// is the specification's own split: the first is the program forgetting an
    /// argument, the second is the program writing a bad one.
    #[construct]
    fn build(this: u64, locales: u64, options: u64) -> u64 {
        let tags = super::requested(locales);
        let selected = super::selected(&tags);
        let style = super::option(options, "style").unwrap_or_else(|| "decimal".to_owned());
        let display =
            super::option(options, "currencyDisplay").unwrap_or_else(|| "symbol".to_owned());
        let sign = super::option(options, "currencySign").unwrap_or_else(|| "standard".to_owned());
        let currency = super::option(options, "currency").unwrap_or_default();
        let minimum = digits(options, "minimumFractionDigits");
        let maximum = digits(options, "maximumFractionDigits");
        let locale_text = selected.to_string();

        if style == "currency" {
            if currency.is_empty() {
                super::super::throw::type_error(
                    "Intl.NumberFormat: currency code is required with currency style",
                );
                return with_current(|context| super::super::objects::undefined_of(context));
            }
            if CurrencyType::try_from_str(&currency).is_err() {
                super::super::throw::range_error(&format!(
                    "Invalid currency code : {currency}"
                ));
                return with_current(|context| super::super::objects::undefined_of(context));
            }
        }

        with_current(|context| {
            let mut fields: Vec<(&str, u64)> = Vec::with_capacity(6);
            let mut written = |text: &str| {
                context
                    .intern_value(crate::text::Str::from_str(text))
                    .bits()
            };
            fields.push(("locale", written(&locale_text)));
            fields.push(("style", written(&style)));
            if style == "currency" {
                fields.push(("currency", written(&currency)));
                fields.push(("currencyDisplay", written(&display)));
                fields.push(("currencySign", written(&sign)));
            }
            // Absent unless the program named them, and that absence is
            // deliberate: the specification fills the defaults for a currency
            // from that currency's own CLDR fraction digits, and ICU4X keeps
            // that table inside its currency formatter with no public reader.
            // A number here would be this file's GUESS at CLDR — the one thing
            // `super`'s documentation says this folder must never contain — so
            // `resolvedOptions()` under-reports rather than mis-reports.
            for (name, asked) in [
                ("minimumFractionDigits", minimum),
                ("maximumFractionDigits", maximum),
            ] {
                if let Some(asked) = asked {
                    fields.push((name, Value::from_f64(asked).bits()));
                }
            }
            super::resolved(context, this, "NumberFormat", &fields)
        })
    }

    /// `nf.resolvedOptions()`.
    #[js("resolvedOptions")]
    fn resolved_options(this: u64) -> u64 {
        super::stored(this)
    }
}


/// One fraction-digit option off the bag, as the count it names.
///
/// [`super::option`] reads the text options every service shares; this reads a
/// NUMBER, which only this service has. Clamped to `0..=100` because that is
/// the range the specification defines them over, and truncated because a
/// fractional count of digits is not a thing a formatter can be asked for.
fn digits(bag: u64, name: &str) -> Option<f64> {
    let key = with_current(|context| context.well_known_text(name));
    let found = super::super::computed::get_indexed(bag, key);
    if super::super::throw::in_flight() {
        return None;
    }
    let asked = super::super::class_support::to_number(found);
    match asked.is_nan() {
        true => None,
        false => Some(asked.trunc().clamp(0.0, 100.0)),
    }
}

/// One fraction-digit field of what a formatter resolved.
///
/// [`super::stored_text`] is the same read for a text field; this one exists
/// because these two fields are numbers, and reading a number back out of its
/// own text would be a third float-to-decimal conversion.
fn stored_digits(this: u64, name: &str) -> Option<i16> {
    let held = super::stored(this);
    with_current(|context| {
        let cell = Value(held).as_slot()?;
        let key = context.well_known(name);
        let found = super::super::objects::read_property(context, cell, key)?;
        Some(found.as_f64()? as i16)
    })
}

/// A decimal rounded at `position`, away from zero on an exact tie.
///
/// `halfExpand` is ECMA-402's default rounding mode and `fixed_decimal`'s
/// `round` is half-to-EVEN, which differs on exactly one input shape: the digits
/// below the cut are `5` followed by nothing. That case is detectable — the
/// lowest nonzero digit is the `5` itself — so it is detected and expanded, and
/// everything else takes the library's own rounding.
///
/// The alternative was `round_with_mode(_, SignedRoundingMode::…)`, which says
/// this in one call and cannot be written: `SignedRoundingMode` lives in
/// `fixed_decimal`, which the `icu` facade does not re-export and which this
/// crate does not depend on directly. Naming a second crate to get one enum is
/// the bigger change; this is four lines and pinned by `2.5` rounding to `3`.
fn half_expanded(decimal: Decimal, position: i16) -> Decimal {
    let tie = decimal.absolute.nonzero_magnitude_end() == position - 1
        && decimal.absolute.digit_at(position - 1) == 5;
    match tie {
        true => decimal.expanded(position),
        false => decimal.rounded(position),
    }
}

/// A decimal cut to the fraction digits a formatter resolved.
///
/// `default_max` differs per style and is the specification's, not CLDR's: 3
/// for a plain decimal and 0 for a percent. The currency style never reaches
/// here — ICU4X applies that currency's own fraction digits inside its
/// formatter, which is the CLDR answer and the one that must win.
fn fitted(this: u64, decimal: Decimal, default_max: i16) -> Decimal {
    let minimum = stored_digits(this, "minimumFractionDigits").unwrap_or(0);
    // A maximum below the minimum is the minimum: the specification resolves
    // the pair that way, and the alternative is a formatter that pads to two
    // digits and then cuts to one.
    let maximum = stored_digits(this, "maximumFractionDigits")
        .unwrap_or(default_max)
        .max(minimum);
    let mut decimal = half_expanded(decimal, -maximum);
    // Rounding leaves the cut position DISPLAYED, so `1234.5` rounded at -3 is
    // `1234.500`; padding to the minimum is what takes the zeros back off,
    // because `pad_end` truncates trailing zeros it is not asked to keep.
    decimal.absolute.pad_end(-minimum);
    decimal
}

/// What `format` answers, or `None` when the locale carries no data for it.
fn written(this: u64, value: f64) -> Option<String> {
    // A number with no decimal form. CLDR carries a NaN and an infinity symbol
    // per locale, and ICU4X 2.x exposes neither outside its own formatter — so
    // this answers the number's own text rather than guessing at the symbol,
    // and says so instead of printing `0`, which is what parsing "NaN" as a
    // decimal would have produced.
    if !value.is_finite() {
        return crate::coerce::number_to_string(value).to_rust();
    }
    let locale = super::locale::parse(super::stored_text(this, "locale").as_deref());
    let style = super::stored_text(this, "style").unwrap_or_default();
    let decimal = super::decimal_of(value);
    match style.as_str() {
        "currency" => {
            let code = super::stored_text(this, "currency")?;
            let currency = CurrencyType::try_from_str(&code).ok()?;
            let display = super::stored_text(this, "currencyDisplay").unwrap_or_default();
            let usage = match super::stored_text(this, "currencySign").as_deref() {
                Some("accounting") => CurrencyUsage::Accounting,
                _ => CurrencyUsage::Standard,
            };
            let options = CurrencyFormatterOptions::from(usage);
            let prefs = (&locale).into();
            let made = match display.as_str() {
                "narrowSymbol" => {
                    CurrencyFormatter::try_new_symbol_narrow(prefs, currency, options)
                }
                "code" => CurrencyFormatter::try_new_code(prefs, currency, options),
                // The spelled-out name — "1,234.50 US dollars" — is the one
                // form whose pattern is chosen by a PLURAL category, which is
                // why ICU4X builds it without a usage bag: an accounting
                // pattern has no name form to be accounting in.
                "name" => CurrencyFormatter::try_new_name(prefs, currency),
                _ => CurrencyFormatter::try_new_symbol(prefs, currency, options),
            };
            Some(made.ok()?.format_fixed_decimal(&decimal).to_string())
        }
        "percent" => {
            let mut decimal = decimal;
            // By a power of ten rather than by `value * 100.0`: the language
            // says a percent is the number times a hundred, and doing it on the
            // decimal keeps `0.07` at `7` instead of `7.000000000000001`.
            decimal.multiply_pow10(2);
            // The shift keeps the digit at magnitude zero VISIBLE, which for a
            // number that was below one means two leading zeros come with it:
            // `0.256` becomes `025.6` and formats as `026%`. Trimming them is
            // not cosmetic — a leading zero is a digit the locale would have
            // grouped.
            decimal.absolute.trim_start();
            let decimal = fitted(this, decimal, 0);
            let made =
                PercentFormatter::try_new((&locale).into(), PercentFormatterOptions::default());
            Some(made.ok()?.format(&decimal).to_string())
        }
        _ => {
            let decimal = fitted(this, decimal, 3);
            let made =
                DecimalFormatter::try_new((&locale).into(), DecimalFormatterOptions::default());
            Some(made.ok()?.format(&decimal).to_string())
        }
    }
}

/// `Intl.NumberFormat.prototype.format` is an ACCESSOR, and that is not a
/// detail.
///
/// The specification makes it a getter answering a function already BOUND to
/// the formatter, because the way a formatter is used is
/// `values.map(nf.format)` — and a plain method detached from its receiver has
/// no `this` to read the locale from. Written as a method, that call formats
/// with the ROOT locale and answers a list that looks formatted:
/// `[1234.5].map(nf.format)` comes back `1,234.5` under a `pt-BR` formatter,
/// where `pt-BR` writes `1.234,5`. A wrong answer that looks right, which is
/// the whole reason this file exists.
///
/// The formatter travels in the bound function's ENVIRONMENT slot, which is
/// [`super::collate`]'s mechanism and the reason neither needs a side table
/// keyed by cell. Through the context rather than through `define_getter`: a
/// registration runs INSIDE `global_get`'s borrow and `define_getter` is an
/// entry point that takes one of its own, so reaching it from here re-enters
/// the `RefCell` and aborts the process.
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
        // Not a number formatter — `Intl.NumberFormat.prototype.format` read
        // off something else. The language throws here, and so does this: the
        // alternative is formatting with the root locale.
        super::super::throw::type_error(
            "Intl.NumberFormat.prototype.format called on a value that is not a number format",
        );
        return with_current(|context| super::super::objects::undefined_of(context));
    }
    let asked = super::super::class_support::to_number(value);
    let Some(text) = written(formatter, asked) else {
        return with_current(|context| super::super::objects::undefined_of(context));
    };
    with_current(|context| {
        context
            .intern_value(crate::text::Str::from_str(&text))
            .bits()
    })
}

/// Installs `Intl.NumberFormat` with its accessor.
///
/// A wrapper around the registration the attribute derives, for the reason
/// [`super::collate`]'s wrapper exists: the attribute installs METHODS, and
/// `format` is not one.
pub(super) fn register_number_format_class(context: &mut Context) -> u64 {
    let made = register_number_format(context);
    if let Some(prototype) = super::super::class_support::prototype(context, "NumberFormat") {
        install_format(context, prototype);
    }
    made
}
