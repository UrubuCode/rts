//! node:querystring — `parse(str[, sep[, eq[, options]]])` → a plain object.
//!
//! Splits on `sep` (default `"&"`), each pair on the FIRST `eq` (default `"="`),
//! percent-decodes key and value with `+`→space, and groups repeated keys into
//! an array (single occurrence → a bare string), matching Node. Insertion order
//! of first occurrence is preserved (shape-key order). Capped at `maxKeys`
//! processed pairs (default 1000; `options.maxKeys === 0` removes the limit).
//! `options.decodeURIComponent` replaces the default percent/`+` decoder.
//!
//! `parse`/`decode` are the same algorithm under two JS names — declared as two
//! thin `#[rtse::function]` wrappers over the shared `parse_with_options`
//! (the macro ties one Rust fn to one JS name; there is no `aliases` form yet).

use super::codec::unescape;
use super::words::{invoke_string_fn, is_function_word, object, opt_field, str_array_word, str_word};

use rts_engine::abi::ty::Handle;

const MAX_KEYS: usize = 1000;

/// `querystring.parse(str[, sep[, eq[, options]]])` — default `sep`/`eq`;
/// groups repeated keys into arrays; reads `options.maxKeys`/
/// `options.decodeURIComponent`.
#[rtse::function(module = "node:querystring", value = "parse")]
fn parse(s: &str, #[default("&")] sep: &str, #[default("=")] eq: &str, options: Option<Handle>) -> Handle {
    parse_with_options(s, sep, eq, options)
}

/// `querystring.decode` — alias of `parse`.
#[rtse::function(module = "node:querystring", value = "decode")]
fn decode(s: &str, #[default("&")] sep: &str, #[default("=")] eq: &str, options: Option<Handle>) -> Handle {
    parse_with_options(s, sep, eq, options)
}

fn parse_with_options(input: &str, sep: &str, eq: &str, options: Option<u64>) -> u64 {
    let sep = if sep.is_empty() { "&" } else { sep };
    let eq = if eq.is_empty() { "=" } else { eq };
    let Some(options) = options else {
        return parse_impl(input, sep, eq, MAX_KEYS, None);
    };
    // maxKeys: absent → 1000; `0` → unlimited (`usize::MAX`).
    let max_keys = match opt_field(options, "maxKeys") {
        Some(w) => match super::words::scalar_string(w as u64).parse::<usize>() {
            Ok(0) => usize::MAX,
            Ok(n) => n,
            Err(_) => MAX_KEYS,
        },
        None => MAX_KEYS,
    };
    // decodeURIComponent: a function VALUE replaces the default decoder.
    let decode = match opt_field(options, "decodeURIComponent") {
        Some(w) if is_function_word(w as u64) => Some(w as u64),
        _ => None,
    };
    parse_impl(input, sep, eq, max_keys, decode)
}

/// Decode one raw component: the custom hook if provided, else the default
/// percent/`+` decoder.
fn decode_component(raw: &str, decode: Option<u64>) -> String {
    match decode {
        Some(f) => invoke_string_fn(f, raw),
        None => unescape(raw, true),
    }
}

fn parse_impl(input: &str, sep: &str, eq: &str, max_keys: usize, decode: Option<u64>) -> u64 {
    // Ordered accumulation: first-seen key order, values grouped.
    let mut order: Vec<String> = Vec::new();
    let mut values: Vec<Vec<String>> = Vec::new();
    let mut index: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

    if !input.is_empty() {
        for pair in input.split(sep).take(max_keys) {
            if pair.is_empty() {
                continue;
            }
            let (raw_key, raw_val) = match pair.split_once(eq) {
                Some((k, v)) => (k, v),
                None => (pair, ""),
            };
            let key = decode_component(raw_key, decode);
            let val = decode_component(raw_val, decode);
            match index.get(&key) {
                Some(&i) => values[i].push(val),
                None => {
                    index.insert(key.clone(), order.len());
                    order.push(key);
                    values.push(vec![val]);
                }
            }
        }
    }

    let value_words: Vec<i64> = values
        .iter()
        .map(|vs| {
            if vs.len() == 1 {
                str_word(&vs[0])
            } else {
                str_array_word(vs)
            }
        })
        .collect();
    object(order, &value_words)
}
