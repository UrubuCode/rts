//! Intl.* global classes — ABI registration as composite-name GlobalClassSpecs.
//!
//! Registered under composite names ("Intl.NumberFormat", etc.) so codegen can
//! resolve `new Intl.NumberFormat(...)` via `global_class_lookup("Intl.NumberFormat")`
//! and dispatch instance methods through the standard global-class path.
//!
//! All constructors share the signature `(locale: StrPtr, options: Handle)`.

use crate::abi::{AbiType, GlobalClassSpec, MemberKind, NamespaceMember};

const fn ctor(symbol: &'static str, sig: &'static str) -> NamespaceMember {
    NamespaceMember {
        name: "new",
        kind: MemberKind::Constructor,
        symbol,
        args: &[AbiType::StrPtr, AbiType::Handle],
        returns: AbiType::Handle,
        doc: "Intl constructor (hardcoded en behaviour, no ICU).",
        ts_signature: sig,
        intrinsic: None,
        pure: false,
    }
}

// ── Intl.NumberFormat ───────────────────────────────────────────────────────

pub const NUMBER_FORMAT_MEMBERS: &[NamespaceMember] = &[
    ctor(
        "__RTS_FN_GL_INTL_NUMBER_FORMAT_NEW",
        "new Intl.NumberFormat(locale?: string, options?: object): Intl.NumberFormat",
    ),
    NamespaceMember {
        name: "format",
        kind: MemberKind::InstanceMethod,
        symbol: "__RTS_FN_GL_INTL_NUMBER_FORMAT_FORMAT",
        args: &[AbiType::Handle, AbiType::F64],
        returns: AbiType::Handle,
        doc: "Formats a number according to the configured options.",
        ts_signature: "format(value: number): string",
        intrinsic: None,
        pure: true,
    },
];

pub const NUMBER_FORMAT_CLASS_SPEC: GlobalClassSpec = GlobalClassSpec {
    name: "Intl.NumberFormat",
    doc: "Intl.NumberFormat (en, no ICU).",
    members: NUMBER_FORMAT_MEMBERS,
};

// ── Intl.DateTimeFormat ─────────────────────────────────────────────────────

pub const DATE_TIME_FORMAT_MEMBERS: &[NamespaceMember] = &[
    ctor(
        "__RTS_FN_GL_INTL_DATE_TIME_FORMAT_NEW",
        "new Intl.DateTimeFormat(locale?: string, options?: object): Intl.DateTimeFormat",
    ),
    NamespaceMember {
        name: "format",
        kind: MemberKind::InstanceMethod,
        symbol: "__RTS_FN_GL_INTL_DATE_TIME_FORMAT_FORMAT",
        args: &[AbiType::Handle, AbiType::Handle],
        returns: AbiType::Handle,
        doc: "Formats a Date according to the configured options.",
        ts_signature: "format(date: Date): string",
        intrinsic: None,
        pure: true,
    },
];

pub const DATE_TIME_FORMAT_CLASS_SPEC: GlobalClassSpec = GlobalClassSpec {
    name: "Intl.DateTimeFormat",
    doc: "Intl.DateTimeFormat (en, no ICU).",
    members: DATE_TIME_FORMAT_MEMBERS,
};

// ── Intl.Collator ────────────────────────────────────────────────────────────

pub const COLLATOR_MEMBERS: &[NamespaceMember] = &[
    ctor(
        "__RTS_FN_GL_INTL_COLLATOR_NEW",
        "new Intl.Collator(locale?: string, options?: object): Intl.Collator",
    ),
    NamespaceMember {
        name: "compare",
        kind: MemberKind::InstanceMethod,
        symbol: "__RTS_FN_GL_INTL_COLLATOR_COMPARE",
        args: &[AbiType::Handle, AbiType::StrPtr, AbiType::StrPtr],
        returns: AbiType::I64,
        doc: "Compares two strings; returns -1/0/1.",
        ts_signature: "compare(a: string, b: string): number",
        intrinsic: None,
        pure: true,
    },
];

pub const COLLATOR_CLASS_SPEC: GlobalClassSpec = GlobalClassSpec {
    name: "Intl.Collator",
    doc: "Intl.Collator (en, no ICU).",
    members: COLLATOR_MEMBERS,
};

// ── Intl.Segmenter ───────────────────────────────────────────────────────────

