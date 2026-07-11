//! node:querystring — `parse(str[, sep[, eq]])` → a plain object.
//!
//! Splits on `sep` (default `"&"`), each pair on the FIRST `eq` (default `"="`),
//! percent-decodes key and value with `+`→space, and groups repeated keys into
//! an array (single occurrence → a bare string), matching Node. Insertion order
//! of first occurrence is preserved (shape-key order). Capped at Node's default
//! `maxKeys` (1000) processed pairs.

use super::codec::{read_str, unescape};
use super::words::{object, str_array_word, str_word};

const MAX_KEYS: usize = 1000;

/// `querystring.parse(str)` — default `sep`/`eq`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_QUERYSTRING_PARSE(ptr: *const u8, len: i64) -> u64 {
    parse_impl(read_str(ptr, len), "&", "=")
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
    )
}

fn parse_impl(input: &str, sep: &str, eq: &str) -> u64 {
    // Ordered accumulation: first-seen key order, values grouped.
    let mut order: Vec<String> = Vec::new();
    let mut values: Vec<Vec<String>> = Vec::new();
    let mut index: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

    if !input.is_empty() {
        for pair in input.split(sep).take(MAX_KEYS) {
            if pair.is_empty() {
                continue;
            }
            let (raw_key, raw_val) = match pair.split_once(eq) {
                Some((k, v)) => (k, v),
                None => (pair, ""),
            };
            let key = unescape(raw_key, true);
            let val = unescape(raw_val, true);
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
