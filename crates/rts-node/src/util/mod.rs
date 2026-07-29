//! `node:util` — the clean, self-contained utility surface: `format`/
//! `formatWithOptions` (printf), `isDeepStrictEqual`, `stripVTControlCharacters`,
//! `toUSVString`, `getSystemErrorName`, `styleText`. Real implementations.
//!
//! Deferred (need the full inspect renderer / the async subsystem / a
//! sub-object-of-predicates or separate-specifier layer): `inspect` (the deep
//! object renderer — `%o`/`%O`/`%s`-of-object here fall back to `String(value)`),
//! `types.*` (the ~40 type predicates), `promisify`/`callbackify`,
//! `parseArgs`/`parseEnv`, `debuglog`, `deprecate`, `inherits`, `MIMEType`.
//! The printf + comparison + string-utility surface is implemented.
//!
//! Module layout: `words` (ToString bridge + string utilities), `format`
//! (printf), `symbols` (`#[rtse::function]`-declared entry points), `mod`
//! (registration).
//!
//! `formatHex`/`formatBin`/`formatOct`/`parseInt` are compat aliases that reuse
//! the runtime `__RTS_FN_NS_FMT_*` externs BY SYMBOL (a fn defined in the `fmt`
//! namespace, another module) — `#[rtse::function]` derives a fresh linker
//! symbol from ITS OWN Rust fn name, so it cannot spell an existing foreign
//! symbol; these four rows stay hand-declared `Member`s.

mod format;
mod inspect;
mod json;
mod parseargs;
mod symbols;
mod words;

use rts_engine::AbiType::{self, Handle, I64, StrPtr};
use rts_engine::{Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

// The fmt-backed compat helpers (formatHex/Bin/Oct + parseInt) reuse the
// runtime `__RTS_FN_NS_FMT_*` externs (reached by symbol at link), so this
// canonical node:util module carries them alongside format/isDeepStrictEqual/…
// (no separate registrar to shadow it).
unsafe extern "C" {
    fn __RTS_FN_NS_FMT_FMT_HEX(v: i64) -> u64;
    fn __RTS_FN_NS_FMT_FMT_BIN(v: i64) -> u64;
    fn __RTS_FN_NS_FMT_FMT_OCT(v: i64) -> u64;
    fn __RTS_FN_NS_FMT_PARSE_I64(p: *const u8, l: i64) -> i64;
}

fn f(name: &str, symbol: &str, args: Vec<AbiType>, ret: AbiType, ts: &str, fp: *const u8) -> Member {
    Member {
        name: name.to_string(),
        kind: MemberKind::Function,
        sig: Sig::new(args, ret),
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: String::new(),
        ret_class: None,
        pure: false,
        emit: None,
    }
}

/// Registers the `node:util` surface.
pub fn register(e: &mut Engine) {
    register_named(e, "node:util");
}

/// Registers `node:sys` — the deprecated exact alias of `node:util` (Node's
/// `sys` module is literally `module.exports = require('util')`), so it gets the
/// identical member set.
pub fn register_sys(e: &mut Engine) {
    register_named(e, "node:sys");
}

fn register_named(e: &mut Engine, ns_name: &str) {
    use symbols::{
        format0_entry, format1_entry, format2_entry, format3_entry, format4_entry,
        format_opts0_entry, format_opts1_entry, format_opts2_entry, get_system_error_name_entry,
        inspect_value_entry, inspect_value_opts_entry, is_deep_strict_equal_entry,
        strip_vt_control_characters_entry, style_text_fn_entry, to_usv_string_fn_entry,
    };

    e.module(ns_name, |m| {
        m.doc(
            "Utilities: format/formatWithOptions, isDeepStrictEqual, \
             stripVTControlCharacters, toUSVString, getSystemErrorName, styleText.",
        );
        m.registry(format0_entry());
        m.registry(format1_entry());
        m.registry(format2_entry());
        m.registry(format3_entry());
        m.registry(format4_entry());
        m.registry(format_opts0_entry());
        m.registry(format_opts1_entry());
        m.registry(format_opts2_entry());
        m.registry(parseargs::parse_args_entry());
        m.registry(inspect_value_entry());
        m.registry(inspect_value_opts_entry());
        m.registry(is_deep_strict_equal_entry());
        m.registry(strip_vt_control_characters_entry());
        m.registry(to_usv_string_fn_entry());
        m.registry(get_system_error_name_entry());
        m.registry(style_text_fn_entry());
        // fmt-backed compat helpers (formatHex/Bin/Oct + parseInt) — hand
        // declared: the symbol names a fn defined in the `fmt` namespace, not
        // this module, which `#[rtse::function]` cannot spell (it derives the
        // symbol from its own Rust fn name).
        m.member(f("formatHex", "__RTS_FN_NS_FMT_FMT_HEX", vec![I64], Handle, "formatHex(value: number): string", __RTS_FN_NS_FMT_FMT_HEX as *const u8));
        m.member(f("formatBin", "__RTS_FN_NS_FMT_FMT_BIN", vec![I64], Handle, "formatBin(value: number): string", __RTS_FN_NS_FMT_FMT_BIN as *const u8));
        m.member(f("formatOct", "__RTS_FN_NS_FMT_FMT_OCT", vec![I64], Handle, "formatOct(value: number): string", __RTS_FN_NS_FMT_FMT_OCT as *const u8));
        m.member(f("parseInt", "__RTS_FN_NS_FMT_PARSE_I64", vec![StrPtr], I64, "parseInt(s: string): number", __RTS_FN_NS_FMT_PARSE_I64 as *const u8));
    });
}
