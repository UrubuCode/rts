//! node:util — `inspect(value)`: the deep-object renderer behind `console.log`
//! and the `%o`/`%O`/object-`%s` format specifiers. Walks the PolyValue graph
//! and produces Node's textual representation (quoted strings inside containers,
//! `[ 1, 2, 3 ]` arrays, `{ a: 1, b: 'two' }` objects), with a depth limit that
//! collapses deeper containers to `[Array]`/`[Object]` like Node's default.

use rts_engine::heap::handles::{with_entry, Entry};
use rts_engine::heap::poly::{
    poly_handle_normalize, POLY_BOX_BASE, POLY_PAYLOAD_MASK, POLY_TAG_MASK, POLY_TAG_SHIFT,
};
use rts_engine::heap::shapes::global_shape_keys;

/// Node's default `util.inspect` depth.
const DEFAULT_DEPTH: i32 = 2;

/// `util.inspect(value)` — top level. A top-level string is returned verbatim
/// with quotes (matching Node), unlike `String(value)`.
pub fn inspect(word: u64) -> String {
    render(word, DEFAULT_DEPTH, true)
}

/// `util.inspect(value, options)` — honors `options.depth` (a number, or `null`
/// for effectively unlimited); other options are not yet applied.
pub fn inspect_with_options(word: u64, options: u64) -> String {
    render(word, option_depth(options).unwrap_or(DEFAULT_DEPTH), true)
}

/// Read `options.depth`: a number → that depth; `null` → a large depth; absent →
/// `None` (caller uses the default).
fn option_depth(options: u64) -> Option<i32> {
    let handle = poly_handle_normalize(options)?;
    let depth_word = with_entry(handle, |e| match e {
        Some(Entry::Vec(slots)) if !slots.is_empty() => {
            let w0 = slots[0] as u64;
            if (w0 & POLY_BOX_BASE) != POLY_BOX_BASE {
                return None;
            }
            let keys = global_shape_keys((w0 & POLY_PAYLOAD_MASK) as u32)?;
            if keys.len() + 1 != slots.len() {
                return None;
            }
            keys.iter().position(|k| k == "depth").map(|i| slots[i + 1])
        }
        Some(Entry::Map(m)) => m.get("depth").copied(),
        _ => None,
    })?;
    let w = depth_word as u64;
    // A genuine inline double → that depth; the `null` singleton → large.
    if (w & POLY_BOX_BASE) != POLY_BOX_BASE {
        Some(f64::from_bits(w) as i32)
    } else if (w >> POLY_TAG_SHIFT) & POLY_TAG_MASK == 1 {
        Some((w & POLY_PAYLOAD_MASK) as u32 as i32) // boxed int32
    } else {
        Some(1_000_000) // null / anything else → effectively unlimited
    }
}

fn render(word: u64, depth: i32, top: bool) -> String {
    // Inline double.
    if (word & POLY_BOX_BASE) != POLY_BOX_BASE {
        return fmt_number(f64::from_bits(word));
    }
    let tag = (word >> POLY_TAG_SHIFT) & POLY_TAG_MASK;
    match tag {
        1 => ((word & POLY_PAYLOAD_MASK) as u32 as i32).to_string(),
        2 => match word & POLY_PAYLOAD_MASK {
            1 => "null".to_string(),
            2 => "false".to_string(),
            3 => "true".to_string(),
            _ => "undefined".to_string(),
        },
        3 => {
            let s = string_of(word);
            // A container element quotes its strings; a bare top-level string is
            // quoted too (Node: util.inspect("x") === "'x'").
            let _ = top;
            format!("'{}'", s.replace('\'', "\\'"))
        }
        4 => render_heap(word, depth),
        5 => "[Function (anonymous)]".to_string(),
        _ => "undefined".to_string(),
    }
}

fn string_of(word: u64) -> String {
    match poly_handle_normalize(word) {
        Some(h) => with_entry(h, |e| match e {
            Some(Entry::String(s)) => String::from_utf8_lossy(s).into_owned(),
            _ => String::new(),
        }),
        None => String::new(),
    }
}

fn render_heap(word: u64, depth: i32) -> String {
    let Some(h) = poly_handle_normalize(word) else {
        return "undefined".to_string();
    };
    // Classify: shaped object (slot 0 = registered shape id), array, or Map.
    enum Kind {
        Array(Vec<i64>),
        Object(Vec<(String, i64)>),
    }
    let kind = with_entry(h, |e| match e {
        Some(Entry::Vec(slots)) => {
            if let Some(&w0) = slots.first() {
                let w0 = w0 as u64;
                if (w0 & POLY_BOX_BASE) == POLY_BOX_BASE {
                    let shape_id = (w0 & POLY_PAYLOAD_MASK) as u32;
                    if let Some(keys) = global_shape_keys(shape_id) {
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
            if slots.is_empty() {
                return "[]".to_string();
            }
            if depth < 0 {
                return "[Array]".to_string();
            }
            let items: Vec<String> = slots.iter().map(|&w| render(w as u64, depth - 1, false)).collect();
            format!("[ {} ]", items.join(", "))
        }
        Some(Kind::Object(pairs)) => {
            if pairs.is_empty() {
                return "{}".to_string();
            }
            if depth < 0 {
                return "[Object]".to_string();
            }
            let items: Vec<String> = pairs
                .iter()
                .map(|(k, v)| format!("{}: {}", fmt_key(k), render(*v as u64, depth - 1, false)))
                .collect();
            format!("{{ {} }}", items.join(", "))
        }
        None => "undefined".to_string(),
    }
}

/// An object key is printed bare when it is a valid identifier, else quoted.
fn fmt_key(k: &str) -> String {
    let ident = !k.is_empty()
        && k.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_' || c == '$')
        && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$');
    if ident {
        k.to_string()
    } else {
        format!("'{}'", k.replace('\'', "\\'"))
    }
}

/// JS number formatting (integers without a trailing `.0`).
fn fmt_number(f: f64) -> String {
    if f.is_nan() {
        "NaN".to_string()
    } else if f.is_infinite() {
        if f < 0.0 { "-Infinity".to_string() } else { "Infinity".to_string() }
    } else {
        // Rust's f64 Display already prints 42.0 as "42" and 3.5 as "3.5".
        format!("{f}")
    }
}
