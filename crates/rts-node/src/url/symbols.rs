//! node:url — the free-function entry points wiring the function surface to
//! the `whatwg`/`fileurl`/`legacy` helpers, declared via `#[rtse::function]`
//! (symbol/ABI/TS-signature derived from the Rust signature).

use super::words::{buffer, intern, object_entries, word_scalar};
use super::{fileurl, legacy, whatwg};

use rts_engine::abi::ty::Handle;
use rts_engine::heap::handles::rtse_class_of;

unsafe extern "C" {
    // The shared WHATWG `URL` serializer (globals/url) — recomposes the current
    // href (reflecting setters + searchParams mutations). Returns a string handle.
    fn __rtsm_global_URL_to_string(handle: u64) -> u64;
}

/// A WHATWG `URL` instance is a `#[rtse::class]`-backed `Entry::Rtse` classed
/// `"URL"` (migrated from the old `Entry::Env` slot vector). Legacy urlObjects
/// are shaped `Entry::Vec` plain objects. Used so `url.format` accepts BOTH forms
/// (Node overloads it).
fn is_whatwg_url(handle: u64) -> bool {
    rtse_class_of(handle) == Some("URL")
}

/// `url.domainToASCII(domain)`.
#[rtse::function(module = "node:url", value = "domainToASCII")]
fn domain_to_ascii(domain: &str) -> String {
    whatwg::domain_to_ascii(domain)
}

/// `url.domainToUnicode(domain)`.
#[rtse::function(module = "node:url", value = "domainToUnicode")]
fn domain_to_unicode(domain: &str) -> String {
    whatwg::domain_to_unicode(domain)
}

/// `url.fileURLToPath(url)` — `url` is a URL string or a `URL` instance handle.
#[rtse::function(module = "node:url", value = "fileURLToPath")]
fn file_url_to_path(url: Handle) -> String {
    let bytes = fileurl::file_url_to_path_bytes(url);
    String::from_utf8_lossy(&bytes).into_owned()
}

/// `url.fileURLToPathBuffer(url)` → `Buffer` of the raw path bytes.
#[rtse::function(module = "node:url", value = "fileURLToPathBuffer")]
fn file_url_to_path_buffer(url: Handle) -> Handle {
    buffer(fileurl::file_url_to_path_bytes(url))
}

/// `url.pathToFileURL(path)` → a real `URL`.
#[rtse::function(module = "node:url", value = "pathToFileURL", ret_ts = "URL")]
fn path_to_file_url(path: &str) -> Handle {
    fileurl::path_to_file_url(path)
}

/// `url.urlToHttpOptions(url)` → the request-options object.
#[rtse::function(module = "node:url", value = "urlToHttpOptions")]
fn url_to_http_options(url: Handle) -> Handle {
    whatwg::url_to_http_options(url)
}

/// `url.resolve(from, to)`.
#[rtse::function(module = "node:url", value = "resolve")]
fn resolve(from: &str, to: &str) -> String {
    legacy::resolve(from, to)
}

/// `url.parse(urlString[, parseQueryString[, slashesDenoteHost]])`.
#[rtse::function(module = "node:url", value = "parse")]
fn parse(url_string: &str, #[default(false)] parse_query_string: bool, #[default(false)] slashes_denote_host: bool) -> Handle {
    legacy::parse(url_string, parse_query_string, slashes_denote_host)
}

/// `url.format(urlObject)` — recompose from a legacy urlObject's fields.
#[rtse::function(module = "node:url", value = "format", ret_ts = "string")]
fn format(url_object: Handle) -> Handle {
    // `url.format(URL)` (WHATWG instance) → the serialized href. `url.format(
    // urlObject)` (legacy plain object) → recompose from its fields (below).
    if is_whatwg_url(url_object) {
        return unsafe { __rtsm_global_URL_to_string(url_object) };
    }
    let mut fields: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut slashes = false;
    for (key, word) in object_entries(url_object) {
        if key == "slashes" {
            slashes = matches!(word_scalar(word as u64).as_deref(), Some("true"));
            continue;
        }
        if let Some(v) = word_scalar(word as u64) {
            fields.insert(key, v);
        }
    }
    let g = |k: &str| fields.get(k).cloned().filter(|s| !s.is_empty());
    let out = legacy::format_from_fields(
        g("protocol"),
        slashes,
        g("auth"),
        g("host"),
        g("hostname"),
        g("port"),
        g("pathname"),
        g("search"),
        g("hash"),
    );
    intern(&out)
}
