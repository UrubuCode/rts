//! node:querystring — `stringify(obj[, sep[, eq]])` → a query string.
//!
//! Reads the object's own keyed entries natively (`words::object_entries`),
//! percent-encodes each key and value, expands an array value into repeated
//! `key=v` pairs, coerces scalars (number/boolean → text, `null`/`undefined`/
//! non-array-object → empty value), and joins with `sep`/`eq`. Matches Node's
//! `querystring.stringify`.
//!
//! `stringify`/`encode` are the same algorithm under two JS names — declared as
//! two thin `#[rtse::function]` wrappers over the shared `stringify_with_options`
//! (the macro ties one Rust fn to one JS name; there is no `aliases` form yet).

use super::codec::escape;
use super::words::{
    array_strings, invoke_string_fn, is_function_word, is_object_word, object_entries,
    opt_field, scalar_string, word_handle,
};

use rts_engine::abi::ty::Handle;

/// `querystring.stringify(obj[, sep[, eq[, options]]])` — default `sep`/`eq`;
/// reads `options.encodeURIComponent`.
#[rtse::function(module = "node:querystring", value = "stringify")]
fn stringify(obj: Handle, #[default("&")] sep: &str, #[default("=")] eq: &str, options: Option<Handle>) -> String {
    stringify_with_options(obj, sep, eq, options)
}

/// `querystring.encode` — alias of `stringify`.
#[rtse::function(module = "node:querystring", value = "encode")]
fn encode(obj: Handle, #[default("&")] sep: &str, #[default("=")] eq: &str, options: Option<Handle>) -> String {
    stringify_with_options(obj, sep, eq, options)
}

fn stringify_with_options(obj: u64, sep: &str, eq: &str, options: Option<u64>) -> String {
    let sep = if sep.is_empty() { "&" } else { sep };
    let eq = if eq.is_empty() { "=" } else { eq };
    let encode = options.and_then(|o| match opt_field(o, "encodeURIComponent") {
        Some(w) if is_function_word(w as u64) => Some(w as u64),
        _ => None,
    });
    stringify_impl(obj, sep, eq, encode)
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
