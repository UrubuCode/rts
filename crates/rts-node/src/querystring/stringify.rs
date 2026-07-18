//! node:querystring — `stringify(obj[, sep[, eq]])` → a query string.
//!
//! Reads the object's own keyed entries natively (`words::object_entries`),
//! percent-encodes each key and value, expands an array value into repeated
//! `key=v` pairs, coerces scalars (number/boolean → text, `null`/`undefined`/
//! non-array-object → empty value), and joins with `sep`/`eq`. Matches Node's
//! `querystring.stringify`.

use super::codec::{escape, read_str};
use super::words::{
    array_strings, intern, invoke_string_fn, is_function_word, is_object_word, object_entries,
    opt_field, scalar_string, word_handle,
};

/// `querystring.stringify(obj)` — default `sep`/`eq`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_QUERYSTRING_STRINGIFY(obj: u64) -> u64 {
    intern(&stringify_impl(obj, "&", "=", None))
}

/// `querystring.stringify(obj, sep, eq)` — explicit separators.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_QUERYSTRING_STRINGIFY_SEP(
    obj: u64,
    sep_ptr: *const u8,
    sep_len: i64,
    eq_ptr: *const u8,
    eq_len: i64,
) -> u64 {
    let sep = read_str(sep_ptr, sep_len);
    let eq = read_str(eq_ptr, eq_len);
    intern(&stringify_impl(
        obj,
        if sep.is_empty() { "&" } else { sep },
        if eq.is_empty() { "=" } else { eq },
        None,
    ))
}

/// `querystring.stringify(obj, sep, eq, options)` — with `encodeURIComponent`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_QUERYSTRING_STRINGIFY_OPTS(
    obj: u64,
    sep_ptr: *const u8,
    sep_len: i64,
    eq_ptr: *const u8,
    eq_len: i64,
    options: u64,
) -> u64 {
    let sep = read_str(sep_ptr, sep_len);
    let eq = read_str(eq_ptr, eq_len);
    let encode = match opt_field(options, "encodeURIComponent") {
        Some(w) if is_function_word(w as u64) => Some(w as u64),
        _ => None,
    };
    intern(&stringify_impl(
        obj,
        if sep.is_empty() { "&" } else { sep },
        if eq.is_empty() { "=" } else { eq },
        encode,
    ))
}

/// Encode one component: the custom hook if provided, else the default escaper.
fn encode_component(raw: &str, encode: Option<u64>) -> String {
    match encode {
        Some(f) => invoke_string_fn(f, raw),
        None => escape(raw),
    }
}

fn stringify_impl(obj: u64, sep: &str, eq: &str, encode: Option<u64>) -> String {
    let entries = match object_entries(obj) {
        Some(e) => e,
        None => return String::new(),
    };
    let mut pairs: Vec<String> = Vec::new();
    for (key, vword) in entries {
        let k = encode_component(&key, encode);
        let w = vword as u64;
        // An array value expands to one `key=v` pair per element; a nested
        // (non-array) object coerces to an empty value, like Node.
        if is_object_word(w) {
            if let Some(h) = word_handle(w) {
                if object_entries(h).is_some() {
                    pairs.push(format!("{k}{eq}"));
                } else {
                    for item in array_strings(h) {
                        pairs.push(format!("{k}{eq}{}", encode_component(&item, encode)));
                    }
                }
                continue;
            }
        }
        pairs.push(format!("{k}{eq}{}", encode_component(&scalar_string(w), encode)));
    }
    pairs.join(sep)
}
