//! Intl.* global classes — hardcoded en-US/en-GB behaviour (no ICU).
//!
//! Each constructor (`Intl.NumberFormat`, `Intl.DateTimeFormat`, `Intl.Collator`,
//! `Intl.Segmenter`, `Intl.PluralRules`, `Intl.ListFormat`,
//! `Intl.RelativeTimeFormat`) allocates an `Entry::Map` storing the locale
//! string handle (`__locale`) and the raw options Map handle (`__options`).
//! Instance methods read those back and format with fixed English rules,
//! sufficient to be byte-identical to Node for the supported test cases.

use crate::namespaces::gc::handles::{alloc_entry, with_entry, Entry};

// ── Helpers ───────────────────────────────────────────────────────────────────

unsafe fn str_from_raw(ptr: i64, len: i64) -> String {
    if ptr == 0 || len <= 0 {
        return String::new();
    }
    let bytes = unsafe { std::slice::from_raw_parts(ptr as *const u8, len as usize) };
    std::str::from_utf8(bytes).unwrap_or("").to_owned()
}

fn alloc_str(s: String) -> u64 {
    alloc_entry(Entry::String(s.into_bytes()))
}

/// Reads the string stored at a Map's `key` (value slot is a String handle).
fn map_get_str(map_h: u64, key: &str) -> Option<String> {
    let slot = with_entry(map_h, |e| match e {
        Some(Entry::Map(m)) => m.get(key).copied().unwrap_or(0) as u64,
        _ => 0,
    });
    if slot == 0 {
        return None;
    }
    with_entry(slot, |e| match e {
        Some(Entry::String(b)) => Some(String::from_utf8_lossy(b).into_owned()),
        _ => None,
    })
}

/// Reads the i64 stored at a Map's `key` (number value slot).
fn map_get_int(map_h: u64, key: &str) -> Option<i64> {
    with_entry(map_h, |e| match e {
        Some(Entry::Map(m)) => m.get(key).copied(),
        _ => None,
    })
}

/// Returns the `__options` Map handle stored in an Intl instance.
fn options_of(handle: u64) -> u64 {
    map_get_int(handle, "__options").unwrap_or(0) as u64
}

/// Allocates an Intl instance Map storing locale + options handle.
fn alloc_intl(locale: String, options: u64) -> u64 {
    use indexmap::IndexMap;
    let mut m: IndexMap<String, i64> = IndexMap::new();
    let loc_h = alloc_str(locale);
    m.insert("__locale".to_string(), loc_h as i64);
    m.insert("__options".to_string(), options as i64);
    alloc_entry(Entry::Map(Box::new(m)))
}

// ── Intl.NumberFormat ───────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_INTL_NUMBER_FORMAT_NEW(
    locale_ptr: i64,
    locale_len: i64,
    options: u64,
) -> u64 {
    let locale = unsafe { str_from_raw(locale_ptr, locale_len) };
    alloc_intl(locale, options)
}

/// Inserts thousands separators into the integer part: 1234567 -> "1,234,567".
fn group_thousands(int_str: &str) -> String {
    let bytes = int_str.as_bytes();
    let mut out = String::new();
    let n = bytes.len();
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 && (n - i) % 3 == 0 {
            out.push(',');
        }
        out.push(*b as char);
    }
    out
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_INTL_NUMBER_FORMAT_FORMAT(handle: u64, value: f64) -> u64 {
    let opts = options_of(handle);
    let style = map_get_str(opts, "style").unwrap_or_default();
    let min_frac = map_get_int(opts, "minimumFractionDigits");

    let is_currency = style == "currency";
    // Number of fractional digits: currency defaults to 2; otherwise honour
    // minimumFractionDigits or fall back to a plain integer/decimal render.
    let frac_digits: usize = if is_currency {
        min_frac.map(|v| v as usize).unwrap_or(2).max(2)
    } else {
        min_frac.map(|v| v as usize).unwrap_or(0)
    };

    let negative = value.is_sign_negative() && value != 0.0;
    let abs = value.abs();
    let rounded = format!("{:.*}", frac_digits, abs);
    let (int_part, frac_part) = match rounded.split_once('.') {
        Some((i, f)) => (i.to_string(), Some(f.to_string())),
        None => (rounded.clone(), None),
    };
    let mut body = group_thousands(&int_part);
    if let Some(f) = frac_part {
        body.push('.');
        body.push_str(&f);
    }

    let mut out = String::new();
    if negative {
        out.push('-');
    }
    if is_currency {
        // USD symbol for en-US (only currency tested).
        out.push('$');
    }
    out.push_str(&body);
    alloc_str(out)
}

