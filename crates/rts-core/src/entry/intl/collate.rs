//! `Intl.Collator` — comparing strings the way a language orders them.
//!
//! Not `<` over code units. `"ä"` sorts beside `"a"` in German and after `"z"`
//! in Swedish, and `"alpha"` equals `"ALPHA"` only when the caller asked for a
//! sensitivity that ignores case. Every one of those is CLDR's answer.

use icu::collator::options::{CollatorOptions, Strength};
use icu::collator::{Collator as Sorter, CollatorBorrowed};

use super::super::{Context, with_current};
use crate::value::Value;

/// `Intl.Collator`.
#[rtse::class("Collator")]
impl Collator {
    /// `new Intl.Collator(locales, options)`.
    ///
    /// `sensitivity` is the option programs actually reach for, and it maps
    /// onto the collation STRENGTH: `base` ignores accents and case, `accent`
    /// keeps accents, `case` keeps case, and `variant` — the default — keeps
    /// both.
    #[construct]
    fn build(this: u64, locales: u64, options: u64) -> u64 {
        let tags = super::requested(locales);
        let selected = super::selected(&tags);
        let sensitivity = super::option(options, "sensitivity").unwrap_or_else(|| "variant".to_owned());
        let locale_text = selected.to_string();
        if sorter(&selected, &sensitivity).is_none() {
            super::super::throw::range_error(&format!(
                "Intl.Collator: no collation data for {locale_text}"
            ));
            return with_current(|context| super::super::objects::undefined_of(context));
        }
        with_current(|context| {
            let locale_value = context
                .intern_value(crate::text::Str::from_str(&locale_text))
                .bits();
            let sensitivity_value = context
                .intern_value(crate::text::Str::from_str(&sensitivity))
                .bits();
            super::resolved(
                context,
                this,
                "Collator",
                &[
                    ("locale", locale_value),
                    ("sensitivity", sensitivity_value),
                ],
            )
        })
    }

    /// `coll.resolvedOptions()`.
    #[js("resolvedOptions")]
    fn resolved_options(this: u64) -> u64 {
        super::stored(this)
    }
}

/// The collator for a locale at a sensitivity.
///
/// Borrowed rather than owned, which is what `try_new` answers when the data is
/// compiled in: the tables are in the binary, so a collator over them borrows.
fn sorter(locale: &icu::locale::Locale, sensitivity: &str) -> Option<CollatorBorrowed<'static>> {
    let strength = match sensitivity {
        "base" => Strength::Primary,
        "accent" => Strength::Secondary,
        "case" => Strength::Primary,
        _ => Strength::Tertiary,
    };
    let mut options = CollatorOptions::default();
    options.strength = Some(strength);
    Sorter::try_new(locale.into(), options).ok()
}

/// A value's text, when it genuinely is a string.
///
/// A non-string argument is `ToString`'d by the specification, and that
/// conversion calls user code — so it belongs at the call site rather than
/// inside this read. Named as the limit it is.
fn text_of(value: u64) -> Option<String> {
    with_current(|context| {
        let cell = Value(value).as_slot()?;
        context.text_at(cell)?.to_rust()
    })
}

/// `Intl.Collator.prototype.compare` is an ACCESSOR, and that is not a detail.
///
/// The specification makes it a getter answering a function already BOUND to
/// the collator, because the way a collator is used is `list.sort(coll.compare)`
/// — and a plain method detached from its receiver has no `this` to read the
/// locale from. Written as a method, that call compared with the root locale
/// and answered a list that LOOKED sorted: `["ä", "z", "a"]` came back
/// `a,ä,z` under a Swedish collator, where Swedish orders `ä` after `z`. A
/// wrong answer that looks right, which is the whole reason this file exists
/// rather than a `<` over code units.
///
/// The collator travels in the bound function's ENVIRONMENT slot — the
/// mechanism `proxy::revocable` uses for the same shape of problem, and the
/// reason neither needs a side table keyed by cell.
/// Through the context rather than through `define_getter`, and that is not a
/// style choice: a registration runs INSIDE `global_get`'s borrow, and
/// `define_getter` is an entry point that takes one of its own. Reaching it
/// from here re-enters the `RefCell` and aborts the process — measured, with
/// the backtrace naming both borrows.
fn install_compare(context: &mut Context, prototype: u64) {
    let key = super::super::modules::member_key(context, "compare");
    let getter = super::super::native::callable(context, bound_compare as super::super::native::Native);
    super::super::native::name_of(context, getter, "get compare");
    if let Some(cell) = Value(prototype).as_slot() {
        context.set_accessor(cell, key, Some(getter), None);
    }
}

/// The getter: a function that already knows which collator it compares with.
///
/// A fresh one per read, which is what the specification's own wording allows
/// and what keeps this from needing anywhere to cache one. `coll.compare` twice
/// answers two functions; the language does not promise otherwise for this
/// accessor.
extern "C" fn bound_compare(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    with_current(|context| {
        let code = compare_with as super::super::native::Native;
        let made = super::super::native::callable(context, code);
        if let Some(cell) = Value(made).as_slot() {
            // The receiver becomes the ENVIRONMENT, which is what makes the
            // answer survive being detached.
            context.mark_callable(cell, code as usize as u64, this);
        }
        super::super::native::name_of(context, made, "");
        made
    })
}

/// The bound function itself: the collator arrives as the environment.
extern "C" fn compare_with(collator: u64, _this: u64, left: u64, right: u64, _a2: u64, _a3: u64) -> u64 {
    let Some(tag) = super::stored_text(collator, "locale") else {
        // Not a collator at all — `Intl.Collator.prototype.compare` read off
        // something else. The language throws here, and so does this: the
        // alternative is comparing with the root locale and answering a list
        // that looks sorted.
        super::super::throw::type_error(
            "Intl.Collator.prototype.compare called on a value that is not a collator",
        );
        return Value::from_f64(0.0).bits();
    };
    let locale = super::locale::parse(Some(&tag));
    let sensitivity = super::stored_text(collator, "sensitivity").unwrap_or_default();
    let Some(sorter) = sorter(&locale, &sensitivity) else {
        return Value::from_f64(0.0).bits();
    };
    let (Some(left), Some(right)) = (text_of(left), text_of(right)) else {
        return Value::from_f64(0.0).bits();
    };
    let answer = match sorter.compare(&left, &right) {
        core::cmp::Ordering::Less => -1.0,
        core::cmp::Ordering::Equal => 0.0,
        core::cmp::Ordering::Greater => 1.0,
    };
    Value::from_f64(answer).bits()
}

/// Installs `Intl.Collator` with its accessor.
///
/// A wrapper around the registration the attribute derives, for the reason
/// `super`'s own wrapper exists: the attribute installs METHODS, and `compare`
/// is not one.
pub(super) fn register_collator_class(context: &mut Context) -> u64 {
    let made = register_collator(context);
    if let Some(prototype) = super::super::class_support::prototype(context, "Collator") {
        install_compare(context, prototype);
    }
    made
}
