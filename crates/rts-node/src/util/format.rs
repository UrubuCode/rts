//! node:util — `format`/`formatWithOptions`: the printf-style formatter. Handles
//! `%s`/`%d`/`%i`/`%f`/`%j`/`%o`/`%O`/`%c`/`%%`; args beyond the consumed
//! specifiers are space-appended. Object stringification (`%o`/`%O`, and `%s`
//! of a non-primitive) uses `String(value)` — the full `util.inspect` renderer
//! is deferred.

use super::words::{word_to_number, word_to_string};

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
                    'j' | 'o' | 'O' => out.push_str(&word_to_string(a)),
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
    // Remaining args are space-appended.
    while ai < args.len() {
        out.push(' ');
        out.push_str(&word_to_string(args[ai]));
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
