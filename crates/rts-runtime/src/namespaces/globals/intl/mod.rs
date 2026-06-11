//! `Intl.*` global classes (en, no ICU). Migrado do `#[rts_class]` (macro) pro
//! modelo builder hand-written do `rts-engine` (rumo à remoção da `rts-macro`).
//! Todos os membros são `external` — os externs `__RTS_FN_GL_INTL_*` vivem em
//! `instance.rs` intactos; aqui só montamos os 7 `register_*_class_spec`. Nomes
//! de classe compostos ("Intl.NumberFormat") via `e.class("...")`.

pub mod instance;

use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

/// Membro de classe global `external` (fn_ptr nulo — o extern vive em
/// `instance.rs`). Espelha o helper hand-written do `boolean/mod.rs`, adaptado
/// pra membros sem `fn_ptr` próprio.
fn m(name: &str, kind: MemberKind, sig: Sig, symbol: &str, ts: &str, pure: bool) -> Member {
    Member {
        name: name.to_string(),
        kind,
        sig,
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(core::ptr::null::<u8>()),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: String::new(),
        pure,
        intrinsic: None,
    }
}

/// Intl.NumberFormat (en, no ICU) — Fase 2, hand-written, sem macro.
pub fn register_number_format_class_spec(e: &mut Engine) {
    e.class("Intl.NumberFormat")
        .doc("Intl.NumberFormat (en, no ICU).")
        .member(m(
            "new",
            MemberKind::Constructor,
            Sig::new(vec![AbiType::StrPtr, AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_INTL_NUMBER_FORMAT_NEW",
            "new Intl.NumberFormat(locale?: string, options?: object): Intl.NumberFormat",
            false,
        ))
        .member(m(
            "format",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::F64], AbiType::Handle),
            "__RTS_FN_GL_INTL_NUMBER_FORMAT_FORMAT",
            "format(value: number): string",
            true,
        ))
        .done();
}

/// Intl.DateTimeFormat (en, no ICU) — Fase 2, hand-written, sem macro.
pub fn register_date_time_format_class_spec(e: &mut Engine) {
    e.class("Intl.DateTimeFormat")
        .doc("Intl.DateTimeFormat (en, no ICU).")
        .member(m(
            "new",
            MemberKind::Constructor,
            Sig::new(vec![AbiType::StrPtr, AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_INTL_DATE_TIME_FORMAT_NEW",
            "new Intl.DateTimeFormat(locale?: string, options?: object): Intl.DateTimeFormat",
            false,
        ))
        .member(m(
            "format",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_INTL_DATE_TIME_FORMAT_FORMAT",
            "format(date: Date): string",
            true,
        ))
        .done();
}

/// Intl.Collator (en, no ICU) — Fase 2, hand-written, sem macro.
pub fn register_collator_class_spec(e: &mut Engine) {
    e.class("Intl.Collator")
        .doc("Intl.Collator (en, no ICU).")
        .member(m(
            "new",
            MemberKind::Constructor,
            Sig::new(vec![AbiType::StrPtr, AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_INTL_COLLATOR_NEW",
            "new Intl.Collator(locale?: string, options?: object): Intl.Collator",
            false,
        ))
        .member(m(
            "compare",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::StrPtr, AbiType::StrPtr], AbiType::I64),
            "__RTS_FN_GL_INTL_COLLATOR_COMPARE",
            "compare(a: string, b: string): number",
            true,
        ))
        .done();
}

/// Intl.Segmenter (en, no ICU) — Fase 2, hand-written, sem macro.
pub fn register_segmenter_class_spec(e: &mut Engine) {
    e.class("Intl.Segmenter")
        .doc("Intl.Segmenter (en, no ICU).")
        .member(m(
            "new",
            MemberKind::Constructor,
            Sig::new(vec![AbiType::StrPtr, AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_INTL_SEGMENTER_NEW",
            "new Intl.Segmenter(locale?: string, options?: object): Intl.Segmenter",
            false,
        ))
        .member(m(
            "segment",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::StrPtr], AbiType::Handle),
            "__RTS_FN_GL_INTL_SEGMENTER_SEGMENT",
            "segment(input: string): Iterable<{segment: string; isWordLike: boolean}>",
            true,
        ))
        .done();
}

/// Intl.PluralRules (en, no ICU) — Fase 2, hand-written, sem macro.
pub fn register_plural_rules_class_spec(e: &mut Engine) {
    e.class("Intl.PluralRules")
        .doc("Intl.PluralRules (en, no ICU).")
        .member(m(
            "new",
            MemberKind::Constructor,
            Sig::new(vec![AbiType::StrPtr, AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_INTL_PLURAL_RULES_NEW",
            "new Intl.PluralRules(locale?: string, options?: object): Intl.PluralRules",
            false,
        ))
        .member(m(
            "select",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::F64], AbiType::Handle),
            "__RTS_FN_GL_INTL_PLURAL_RULES_SELECT",
            "select(n: number): string",
            true,
        ))
        .done();
}

/// Intl.ListFormat (en, no ICU) — Fase 2, hand-written, sem macro.
pub fn register_list_format_class_spec(e: &mut Engine) {
    e.class("Intl.ListFormat")
        .doc("Intl.ListFormat (en, no ICU).")
        .member(m(
            "new",
            MemberKind::Constructor,
            Sig::new(vec![AbiType::StrPtr, AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_INTL_LIST_FORMAT_NEW",
            "new Intl.ListFormat(locale?: string, options?: object): Intl.ListFormat",
            false,
        ))
        .member(m(
            "format",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_INTL_LIST_FORMAT_FORMAT",
            "format(items: string[]): string",
            true,
        ))
        .done();
}

/// Intl.RelativeTimeFormat (en, no ICU) — Fase 2, hand-written, sem macro.
pub fn register_relative_time_format_class_spec(e: &mut Engine) {
    e.class("Intl.RelativeTimeFormat")
        .doc("Intl.RelativeTimeFormat (en, no ICU).")
        .member(m(
            "new",
            MemberKind::Constructor,
            Sig::new(vec![AbiType::StrPtr, AbiType::Handle], AbiType::Handle),
            "__RTS_FN_GL_INTL_RELATIVE_TIME_FORMAT_NEW",
            "new Intl.RelativeTimeFormat(locale?: string, options?: object): Intl.RelativeTimeFormat",
            false,
        ))
        .member(m(
            "format",
            MemberKind::InstanceMethod,
            Sig::new(vec![AbiType::Handle, AbiType::F64, AbiType::StrPtr], AbiType::Handle),
            "__RTS_FN_GL_INTL_RELATIVE_TIME_FORMAT_FORMAT",
            "format(value: number, unit: string): string",
            true,
        ))
        .done();
}