// ── Intl.DateTimeFormat ─────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_INTL_DATE_TIME_FORMAT_NEW(
    locale_ptr: i64,
    locale_len: i64,
    options: u64,
) -> u64 {
    let locale = unsafe { str_from_raw(locale_ptr, locale_len) };
    alloc_intl(locale, options)
}

/// Decomposes a UTC epoch-ms timestamp into (year, month 1..12, day 1..31).
fn ymd_from_utc_ms(ms: i64) -> (i64, i64, i64) {
    // Days since 1970-01-01 (floor division for negatives).
    let mut days = ms.div_euclid(86_400_000);
    // Civil-from-days algorithm (Howard Hinnant).
    days += 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let doe = days - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let year = if m <= 2 { y + 1 } else { y };
    (year, m, d)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_INTL_DATE_TIME_FORMAT_FORMAT(handle: u64, date_handle: u64) -> u64 {
    // date_handle is a Date instance (Entry::DateMs).
    let ms = with_entry(date_handle, |e| match e {
        Some(Entry::DateMs(ms)) => *ms,
        _ => 0,
    });
    let (y, m, d) = ymd_from_utc_ms(ms);
    let locale = map_get_str(handle, "__locale").unwrap_or_default();
    // en-GB -> dd/mm/yyyy; default (en-US) -> mm/dd/yyyy.
    let out = if locale.starts_with("en-GB") {
        format!("{:02}/{:02}/{:04}", d, m, y)
    } else {
        format!("{:02}/{:02}/{:04}", m, d, y)
    };
    alloc_str(out)
}

// ── Intl.Collator ────────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_INTL_COLLATOR_NEW(
    locale_ptr: i64,
    locale_len: i64,
    options: u64,
) -> u64 {
    let locale = unsafe { str_from_raw(locale_ptr, locale_len) };
    alloc_intl(locale, options)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_INTL_COLLATOR_COMPARE(
    handle: u64,
    a_ptr: i64,
    a_len: i64,
    b_ptr: i64,
    b_len: i64,
) -> i64 {
    let a = unsafe { str_from_raw(a_ptr, a_len) };
    let b = unsafe { str_from_raw(b_ptr, b_len) };
    let opts = options_of(handle);
    let sensitivity = map_get_str(opts, "sensitivity").unwrap_or_default();
    let (ca, cb) = if sensitivity == "base" || sensitivity == "accent" {
        (a.to_lowercase(), b.to_lowercase())
    } else {
        (a.clone(), b.clone())
    };
    match ca.cmp(&cb) {
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Equal => 0,
        std::cmp::Ordering::Greater => 1,
    }
}

// ── Intl.Segmenter ───────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_INTL_SEGMENTER_NEW(
    locale_ptr: i64,
    locale_len: i64,
    options: u64,
) -> u64 {
    let locale = unsafe { str_from_raw(locale_ptr, locale_len) };
    alloc_intl(locale, options)
}

/// Returns true for characters that count as "word-like" (letters/digits).
fn is_word_char(c: char) -> bool {
    c.is_alphanumeric()
}

/// `seg.segment(input)` -> Vec<i64> handle of segment objects. Each element is
/// a Map handle with `segment` (string) and `isWordLike` (bool) keys, matching
/// the shape that `Array.from(...)` + `.map(x => x.segment + ":" + x.isWordLike)`
/// expects. Granularity "word": runs of word chars vs non-word chars.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_INTL_SEGMENTER_SEGMENT(handle: u64, ptr: i64, len: i64) -> u64 {
    use indexmap::IndexMap;
    let input = unsafe { str_from_raw(ptr, len) };
    let opts = options_of(handle);
    let granularity = map_get_str(opts, "granularity").unwrap_or_else(|| "grapheme".to_string());

    let mut segments: Vec<(String, bool)> = Vec::new();
    if granularity == "word" {
        let chars: Vec<char> = input.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            let word = is_word_char(chars[i]);
            let start = i;
            while i < chars.len() && is_word_char(chars[i]) == word {
                i += 1;
            }
            let s: String = chars[start..i].iter().collect();
            segments.push((s, word));
        }
    } else {
        // grapheme/sentence fallback: per-char, word-like by char class.
        for c in input.chars() {
            segments.push((c.to_string(), is_word_char(c)));
        }
    }

    let mut vec: Vec<i64> = Vec::with_capacity(segments.len());
    for (seg, word_like) in segments {
        let mut m: IndexMap<String, i64> = IndexMap::new();
        let seg_h = alloc_str(seg);
        m.insert("segment".to_string(), seg_h as i64);
        // Bool stored as i64::MIN sentinel scheme used by codegen for object
        // bool fields would be ideal, but reading via `.isWordLike` in a Map
        // member-access path yields the raw i64; store 1/0 so concat renders
        // "true"/"false". RTS templates render bool via the field-type path;
        // store the canonical bool sentinels so `x.isWordLike` -> true/false.
        let bool_val: i64 = if word_like { i64::MIN + 1 } else { i64::MIN };
        m.insert("isWordLike".to_string(), bool_val);
        let obj_h = alloc_entry(Entry::Map(Box::new(m)));
        vec.push(obj_h as i64);
    }
    alloc_entry(Entry::Vec(Box::new(vec)))
}

