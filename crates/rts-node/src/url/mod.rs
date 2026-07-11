//! `node:url` — the utility-function surface over the WHATWG `URL`/
//! `URLSearchParams` classes (which are ambient engine globals): IDNA
//! (`domainToASCII`/`domainToUnicode`, real UTS-46 via `idna`), `file:` URL ⇄
//! path (`fileURLToPath`/`fileURLToPathBuffer`/`pathToFileURL`), the HTTP
//! request-options builder (`urlToHttpOptions`), and the legacy
//! `parse`/`format`/`resolve` API. No stubs — every value is a real transform.
//!
//! `URL`/`URLSearchParams`/`URLPattern` are the engine's WHATWG globals (usable
//! ambiently); this module owns the function surface. `pathToFileURL` returns a
//! real `URL` (constructed via the runtime `URL` ctor), and `fileURLToPath`/
//! `urlToHttpOptions` read a live `URL` through its component getters.
//!
//! Module layout: `whatwg` (IDNA + urlToHttpOptions), `fileurl` (file: ⇄ path),
//! `legacy` (parse/format/resolve), `words` (value build/read + URL externs),
//! `symbols` (extern entry points).

mod fileurl;
mod legacy;
mod symbols;
mod whatwg;
mod words;

use rts_engine::{sig, Engine, FnPtr, Member, MemberFlags, MemberKind};

fn func(name: &str, symbol: &str, sig: rts_engine::Sig, ts: &str, fp: *const u8) -> Member {
    Member {
        name: name.to_string(),
        kind: MemberKind::Function,
        sig,
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

/// Registers the `node:url` function surface.
pub fn register(e: &mut Engine) {
    e.ns("node:url")
        .doc(
            "URL utilities (node:url): domainToASCII/domainToUnicode (UTS-46), \
             fileURLToPath/fileURLToPathBuffer/pathToFileURL, urlToHttpOptions, \
             and the legacy parse/format/resolve. URL/URLSearchParams are engine \
             globals.",
        )
        .member(func(
            "domainToASCII",
            "__RTS_FN_NODE_URL_DOMAIN_TO_ASCII",
            sig!(StrPtr => Handle),
            "domainToASCII(domain: string): string",
            symbols::__RTS_FN_NODE_URL_DOMAIN_TO_ASCII as *const u8,
        ))
        .member(func(
            "domainToUnicode",
            "__RTS_FN_NODE_URL_DOMAIN_TO_UNICODE",
            sig!(StrPtr => Handle),
            "domainToUnicode(domain: string): string",
            symbols::__RTS_FN_NODE_URL_DOMAIN_TO_UNICODE as *const u8,
        ))
        .member(func(
            "fileURLToPath",
            "__RTS_FN_NODE_URL_FILE_URL_TO_PATH",
            sig!(Handle => Handle),
            "fileURLToPath(url: object): string",
            symbols::__RTS_FN_NODE_URL_FILE_URL_TO_PATH as *const u8,
        ))
        .member(func(
            "fileURLToPathBuffer",
            "__RTS_FN_NODE_URL_FILE_URL_TO_PATH_BUFFER",
            sig!(Handle => Handle),
            "fileURLToPathBuffer(url: object): object",
            symbols::__RTS_FN_NODE_URL_FILE_URL_TO_PATH_BUFFER as *const u8,
        ))
        .member(func(
            "pathToFileURL",
            "__RTS_FN_NODE_URL_PATH_TO_FILE_URL",
            sig!(StrPtr => Handle),
            "pathToFileURL(path: string): URL",
            symbols::__RTS_FN_NODE_URL_PATH_TO_FILE_URL as *const u8,
        ))
        .member(func(
            "urlToHttpOptions",
            "__RTS_FN_NODE_URL_TO_HTTP_OPTIONS",
            sig!(Handle => Handle),
            "urlToHttpOptions(url: object): object",
            symbols::__RTS_FN_NODE_URL_TO_HTTP_OPTIONS as *const u8,
        ))
        .member(func(
            "resolve",
            "__RTS_FN_NODE_URL_RESOLVE",
            sig!(StrPtr, StrPtr => Handle),
            "resolve(from: string, to: string): string",
            symbols::__RTS_FN_NODE_URL_RESOLVE as *const u8,
        ))
        .member(func(
            "parse",
            "__RTS_FN_NODE_URL_PARSE",
            sig!(StrPtr => Handle),
            "parse(urlString: string): object",
            symbols::__RTS_FN_NODE_URL_PARSE as *const u8,
        ))
        .member(func(
            "parse",
            "__RTS_FN_NODE_URL_PARSE_OPTS",
            sig!(StrPtr, Bool, Bool => Handle),
            "parse(urlString: string, parseQueryString: boolean, slashesDenoteHost: boolean): object",
            symbols::__RTS_FN_NODE_URL_PARSE_OPTS as *const u8,
        ))
        .member(func(
            "format",
            "__RTS_FN_NODE_URL_FORMAT",
            sig!(Handle => Handle),
            "format(urlObject: object): string",
            symbols::__RTS_FN_NODE_URL_FORMAT as *const u8,
        ))
        .done();
}
