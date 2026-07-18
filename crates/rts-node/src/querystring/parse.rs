//! node:querystring — `parse(str[, sep[, eq[, options]]])` → a plain object.
//!
//! Splits on `sep` (default `"&"`), each pair on the FIRST `eq` (default `"="`),
//! percent-decodes key and value with `+`→space, and groups repeated keys into
//! an array (single occurrence → a bare string), matching Node. Insertion order
//! of first occurrence is preserved (shape-key order). Capped at `maxKeys`
//! processed pairs (default 1000; `options.maxKeys === 0` removes the limit).
//! `options.decodeURIComponent` replaces the default percent/`+` decoder.

use super::codec::{read_str, unescape};
use super::words::{invoke_string_fn, is_function_word, object, opt_field, str_array_word, str_word};

const MAX_KEYS: usize = 1000;

/// `querystring.parse(str)` — default `sep`/`eq`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_QUERYSTRING_PARSE(ptr: *const u8, len: i64) -> u64 {
    parse_impl(read_str(ptr, len), "&", "=", MAX_KEYS, None)
}

/// `querystring.parse(str, sep, eq)` — explicit separators.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_QUERYSTRING_PARSE_SEP(
    ptr: *const u8,
    len: i64,
    sep_ptr: *const u8,
    sep_len: i64,
    eq_ptr: *const u8,
    eq_len: i64,
) -> u64 {
    let sep = read_str(sep_ptr, sep_len);
    let eq = read_str(eq_ptr, eq_len);
    parse_impl(
        read_str(ptr, len),
        if sep.is_empty() { "&" } else { sep },
        if eq.is_empty() { "=" } else { eq },
        MAX_KEYS,
        None,
    )
}

/// `querystring.parse(str, sep, eq, options)` — with `maxKeys` +
/// `decodeURIComponent`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_QUERYSTRING_PARSE_OPTS(
    ptr: *const u8,
    len: i64,
    sep_ptr: *const u8,
    sep_len: i64,
    eq_ptr: *const u8,
    eq_len: i64,
    options: u64,
) -> u64 {
    let sep = read_str(sep_ptr, sep_len);
    let eq = read_str(eq_ptr, eq_len);
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
    parse_impl(
        read_str(ptr, len),
        if sep.is_empty() { "&" } else { sep },
        if eq.is_empty() { "=" } else { eq },
        max_keys,
        decode,
    )
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
