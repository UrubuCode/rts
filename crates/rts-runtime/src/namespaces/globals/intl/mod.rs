//! `Intl.*` global classes (en, no ICU). Migrado ao modelo `#[rts_class]`
//! (stage 5) via membros `external` — os externs `__RTS_FN_GL_INTL_*` ficam em
//! `instance.rs` intactos; o macro deriva apenas os 7 `*_CLASS_SPEC`. Nomes de
//! classe compostos ("Intl.NumberFormat") via `name = "..."`.

pub mod instance;

// All members are `external` (externs live in instance.rs); the type tokens
// live only in the macro-consumed stubs.
#[allow(unused_imports)]
use rts_engine::abi::ty::{Handle, F64, I64};
use rts_macro::rts_class;

/// Intl.NumberFormat (en, no ICU).
#[rts_class(
    IntlNumberFormat,
    name = "Intl.NumberFormat",
    prefix = "INTL_NUMBER_FORMAT",
    spec = "NUMBER_FORMAT_CLASS_SPEC"
)]
impl IntlNumberFormatClass {
    #[rts_ctor(
        external,
        symbol = "__RTS_FN_GL_INTL_NUMBER_FORMAT_NEW",
        ts = "new Intl.NumberFormat(locale?: string, options?: object): Intl.NumberFormat"
    )]
    pub fn new(_locale: Str, _options: Handle) -> Handle {
        unreachable!()
    }
    #[rts_method(
        external,
        name = "format",
        symbol = "__RTS_FN_GL_INTL_NUMBER_FORMAT_FORMAT",
        ts = "format(value: number): string",
        pure
    )]
    pub fn format(_h: Handle, _value: F64) -> Handle {
        unreachable!()
    }
}

/// Intl.DateTimeFormat (en, no ICU).
#[rts_class(
    IntlDateTimeFormat,
    name = "Intl.DateTimeFormat",
    prefix = "INTL_DATE_TIME_FORMAT",
    spec = "DATE_TIME_FORMAT_CLASS_SPEC"
)]
impl IntlDateTimeFormatClass {
    #[rts_ctor(
        external,
        symbol = "__RTS_FN_GL_INTL_DATE_TIME_FORMAT_NEW",
        ts = "new Intl.DateTimeFormat(locale?: string, options?: object): Intl.DateTimeFormat"
    )]
    pub fn new(_locale: Str, _options: Handle) -> Handle {
        unreachable!()
    }
    #[rts_method(
        external,
        name = "format",
        symbol = "__RTS_FN_GL_INTL_DATE_TIME_FORMAT_FORMAT",
        ts = "format(date: Date): string",
        pure
    )]
    pub fn format(_h: Handle, _date: Handle) -> Handle {
        unreachable!()
    }
}

/// Intl.Collator (en, no ICU).
#[rts_class(
    IntlCollator,
    name = "Intl.Collator",
    prefix = "INTL_COLLATOR",
    spec = "COLLATOR_CLASS_SPEC"
)]
impl IntlCollatorClass {
    #[rts_ctor(
        external,
        symbol = "__RTS_FN_GL_INTL_COLLATOR_NEW",
        ts = "new Intl.Collator(locale?: string, options?: object): Intl.Collator"
    )]
    pub fn new(_locale: Str, _options: Handle) -> Handle {
        unreachable!()
    }
    #[rts_method(
        external,
        name = "compare",
        symbol = "__RTS_FN_GL_INTL_COLLATOR_COMPARE",
        ts = "compare(a: string, b: string): number",
        pure
    )]
    pub fn compare(_h: Handle, _a: Str, _b: Str) -> I64 {
        unreachable!()
    }
}

/// Intl.Segmenter (en, no ICU).
#[rts_class(
    IntlSegmenter,
    name = "Intl.Segmenter",
    prefix = "INTL_SEGMENTER",
    spec = "SEGMENTER_CLASS_SPEC"
)]
impl IntlSegmenterClass {
    #[rts_ctor(
        external,
        symbol = "__RTS_FN_GL_INTL_SEGMENTER_NEW",
        ts = "new Intl.Segmenter(locale?: string, options?: object): Intl.Segmenter"
    )]
    pub fn new(_locale: Str, _options: Handle) -> Handle {
        unreachable!()
    }
    #[rts_method(
        external,
        name = "segment",
        symbol = "__RTS_FN_GL_INTL_SEGMENTER_SEGMENT",
        ts = "segment(input: string): Iterable<{segment: string; isWordLike: boolean}>",
        pure
    )]
    pub fn segment(_h: Handle, _input: Str) -> Handle {
        unreachable!()
    }
}

/// Intl.PluralRules (en, no ICU).
#[rts_class(
    IntlPluralRules,
    name = "Intl.PluralRules",
    prefix = "INTL_PLURAL_RULES",
    spec = "PLURAL_RULES_CLASS_SPEC"
)]
impl IntlPluralRulesClass {
    #[rts_ctor(
        external,
        symbol = "__RTS_FN_GL_INTL_PLURAL_RULES_NEW",
        ts = "new Intl.PluralRules(locale?: string, options?: object): Intl.PluralRules"
    )]
    pub fn new(_locale: Str, _options: Handle) -> Handle {
        unreachable!()
    }
    #[rts_method(
        external,
        name = "select",
        symbol = "__RTS_FN_GL_INTL_PLURAL_RULES_SELECT",
        ts = "select(n: number): string",
        pure
    )]
    pub fn select(_h: Handle, _n: F64) -> Handle {
        unreachable!()
    }
}

/// Intl.ListFormat (en, no ICU).
#[rts_class(
    IntlListFormat,
    name = "Intl.ListFormat",
    prefix = "INTL_LIST_FORMAT",
    spec = "LIST_FORMAT_CLASS_SPEC"
)]
impl IntlListFormatClass {
    #[rts_ctor(
        external,
        symbol = "__RTS_FN_GL_INTL_LIST_FORMAT_NEW",
        ts = "new Intl.ListFormat(locale?: string, options?: object): Intl.ListFormat"
    )]
    pub fn new(_locale: Str, _options: Handle) -> Handle {
        unreachable!()
    }
    #[rts_method(
        external,
        name = "format",
        symbol = "__RTS_FN_GL_INTL_LIST_FORMAT_FORMAT",
        ts = "format(items: string[]): string",
        pure
    )]
    pub fn format(_h: Handle, _items: Handle) -> Handle {
        unreachable!()
    }
}

/// Intl.RelativeTimeFormat (en, no ICU).
#[rts_class(
    IntlRelativeTimeFormat,
    name = "Intl.RelativeTimeFormat",
    prefix = "INTL_RELATIVE_TIME_FORMAT",
    spec = "RELATIVE_TIME_FORMAT_CLASS_SPEC"
)]
impl IntlRelativeTimeFormatClass {
    #[rts_ctor(
        external,
        symbol = "__RTS_FN_GL_INTL_RELATIVE_TIME_FORMAT_NEW",
        ts = "new Intl.RelativeTimeFormat(locale?: string, options?: object): Intl.RelativeTimeFormat"
    )]
    pub fn new(_locale: Str, _options: Handle) -> Handle {
        unreachable!()
    }
    #[rts_method(
        external,
        name = "format",
        symbol = "__RTS_FN_GL_INTL_RELATIVE_TIME_FORMAT_FORMAT",
        ts = "format(value: number, unit: string): string",
        pure
    )]
    pub fn format(_h: Handle, _value: F64, _unit: Str) -> Handle {
        unreachable!()
    }
}
