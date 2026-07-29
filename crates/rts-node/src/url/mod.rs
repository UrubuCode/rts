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

use rts_engine::Engine;

/// Registers the `node:url` function surface.
pub fn register(e: &mut Engine) {
    e.module("node:url", |m| {
        m.doc(
            "URL utilities (node:url): domainToASCII/domainToUnicode (UTS-46), \
             fileURLToPath/fileURLToPathBuffer/pathToFileURL, urlToHttpOptions, \
             and the legacy parse/format/resolve. URL/URLSearchParams are engine \
             globals.",
        );
        m.registry(symbols::domain_to_ascii_entry());
        m.registry(symbols::domain_to_unicode_entry());
        m.registry(symbols::file_url_to_path_entry());
        m.registry(symbols::file_url_to_path_buffer_entry());
        m.registry(symbols::path_to_file_url_entry());
        m.registry(symbols::url_to_http_options_entry());
        m.registry(symbols::resolve_entry());
        m.registry(symbols::parse_entry());
        m.registry(symbols::format_entry());
    });
}
