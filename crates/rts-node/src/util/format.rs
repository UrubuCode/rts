//! node:util — `format`/`formatWithOptions`: the printf-style formatter. Handles
//! `%s`/`%d`/`%i`/`%f`/`%j`/`%o`/`%O`/`%c`/`%%`; args beyond the consumed
//! specifiers are space-appended. `%o`/`%O` render through the real
//! `util.inspect`; a space-appended non-string extra arg does too (matching
//! Node, which inspects trailing objects).

use super::inspect::inspect;
use super::words::{word_to_number, word_to_string};

/// Whether a word is a heap object/array (renders via `inspect` when trailing).
fn is_object(w: u64) -> bool {
    use rts_engine::heap::poly::{POLY_BOX_BASE, POLY_TAG_MASK, POLY_TAG_SHIFT};
    (w & POLY_BOX_BASE) == POLY_BOX_BASE && ((w >> POLY_TAG_SHIFT) & POLY_TAG_MASK) == 4
}

/// `util.format(format, ...args)`.
pub fn format(fmt: &str, args: &[u64]) -> String {
    let chars: Vec<char> = fmt.chars().collect();
    let mut out = String::new();
    let mut ai = 0usize;
    let mut i = 0usize;
    while i < chars.len() {
        if chars[i] == '%' && i + 1 < chars.len() {
            let spec = chars[i + 1];
            if spec == '%' {
                out.push('%');
                i += 2;
                continue;
            }
            if matches!(spec, 's' | 'd' | 'i' | 'f' | 'j' | 'o' | 'O' | 'c') {
                if ai >= args.len() {
                    // No argument left for this specifier — emit it literally.
                    out.push('%');
                    out.push(spec);
                    i += 2;
                    continue;
                }
                let a = args[ai];
                ai += 1;
                match spec {
                    's' => out.push_str(&word_to_string(a)),
                    'd' => out.push_str(&fmt_d(word_to_number(a))),
                    'i' => out.push_str(&fmt_i(word_to_number(a))),
                    'f' => out.push_str(&fmt_f(word_to_number(a))),
                    'o' | 'O' => out.push_str(&inspect(a)),
                    'j' => out.push_str(&word_to_string(a)),
                    'c' => {} // CSS directive: consumes the arg, emits nothing.
                    _ => {}
                }
                i += 2;
                continue;
            }
            // Unknown specifier — keep the '%'.
            out.push('%');
            i += 1;
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    // Remaining args are space-appended (objects inspected, like Node).
    while ai < args.len() {
        out.push(' ');
        let a = args[ai];
        if is_object(a) {
            out.push_str(&inspect(a));
        } else {
            out.push_str(&word_to_string(a));
        }
        ai += 1;
    }
    out
}

fn fmt_d(n: f64) -> String {
    if n.is_nan() {
        "NaN".to_string()
    } else if n.fract() == 0.0 && n.is_finite() {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

fn fmt_i(n: f64) -> String {
    if n.is_nan() || !n.is_finite() {
        "NaN".to_string()
    } else {
        format!("{}", n.trunc() as i64)
    }
}

fn fmt_f(n: f64) -> String {
    if n.is_nan() {
        "NaN".to_string()
    } else {
        format!("{n}")
    }
}
