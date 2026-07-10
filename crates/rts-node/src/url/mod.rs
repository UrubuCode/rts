//! `node:url` — real file-URL <-> path conversion + per-label domain
//! ASCII/Unicode (IDNA Bootstring) conversion, Node-accurate. Registration
//! only; the extern "C" implementations live in `symbols.rs` (`promises.rs`
//! is doc-only — this namespace has no promise sub-API).
//!
//! Native rts-node implementation (no rts-std mirror; self-contained, does
//! not depend on `node:punycode` or any other rts-node module — see the
//! doc comment on `symbols.rs`).
//!
//! **Deferred** (need object/class machinery this flat-function slice
//! doesn't have):
//! - The `URL` and `URLSearchParams` classes — already covered as *global*
//!   classes elsewhere (`rts-shared` `globals/url`), not duplicated here.
//! - Legacy `url.parse(urlString)` / `url.format(urlObject)` /
//!   `url.resolve(from, to)` — `parse` returns a Url object, `format` takes
//!   one; no object marshalling in this slice.

mod promises;
mod symbols;

use rts_engine::{sig, Engine, FnPtr, Member, MemberFlags, MemberKind};

fn pure_func(
    name: &str,
    symbol: &str,
    sig: rts_engine::Sig,
    ts: &str,
    doc: &str,
    fp: *const u8,
) -> Member {
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
        doc: doc.to_string(),
        pure: true,
        intrinsic: None,
    }
}

/// Registers the `node:url` surface into the engine Registry. Deliberately
/// NO alias — `node:url` resolves as its own canonical key, distinct from
/// any existing `url`-named surface (e.g. the `URL`/`URLSearchParams` global
/// classes), so the two coexist without colliding.
pub fn register(e: &mut Engine) {
    e.ns("node:url")
        .doc(
            "Real file-URL <-> path conversion + per-label domain ASCII/Unicode \
             (IDNA Bootstring) conversion (node:url). The URL/URLSearchParams \
             classes (objects) and legacy parse/format/resolve (return objects) \
             are deferred — see the module doc comment.",
        )
        .member(pure_func(
            "fileURLToPath",
            "__RTS_FN_NODE_URL_FILE_URL_TO_PATH",
            sig!(StrPtr => Handle),
            "fileURLToPath(url: string): string",
            "Converts a file: URL string to an OS-native path. Returns \"\" (handle 0) for a non-file URL.",
            symbols::__RTS_FN_NODE_URL_FILE_URL_TO_PATH as *const u8,
        ))
        .member(pure_func(
            "pathToFileURL",
            "__RTS_FN_NODE_URL_PATH_TO_FILE_URL",
            sig!(StrPtr => Handle),
            "pathToFileURL(path: string): string",
            "Converts an OS-native path to a file: URL string (percent-encoded).",
            symbols::__RTS_FN_NODE_URL_PATH_TO_FILE_URL as *const u8,
        ))
        .member(pure_func(
            "domainToASCII",
            "__RTS_FN_NODE_URL_DOMAIN_TO_ASCII",
            sig!(StrPtr => Handle),
            "domainToASCII(domain: string): string",
            "Per-label Punycode ASCII (xn--) serialization of a domain; \"\" if invalid.",
            symbols::__RTS_FN_NODE_URL_DOMAIN_TO_ASCII as *const u8,
        ))
        .member(pure_func(
            "domainToUnicode",
            "__RTS_FN_NODE_URL_DOMAIN_TO_UNICODE",
            sig!(StrPtr => Handle),
            "domainToUnicode(domain: string): string",
            "Per-label Unicode serialization of a Punycode (xn--) domain; \"\" if invalid.",
            symbols::__RTS_FN_NODE_URL_DOMAIN_TO_UNICODE as *const u8,
        ))
        .done();
}
