//! node:util — `parseArgs(config)`: the standard CLI argument parser. Reads a
//! config object (`{ args, options, allowPositionals }`) and returns a plain
//! `{ values, positionals }` object. Real parsing over the engine's shaped-object
//! representation — no fabrication, no method dispatch (only field access).
//!
//! Supported per-option config: `type` ("boolean"/"string"), `short` (an alias
//! char), `multiple` (accumulate into an array). Deferred: `default`, `strict`
//! error reporting details, `tokens`, negated `--no-foo` booleans.

use rts_engine::heap::handles::{alloc_entry, with_entry, Entry};
use rts_engine::heap::poly::{poly_handle_normalize, POLY_BOX_BASE, POLY_PAYLOAD_MASK};
use rts_engine::heap::shapes::{
    alloc_shaped_object, bool_word, global_shape_keys, handle_word_auto, string_word,
};

/// The `(key, value_word)` pairs of a shaped object OR an `Entry::Map`. `word`
/// is a boxed OBJECT PolyValue word (a config arg / a nested slot value), so it
/// is normalized to a heap handle first.
fn entries(word: u64) -> Vec<(String, i64)> {
    let Some(handle) = poly_handle_normalize(word) else { return Vec::new() };
    with_entry(handle, |e| match e {
        Some(Entry::Vec(slots)) if !slots.is_empty() => {
            let w0 = slots[0] as u64;
            if (w0 & POLY_BOX_BASE) == POLY_BOX_BASE {
                let shape_id = (w0 & POLY_PAYLOAD_MASK) as u32;
                if let Some(keys) = global_shape_keys(shape_id) {
                    if keys.len() + 1 == slots.len() {
                        return keys.into_iter().zip(slots[1..].iter().copied()).collect();
                    }
                }
            }
            Vec::new()
        }
        Some(Entry::Map(m)) => m.iter().map(|(k, v)| (k.clone(), *v)).collect(),
        _ => Vec::new(),
    })
}

/// A field's value word off an object handle.
fn get(handle: u64, key: &str) -> Option<i64> {
    entries(handle).into_iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

/// Coerce a word to a `String` (a STR word → its text; else empty).
fn word_str(w: u64) -> String {
    poly_handle_normalize(w)
        .map(|h| {
            with_entry(h, |e| match e {
                Some(Entry::String(s)) => String::from_utf8_lossy(s).into_owned(),
                _ => String::new(),
            })
        })
        .unwrap_or_default()
}

/// Read a JS `string[]` (a boxed array word) into a `Vec<String>`.
fn word_str_array(word: u64) -> Vec<String> {
    let Some(handle) = poly_handle_normalize(word) else { return Vec::new() };
    with_entry(handle, |e| match e {
        Some(Entry::Vec(v)) => v.iter().map(|&w| word_str(w as u64)).collect(),
        _ => Vec::new(),
    })
}

fn truthy(w: i64) -> bool {
    // The `true` singleton (payload 3) — everything else here is false/absent.
    (w as u64 & POLY_PAYLOAD_MASK) == 3 && (w as u64 & POLY_BOX_BASE) == POLY_BOX_BASE
}

struct OptSpec {
    long: String,
    is_string: bool,
    short: Option<char>,
    multiple: bool,
}

/// `util.parseArgs(config)` → `{ values, positionals }`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_UTIL_PARSE_ARGS(config: u64) -> u64 {
    let args: Vec<String> = get(config, "args").map(|w| word_str_array(w as u64)).unwrap_or_default();
    let allow_pos = get(config, "allowPositionals").map(|w| truthy(w)).unwrap_or(false);

    // Build the option specs from config.options.
    let mut specs: Vec<OptSpec> = Vec::new();
    if let Some(opts_w) = get(config, "options") {
        for (long, spec_w) in entries(opts_w as u64) {
            let sw = spec_w as u64;
            let is_string = get(sw, "type").map(|t| word_str(t as u64) == "string").unwrap_or(false);
            let short = get(sw, "short").and_then(|s| word_str(s as u64).chars().next());
            let multiple = get(sw, "multiple").map(|w| truthy(w)).unwrap_or(false);
            specs.push(OptSpec { long, is_string, short, multiple });
        }
    }
    let find_long = |name: &str| specs.iter().find(|s| s.long == name);
    let find_short = |c: char| specs.iter().find(|s| s.short == Some(c));

    // Accumulate values (name → one or many words) and positionals.
    let mut values: Vec<(String, Vec<i64>)> = Vec::new();
    let mut positionals: Vec<i64> = Vec::new();
    let set = |values: &mut Vec<(String, Vec<i64>)>, name: &str, word: i64, multiple: bool| {
        if let Some((_, ws)) = values.iter_mut().find(|(n, _)| n == name) {
            if multiple {
                ws.push(word);
            } else {
                *ws = vec![word];
            }
        } else {
            values.push((name.to_string(), vec![word]));
        }
    };

    let mut i = 0usize;
    let mut only_positional = false;
    while i < args.len() {
        let a = &args[i];
        if only_positional {
            positionals.push(string_word(a.as_bytes()) as i64);
            i += 1;
            continue;
        }
        if a == "--" {
            only_positional = true;
            i += 1;
            continue;
        }
        if let Some(rest) = a.strip_prefix("--") {
            // --name or --name=value
            let (name, inline_val) = match rest.split_once('=') {
                Some((n, v)) => (n.to_string(), Some(v.to_string())),
                None => (rest.to_string(), None),
            };
            let (is_string, multiple) = find_long(&name).map(|s| (s.is_string, s.multiple)).unwrap_or((false, false));
            if is_string {
                let val = inline_val.unwrap_or_else(|| {
                    i += 1;
                    args.get(i).cloned().unwrap_or_default()
                });
                set(&mut values, &name, string_word(val.as_bytes()) as i64, multiple);
            } else {
                set(&mut values, &name, bool_word(true) as i64, multiple);
            }
        } else if let Some(rest) = a.strip_prefix('-') {
            // -x (short); may resolve to a long option.
            if let Some(c) = rest.chars().next() {
                if let Some(spec) = find_short(c) {
                    let (long, is_string, multiple) = (spec.long.clone(), spec.is_string, spec.multiple);
                    if is_string {
                        let val = if rest.len() > 1 {
                            rest[1..].to_string()
                        } else {
                            i += 1;
                            args.get(i).cloned().unwrap_or_default()
                        };
                        set(&mut values, &long, string_word(val.as_bytes()) as i64, multiple);
                    } else {
                        set(&mut values, &long, bool_word(true) as i64, multiple);
                    }
                }
            }
        } else if allow_pos {
            positionals.push(string_word(a.as_bytes()) as i64);
        }
        i += 1;
    }

    // Build the values object: multiple → array; single → the lone word.
    let keys: Vec<String> = values.iter().map(|(k, _)| k.clone()).collect();
    let key_refs: Vec<&str> = keys.iter().map(String::as_str).collect();
    let value_words: Vec<i64> = values
        .iter()
        .map(|(name, ws)| {
            let is_multiple = specs.iter().find(|s| &s.long == name).map(|s| s.multiple).unwrap_or(false);
            if is_multiple {
                handle_word_auto(alloc_entry(Entry::Vec(Box::new(ws.clone())))) as i64
            } else {
                ws[0]
            }
        })
        .collect();
    let values_obj = alloc_shaped_object(&key_refs, &value_words);
    let positionals_arr = alloc_entry(Entry::Vec(Box::new(positionals)));

    alloc_shaped_object(
        &["values", "positionals"],
        &[handle_word_auto(values_obj) as i64, handle_word_auto(positionals_arr) as i64],
    )
}