pub const SEGMENTER_MEMBERS: &[NamespaceMember] = &[
    ctor(
        "__RTS_FN_GL_INTL_SEGMENTER_NEW",
        "new Intl.Segmenter(locale?: string, options?: object): Intl.Segmenter",
    ),
    NamespaceMember {
        name: "segment",
        kind: MemberKind::InstanceMethod,
        symbol: "__RTS_FN_GL_INTL_SEGMENTER_SEGMENT",
        args: &[AbiType::Handle, AbiType::StrPtr],
        returns: AbiType::Handle,
        doc: "Segments a string into an iterable of {segment, isWordLike} objects.",
        ts_signature: "segment(input: string): Iterable<{segment: string; isWordLike: boolean}>",
        intrinsic: None,
        pure: true,
    },
];

pub const SEGMENTER_CLASS_SPEC: GlobalClassSpec = GlobalClassSpec {
    name: "Intl.Segmenter",
    doc: "Intl.Segmenter (en, no ICU).",
    members: SEGMENTER_MEMBERS,
};

// ── Intl.PluralRules ─────────────────────────────────────────────────────────

pub const PLURAL_RULES_MEMBERS: &[NamespaceMember] = &[
    ctor(
        "__RTS_FN_GL_INTL_PLURAL_RULES_NEW",
        "new Intl.PluralRules(locale?: string, options?: object): Intl.PluralRules",
    ),
    NamespaceMember {
        name: "select",
        kind: MemberKind::InstanceMethod,
        symbol: "__RTS_FN_GL_INTL_PLURAL_RULES_SELECT",
        args: &[AbiType::Handle, AbiType::F64],
        returns: AbiType::Handle,
        doc: "Returns the plural category (\"one\"/\"other\").",
        ts_signature: "select(n: number): string",
        intrinsic: None,
        pure: true,
    },
];

pub const PLURAL_RULES_CLASS_SPEC: GlobalClassSpec = GlobalClassSpec {
    name: "Intl.PluralRules",
    doc: "Intl.PluralRules (en, no ICU).",
    members: PLURAL_RULES_MEMBERS,
};

// ── Intl.ListFormat ──────────────────────────────────────────────────────────

pub const LIST_FORMAT_MEMBERS: &[NamespaceMember] = &[
    ctor(
        "__RTS_FN_GL_INTL_LIST_FORMAT_NEW",
        "new Intl.ListFormat(locale?: string, options?: object): Intl.ListFormat",
    ),
    NamespaceMember {
        name: "format",
        kind: MemberKind::InstanceMethod,
        symbol: "__RTS_FN_GL_INTL_LIST_FORMAT_FORMAT",
        args: &[AbiType::Handle, AbiType::Handle],
        returns: AbiType::Handle,
        doc: "Joins a list of strings into a localized list.",
        ts_signature: "format(items: string[]): string",
        intrinsic: None,
        pure: true,
    },
];

pub const LIST_FORMAT_CLASS_SPEC: GlobalClassSpec = GlobalClassSpec {
    name: "Intl.ListFormat",
    doc: "Intl.ListFormat (en, no ICU).",
    members: LIST_FORMAT_MEMBERS,
};

// ── Intl.RelativeTimeFormat ──────────────────────────────────────────────────

pub const RELATIVE_TIME_FORMAT_MEMBERS: &[NamespaceMember] = &[
    ctor(
        "__RTS_FN_GL_INTL_RELATIVE_TIME_FORMAT_NEW",
        "new Intl.RelativeTimeFormat(locale?: string, options?: object): Intl.RelativeTimeFormat",
    ),
    NamespaceMember {
        name: "format",
        kind: MemberKind::InstanceMethod,
        symbol: "__RTS_FN_GL_INTL_RELATIVE_TIME_FORMAT_FORMAT",
        args: &[AbiType::Handle, AbiType::F64, AbiType::StrPtr],
        returns: AbiType::Handle,
        doc: "Formats a relative time (e.g. -1, \"day\" -> \"yesterday\").",
        ts_signature: "format(value: number, unit: string): string",
        intrinsic: None,
        pure: true,
    },
];

pub const RELATIVE_TIME_FORMAT_CLASS_SPEC: GlobalClassSpec = GlobalClassSpec {
    name: "Intl.RelativeTimeFormat",
    doc: "Intl.RelativeTimeFormat (en, no ICU).",
    members: RELATIVE_TIME_FORMAT_MEMBERS,
};
