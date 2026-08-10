//! `node:querystring` — parsing and formatting a URL query string, as text.
//!
//! # Where the work happens
//!
//! `parse`/`decode` never touch the entry API beyond reading the input
//! strings and building the result: grouping repeated keys is done on a
//! `Vec<(String, Vec<String>)>` in Rust, and the engine object is built once,
//! at the end, inside a single [`with_runtime`](rts_core_rwk::entry::with_runtime)
//! call. `stringify`/`encode` are the one direction that has to read an
//! arbitrary program value apart: it walks
//! [`own_keys`](rts_core_rwk::entry::own_keys) and reads each value with
//! [`described`](rts_core_rwk::entry::described), which is this crate's
//! `ToString` — the same operation `+` uses on an operand that is not already
//! a string or number.
//!
//! # `decode`/`encode` are the same function as `parse`/`stringify`
//!
//! Not two implementations that happen to agree — [`parse`] is installed
//! twice, under both names, and likewise [`stringify`]. `querystring.decode
//! === querystring.parse` holds here for the reason it holds in real Node:
//! it is one function with two names.
//!
//! # `options`
//!
//! `parse(str, sep?, eq?, options?)` and `stringify(obj, sep?, eq?, options?)`
//! are both exactly four Node arguments, which is this calling convention's
//! own slot count — `options` lands in the fourth ABI slot directly, no
//! [`rts_core_rwk::entry::rest_arguments`] needed. `options.maxKeys` truncates
//! `parse`'s result to that many keys in insertion order (`0` or absent means
//! unlimited, Node's own meaning); `options.decodeURIComponent`/
//! `.encodeURIComponent`, when callable, replace this module's own percent
//! codec for that one call, invoked through [`rts_core_rwk::entry::call`].
//! Rule 8 of `rts-core-rwk`'s README asks a native calling user code to check
//! whether the callee threw before trusting the answer; the check itself
//! (`entry::throw::in_flight`) is `pub(in crate::entry)`, not reachable from
//! this crate, so this module cannot satisfy that rule and does not pretend
//! to — the same gap `async_hooks::local`/`resource.rs` and
//! `child_process::spawn_async` already carry for every callback they invoke
//! through the same function. A throwing callback here answers `undefined`,
//! which flows through the codec unchecked, same as those sites.
//!
//! # Not implemented, by name
//!
//! The result object from
//! `parse`/`decode` is an **ordinary** object, not the null-prototype one
//! real Node returns — nothing in the value API this crate was given builds
//! an object with no prototype, so a key named `__proto__` lands as an own
//! property on a value that still has `Object.prototype` behind it, which is
//! exactly the hardening real Node's null-prototype result exists to avoid.
//! `escape`'s and `unescape`'s exact character table is this module's own
//! (documented in [`percent_encode`]/[`percent_decode`]), not verified
//! byte-for-byte against a real Node 25 binary — the reference document
//! itself flags the exact table as unverified.

use rts_core_rwk::entry::{Context, Provided};

/// The namespace `node:querystring` is.
pub fn namespace(context: &mut Context) -> u64 {
    let members: &[(&str, Provided)] = &[
        ("parse", parse),
        ("decode", parse),
        ("stringify", stringify),
        ("encode", stringify),
        ("escape", escape),
        ("unescape", unescape),
    ];
    rts_core_rwk::entry::make_namespace(context, members)
}

/// `querystring.parse(str, sep?, eq?, options?)` / `.decode(...)`.
extern "C" fn parse(_e: u64, _this: u64, text: u64, sep: u64, eq: u64, options: u64) -> u64 {
    let Some(text) = argument_text(text) else {
        return rts_core_rwk::entry::with_runtime(rts_core_rwk::entry::make_object);
    };
    let sep = argument_text(sep).unwrap_or_else(|| "&".to_owned());
    let eq = argument_text(eq).unwrap_or_else(|| "=".to_owned());
    let decoder = option_callback(options, "decodeURIComponent");
    let max_keys = option_max_keys(options);
    let mut grouped = parse_pairs(&text, &sep, &eq, decoder);
    if let Some(max_keys) = max_keys {
        grouped.truncate(max_keys);
    }
    rts_core_rwk::entry::with_runtime(|context| {
        let result = rts_core_rwk::entry::make_object(context);
        for (key, values) in grouped {
            let value = match values.len() {
                1 => rts_core_rwk::entry::make_string(context, &values[0]),
                _ => {
                    let strings = values.iter().map(|v| rts_core_rwk::entry::make_string(context, v)).collect();
                    rts_core_rwk::entry::make_array_in(context, strings)
                }
            };
            rts_core_rwk::entry::put_member(context, result, &key, value);
        }
        result
    })
}