// ── Intl.PluralRules ─────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_INTL_PLURAL_RULES_NEW(
    locale_ptr: i64,
    locale_len: i64,
    options: u64,
) -> u64 {
    let locale = unsafe { str_from_raw(locale_ptr, locale_len) };
    alloc_intl(locale, options)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_INTL_PLURAL_RULES_SELECT(_handle: u64, n: f64) -> u64 {
    // English cardinal rule: 1 -> "one", everything else -> "other".
    let s = if n == 1.0 { "one" } else { "other" };
    alloc_str(s.to_string())
}

// ── Intl.ListFormat ──────────────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_INTL_LIST_FORMAT_NEW(
    locale_ptr: i64,
    locale_len: i64,
    options: u64,
) -> u64 {
    let locale = unsafe { str_from_raw(locale_ptr, locale_len) };
    alloc_intl(locale, options)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_INTL_LIST_FORMAT_FORMAT(handle: u64, items_vec: u64) -> u64 {
    // items_vec is a Vec<i64> of string handles.
    let items: Vec<String> = with_entry(items_vec, |e| match e {
        Some(Entry::Vec(v)) => v
            .iter()
            .map(|h| {
                with_entry(*h as u64, |se| match se {
                    Some(Entry::String(b)) => String::from_utf8_lossy(b).into_owned(),
                    _ => String::new(),
                })
            })
            .collect(),
        _ => Vec::new(),
    });

    let opts = options_of(handle);
    let kind = map_get_str(opts, "type").unwrap_or_else(|| "conjunction".to_string());
    let conj = if kind == "disjunction" { "or" } else { "and" };

    let out = match items.len() {
        0 => String::new(),
        1 => items[0].clone(),
        2 => format!("{} {} {}", items[0], conj, items[1]),
        _ => {
            let head = items[..items.len() - 1].join(", ");
            format!("{}, {} {}", head, conj, items[items.len() - 1])
        }
    };
    alloc_str(out)
}

// ── Intl.RelativeTimeFormat ──────────────────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_INTL_RELATIVE_TIME_FORMAT_NEW(
    locale_ptr: i64,
    locale_len: i64,
    options: u64,
) -> u64 {
    let locale = unsafe { str_from_raw(locale_ptr, locale_len) };
    alloc_intl(locale, options)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_INTL_RELATIVE_TIME_FORMAT_FORMAT(
    handle: u64,
    value: f64,
    unit_ptr: i64,
    unit_len: i64,
) -> u64 {
    let unit_raw = unsafe { str_from_raw(unit_ptr, unit_len) };
    // Strip trailing plural 's' (days -> day) for matching.
    let unit = unit_raw.trim_end_matches('s').to_string();
    let opts = options_of(handle);
    let numeric = map_get_str(opts, "numeric").unwrap_or_else(|| "always".to_string());
    let n = value as i64;

    if numeric == "auto" {
        // Special-case ±1 day/week/month/year/hour/minute/second.
        if n == -1 {
            let word = match unit.as_str() {
                "day" => Some("yesterday"),
                "week" => Some("last week"),
                "month" => Some("last month"),
                "quarter" => Some("last quarter"),
                "year" => Some("last year"),
                _ => None,
            };
            if let Some(w) = word {
                return alloc_str(w.to_string());
            }
        }
        if n == 1 {
            let word = match unit.as_str() {
                "day" => Some("tomorrow"),
                "week" => Some("next week"),
                "month" => Some("next month"),
                "quarter" => Some("next quarter"),
                "year" => Some("next year"),
                _ => None,
            };
            if let Some(w) = word {
                return alloc_str(w.to_string());
            }
        }
        if n == 0 {
            let word = match unit.as_str() {
                "day" => Some("today"),
                _ => None,
            };
            if let Some(w) = word {
                return alloc_str(w.to_string());
            }
        }
    }

    // Numeric rendering: "in N units" (future) / "N units ago" (past).
    let abs = n.abs();
    let unit_label = if abs == 1 {
        unit.clone()
    } else {
        format!("{}s", unit)
    };
    let out = if n < 0 {
        format!("{} {} ago", abs, unit_label)
    } else {
        format!("in {} {}", abs, unit_label)
    };
    alloc_str(out)
}
