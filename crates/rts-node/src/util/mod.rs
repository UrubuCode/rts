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
//! (printf), `symbols` (extern points), `mod` (registration).

mod format;
mod inspect;
mod symbols;
mod words;

use rts_engine::AbiType::{self, Bool, Handle, I64, PolyValue, StrPtr};
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
        pure: false,
        intrinsic: None,
    }
}

/// Registers the `node:util` surface.
pub fn register(e: &mut Engine) {
    use symbols as s;
    let mut ns = e.ns("node:util").doc(
        "Utilities (node:util): format/formatWithOptions, isDeepStrictEqual, \
         stripVTControlCharacters, toUSVString, getSystemErrorName, styleText.",
    );

    // format / formatWithOptions — variadic → fixed-arity overloads (0..4).
    let format_syms: [(&str, *const u8); 5] = [
        ("__RTS_FN_NODE_UTIL_FORMAT0", s::__RTS_FN_NODE_UTIL_FORMAT0 as *const u8),
        ("__RTS_FN_NODE_UTIL_FORMAT1", s::__RTS_FN_NODE_UTIL_FORMAT1 as *const u8),
        ("__RTS_FN_NODE_UTIL_FORMAT2", s::__RTS_FN_NODE_UTIL_FORMAT2 as *const u8),
        ("__RTS_FN_NODE_UTIL_FORMAT3", s::__RTS_FN_NODE_UTIL_FORMAT3 as *const u8),
        ("__RTS_FN_NODE_UTIL_FORMAT4", s::__RTS_FN_NODE_UTIL_FORMAT4 as *const u8),
    ];
    for (n, (symbol, fp)) in format_syms.iter().enumerate() {
        let mut args = vec![StrPtr];
        args.extend(std::iter::repeat_n(PolyValue, n));
        ns = ns.member(f("format", symbol, args, Handle, "format(fmt: string, ...args: object[]): string", *fp));
    }

    ns = ns.member(f(
        "inspect",
        "__RTS_FN_NODE_UTIL_INSPECT",
        vec![PolyValue],
        Handle,
        "inspect(value: object): string",
        s::__RTS_FN_NODE_UTIL_INSPECT as *const u8,
    ));

    ns.member(f(
        "isDeepStrictEqual",
        "__RTS_FN_NODE_UTIL_IS_DEEP_STRICT_EQUAL",
        vec![PolyValue, PolyValue],
        Bool,
        "isDeepStrictEqual(a: object, b: object): boolean",
        s::__RTS_FN_NODE_UTIL_IS_DEEP_STRICT_EQUAL as *const u8,
    ))
    .member(f(
        "stripVTControlCharacters",
        "__RTS_FN_NODE_UTIL_STRIP_VT",
        vec![StrPtr],
        Handle,
        "stripVTControlCharacters(str: string): string",
        s::__RTS_FN_NODE_UTIL_STRIP_VT as *const u8,
    ))
    .member(f(
        "toUSVString",
        "__RTS_FN_NODE_UTIL_TO_USV_STRING",
        vec![StrPtr],
        Handle,
        "toUSVString(str: string): string",
        s::__RTS_FN_NODE_UTIL_TO_USV_STRING as *const u8,
    ))
    .member(f(
        "getSystemErrorName",
        "__RTS_FN_NODE_UTIL_GET_SYSTEM_ERROR_NAME",
        vec![I64],
        Handle,
        "getSystemErrorName(err: number): string",
        s::__RTS_FN_NODE_UTIL_GET_SYSTEM_ERROR_NAME as *const u8,
    ))
    .member(f(
        "styleText",
        "__RTS_FN_NODE_UTIL_STYLE_TEXT",
        vec![StrPtr, StrPtr],
        Handle,
        "styleText(format: string, text: string): string",
        s::__RTS_FN_NODE_UTIL_STYLE_TEXT as *const u8,
    ))
    // fmt-backed compat helpers (formatHex/Bin/Oct + parseInt).
    .member(f("formatHex", "__RTS_FN_NS_FMT_FMT_HEX", vec![I64], Handle, "formatHex(value: number): string", __RTS_FN_NS_FMT_FMT_HEX as *const u8))
    .member(f("formatBin", "__RTS_FN_NS_FMT_FMT_BIN", vec![I64], Handle, "formatBin(value: number): string", __RTS_FN_NS_FMT_FMT_BIN as *const u8))
    .member(f("formatOct", "__RTS_FN_NS_FMT_FMT_OCT", vec![I64], Handle, "formatOct(value: number): string", __RTS_FN_NS_FMT_FMT_OCT as *const u8))
    .member(f("parseInt", "__RTS_FN_NS_FMT_PARSE_I64", vec![StrPtr], I64, "parseInt(s: string): number", __RTS_FN_NS_FMT_PARSE_I64 as *const u8))
    .done();
}