/// `querystring.stringify(obj, sep?, eq?, options?)` / `.encode(...)`.
extern "C" fn stringify(_e: u64, _this: u64, object: u64, sep: u64, eq: u64, options: u64) -> u64 {
    let sep = argument_text(sep).unwrap_or_else(|| "&".to_owned());
    let eq = argument_text(eq).unwrap_or_else(|| "=".to_owned());
    let encoder = option_callback(options, "encodeURIComponent");
    let absent = rts_core_rwk::entry::undefined_value();
    if object == absent {
        return rts_core_rwk::entry::with_runtime(|context| rts_core_rwk::entry::make_string(context, ""));
    }
    let keys = collect_array(rts_core_rwk::entry::own_keys(object));
    let mut pairs = Vec::new();
    for key in keys {
        let Some(name) = rts_core_rwk::entry::text_of(key) else {
            continue;
        };
        let value = rts_core_rwk::entry::get_indexed(object, key);
        for text in stringify_value(value) {
            pairs.push(format!("{}{eq}{}", encode_with(&name, encoder), encode_with(&text, encoder)));
        }
    }
    let joined = pairs.join(&sep);
    rts_core_rwk::entry::with_runtime(|context| rts_core_rwk::entry::make_string(context, &joined))
}

/// `options.<name>` when it is callable, `None` otherwise (including an
/// absent `options` — the fourth argument's own "no options" value).
fn option_callback(options: u64, name: &str) -> Option<u64> {
    let absent = rts_core_rwk::entry::undefined_value();
    if options == absent {
        return None;
    }
    let value = rts_core_rwk::entry::get_indexed(options, rts_core_rwk::entry::with_runtime(|context| rts_core_rwk::entry::make_string(context, name)));
    let callable = rts_core_rwk::entry::with_runtime(|context| rts_core_rwk::entry::is_callable_in(context, value));
    callable.then_some(value)
}

/// `options.maxKeys` as a truncation count — `None` for "unlimited", which is
/// both Node's `0` and an absent/non-numeric option (this module enforces no
/// cap by default, so an option this crate cannot read is the same as one
/// that was not given, never a silent different default).
fn option_max_keys(options: u64) -> Option<usize> {
    let absent = rts_core_rwk::entry::undefined_value();
    if options == absent {
        return None;
    }
    let value = rts_core_rwk::entry::get_indexed(options, rts_core_rwk::entry::with_runtime(|context| rts_core_rwk::entry::make_string(context, "maxKeys")));
    let count = rts_core_rwk::entry::number_of(value)?;
    if count <= 0.0 { None } else { Some(count as usize) }
}

/// A caller-supplied `decodeURIComponent`, called with one string argument;
/// see the module doc for why this cannot check whether it threw.
fn call_codec(callback: u64, text: &str) -> String {
    let absent = rts_core_rwk::entry::undefined_value();
    let argument = rts_core_rwk::entry::with_runtime(|context| rts_core_rwk::entry::make_string(context, text));
    let result = rts_core_rwk::entry::call(callback, absent, argument, absent, absent, absent);
    rts_core_rwk::entry::text_of(result).unwrap_or_default()
}

fn encode_with(text: &str, encoder: Option<u64>) -> String {
    match encoder {
        Some(callback) => call_codec(callback, text),
        None => percent_encode(text),
    }
}

/// `querystring.escape(str)`.
extern "C" fn escape(_e: u64, _this: u64, text: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let text = argument_text(text).unwrap_or_default();
    let encoded = percent_encode(&text);
    rts_core_rwk::entry::with_runtime(|context| rts_core_rwk::entry::make_string(context, &encoded))
}

/// `querystring.unescape(str)`.
extern "C" fn unescape(_e: u64, _this: u64, text: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let text = argument_text(text).unwrap_or_default();
    let decoded = percent_decode(&text);
    rts_core_rwk::entry::with_runtime(|context| rts_core_rwk::entry::make_string(context, &decoded))
}

