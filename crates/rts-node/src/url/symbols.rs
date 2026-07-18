//! node:url — the `extern "C"` entry points wiring the function surface to the
//! `whatwg`/`fileurl`/`legacy` helpers.

use super::words::{buffer, intern, object_entries, word_scalar};
use super::{fileurl, legacy, whatwg};

use rts_engine::heap::handles::{with_entry, Entry};

unsafe extern "C" {
    // The shared WHATWG `URL` serializer (globals/url) — recomposes the current
    // href (reflecting setters + searchParams mutations). Returns a string handle.
    fn __RTS_FN_GL_URL_TO_STRING(handle: u64) -> u64;
}

/// A WHATWG `URL` instance is stored as an `Entry::Env` (the [href, protocol, …]
/// slot vector). Legacy urlObjects are shaped `Entry::Vec` plain objects. Used
/// so `url.format` accepts BOTH forms (Node overloads it).
fn is_whatwg_url(handle: u64) -> bool {
    with_entry(handle, |e| matches!(e, Some(Entry::Env(_))))
}

fn read(ptr: *const u8, len: i64) -> String {
    unsafe { rts_engine::abi::str_abi::from_abi(ptr, len) }
        .unwrap_or("")
        .to_string()
}

/// `url.domainToASCII(domain)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_URL_DOMAIN_TO_ASCII(p: *const u8, l: i64) -> u64 {
    intern(&whatwg::domain_to_ascii(&read(p, l)))
}

/// `url.domainToUnicode(domain)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_URL_DOMAIN_TO_UNICODE(p: *const u8, l: i64) -> u64 {
    intern(&whatwg::domain_to_unicode(&read(p, l)))
}

/// `url.fileURLToPath(url)` — `url` is a URL string or a `URL` instance handle.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_URL_FILE_URL_TO_PATH(url: u64) -> u64 {
    let bytes = fileurl::file_url_to_path_bytes(url);
    intern(&String::from_utf8_lossy(&bytes))
}

/// `url.fileURLToPathBuffer(url)` → `Buffer` of the raw path bytes.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_URL_FILE_URL_TO_PATH_BUFFER(url: u64) -> u64 {
    buffer(fileurl::file_url_to_path_bytes(url))
}

/// `url.pathToFileURL(path)` → a real `URL`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_URL_PATH_TO_FILE_URL(p: *const u8, l: i64) -> u64 {
    fileurl::path_to_file_url(&read(p, l))
}

/// `url.urlToHttpOptions(url)` → the request-options object.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_URL_TO_HTTP_OPTIONS(url: u64) -> u64 {
    whatwg::url_to_http_options(url)
}

/// `url.resolve(from, to)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_URL_RESOLVE(
    fp: *const u8,
    fl: i64,
    tp: *const u8,
    tl: i64,
) -> u64 {
    intern(&legacy::resolve(&read(fp, fl), &read(tp, tl)))
}

/// `url.parse(urlString)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_URL_PARSE(p: *const u8, l: i64) -> u64 {
    legacy::parse(&read(p, l), false, false)
}

/// `url.parse(urlString, parseQueryString, slashesDenoteHost)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_URL_PARSE_OPTS(
    p: *const u8,
    l: i64,
    parse_query: i64,
    slashes: i64,
) -> u64 {
    legacy::parse(&read(p, l), parse_query != 0, slashes != 0)
}

/// `url.format(urlObject)` — recompose from a legacy urlObject's fields.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_URL_FORMAT(obj: u64) -> u64 {
    // `url.format(URL)` (WHATWG instance) → the serialized href. `url.format(
    // urlObject)` (legacy plain object) → recompose from its fields (below).
    if is_whatwg_url(obj) {
        return unsafe { __RTS_FN_GL_URL_TO_STRING(obj) };
    }
    let mut fields: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut slashes = false;
    for (key, word) in object_entries(obj) {
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
