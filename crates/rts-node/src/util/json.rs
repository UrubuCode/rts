//! node:util — a JSON stringifier over the PolyValue graph, backing the `%j`
//! format specifier (Node's `%j` is `JSON.stringify`). Handles numbers, strings
//! (escaped), booleans, null, arrays, and plain shaped/Map objects. `undefined`
//! and functions render as the string `undefined` (Node's `%j` catches the
//! JSON.stringify throw and emits `[Circular]`/`undefined` — RTS emits
//! `undefined` for the non-serializable cases).

use rts_engine::heap::handles::{with_entry, Entry};
use rts_engine::heap::poly::{
    poly_handle_normalize, POLY_BOX_BASE, POLY_PAYLOAD_MASK, POLY_TAG_MASK, POLY_TAG_SHIFT,
};
use rts_engine::heap::shapes::global_shape_keys;

/// `JSON.stringify(value)` for `%j`.
pub fn stringify(word: u64) -> String {
    if (word & POLY_BOX_BASE) != POLY_BOX_BASE {
        return fmt_num(f64::from_bits(word));
    }
    match (word >> POLY_TAG_SHIFT) & POLY_TAG_MASK {
        1 => ((word & POLY_PAYLOAD_MASK) as u32 as i32).to_string(),
        2 => match word & POLY_PAYLOAD_MASK {
            1 => "null".to_string(),
            2 => "false".to_string(),
            3 => "true".to_string(),
            _ => "undefined".to_string(),
        },
        3 => quote(&string_of(word)),
        4 => stringify_heap(word),
        _ => "undefined".to_string(),
    }
}

fn string_of(word: u64) -> String {
    poly_handle_normalize(word)
        .map(|h| {
            with_entry(h, |e| match e {
                Some(Entry::String(s)) => String::from_utf8_lossy(s).into_owned(),
                _ => String::new(),
            })
        })
        .unwrap_or_default()
}

fn quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn fmt_num(f: f64) -> String {
    // JSON has no NaN/Infinity — they serialize as null.
    if f.is_finite() { format!("{f}") } else { "null".to_string() }
}

fn stringify_heap(word: u64) -> String {
    let Some(h) = poly_handle_normalize(word) else {
        return "null".to_string();
    };
    enum Kind {
        Array(Vec<i64>),
        Object(Vec<(String, i64)>),
    }
    let kind = with_entry(h, |e| match e {
        Some(Entry::Vec(slots)) => {
            if let Some(&w0) = slots.first() {
                let w0 = w0 as u64;
                if (w0 & POLY_BOX_BASE) == POLY_BOX_BASE {
                    if let Some(keys) = global_shape_keys((w0 & POLY_PAYLOAD_MASK) as u32) {
                        if keys.len() + 1 == slots.len() {
                            return Some(Kind::Object(
                                keys.into_iter().zip(slots[1..].iter().copied()).collect(),
                            ));
                        }
                    }
                }
            }
            Some(Kind::Array(slots.as_ref().clone()))
        }
        Some(Entry::Map(m)) => Some(Kind::Object(m.iter().map(|(k, v)| (k.clone(), *v)).collect())),
        _ => None,
    });
    match kind {
        Some(Kind::Array(slots)) => {
            let items: Vec<String> = slots.iter().map(|&w| stringify(w as u64)).collect();
            format!("[{}]", items.join(","))
        }
        Some(Kind::Object(pairs)) => {
            let items: Vec<String> = pairs
                .iter()
                .map(|(k, v)| format!("{}:{}", quote(k), stringify(*v as u64)))
                .collect();
            format!("{{{}}}", items.join(","))
        }
        None => "null".to_string(),
    }
}