/// One property value, reduced to the strings `stringify` writes for it.
///
/// A scalar goes through [`described`](rts_core_rwk::entry::described) — this
/// crate's `ToString` — which is `None` only for an object, so an array falls
/// through to being read element-by-element the same way [`collect_array`]
/// reads any array; anything else that is an object (a plain object, `null`)
/// coerces to a single empty string, matching real Node's "unsupported type"
/// rule.
fn stringify_value(value: u64) -> Vec<String> {
    if let Some(text) = rts_core_rwk::entry::described(value) {
        return vec![text];
    }
    let elements = collect_array(value);
    if elements.is_empty() {
        return vec![String::new()];
    }
    elements
        .into_iter()
        .map(|element| rts_core_rwk::entry::described(element).unwrap_or_default())
        .collect()
}

/// Splits a query string into `key -> values`, first-seen order, repeated
/// keys accumulating into more than one value.
fn parse_pairs(text: &str, sep: &str, eq: &str, decoder: Option<u64>) -> Vec<(String, Vec<String>)> {
    let mut grouped: Vec<(String, Vec<String>)> = Vec::new();
    if text.is_empty() {
        return grouped;
    }
    let decode = |raw: &str| match decoder {
        Some(callback) => call_codec(callback, raw),
        None => percent_decode(raw),
    };
    for pair in split_literal(text, sep) {
        let (raw_key, raw_value) = match pair.find(eq) {
            Some(at) => (&pair[..at], &pair[at + eq.len()..]),
            None => (pair, ""),
        };
        let key = decode(raw_key);
        let value = decode(raw_value);
        match grouped.iter_mut().find(|(existing, _)| *existing == key) {
            Some((_, values)) => values.push(value),
            None => grouped.push((key, vec![value])),
        }
    }
    grouped
}

/// Splits on a literal substring separator — never a regex, per the module's
/// own semantics.
fn split_literal<'a>(text: &'a str, sep: &str) -> Vec<&'a str> {
    if sep.is_empty() {
        return vec![text];
    }
    text.split(sep).collect()
}

/// Percent-decodes a component, with `+` read as space — the historical
/// form-encoding convention `parse` relies on. Never fails: a malformed
/// `%XY` escape is passed through as the literal `%` and the two characters
/// after it, which is this module's non-throwing fallback.
fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                out.push(b' ');
                index += 1;
            }
            b'%' if index + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).ok();
                match hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                    Some(byte) => {
                        out.push(byte);
                        index += 3;
                    }
                    None => {
                        out.push(bytes[index]);
                        index += 1;
                    }
                }
            }
            byte => {
                out.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Percent-encodes everything outside the unreserved set
/// (`A-Za-z0-9 - _ . ! ~ * ' ( )`), including space as `%20` — not `+`; the
/// `+`-for-space convention here is only on the decode side, matching how
/// [`percent_decode`] is written.
fn percent_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'!' | b'~' | b'*' | b'\'' | b'(' | b')' => {
                out.push(*byte as char);
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

/// An argument as text, `None` for an absent (`undefined`) one.
fn argument_text(value: u64) -> Option<String> {
    let absent = rts_core_rwk::entry::undefined_value();
    if value == absent {
        return None;
    }
    rts_core_rwk::entry::text_of(value)
}

/// A JS array's elements, read out through `.length` and indexed access.
fn collect_array(array: u64) -> Vec<u64> {
    let absent = rts_core_rwk::entry::undefined_value();
    if array == absent {
        return Vec::new();
    }
    let length_key = rts_core_rwk::entry::with_runtime(|context| rts_core_rwk::entry::make_string(context, "length"));
    let length_value = rts_core_rwk::entry::get_indexed(array, length_key);
    // Asked of the value, not of its text: reading a number by parsing its
    // decimal back is lossy where a double's shortest decimal is not the double.
    let length = rts_core_rwk::entry::number_of(length_value)
        .map(|value| value as usize)
        .unwrap_or(0);
    (0..length)
        .map(|index| rts_core_rwk::entry::get_indexed(array, rts_core_rwk::entry::make_number(index as f64)))
        .collect()
}
