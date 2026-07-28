//! node:util — value/string helpers: the `ToString` bridge (for `format`), a
//! word→number decode, GC interning, and the pure string utilities
//! (`toUSVString`/`stripVTControlCharacters`/`getSystemErrorName`/`styleText`).

use rts_engine::heap::handles::read_string_handle;
use rts_engine::heap::poly::{poly_handle_normalize, POLY_BOX_BASE, POLY_PAYLOAD_MASK};

use rts_engine::gc_surface::__RTS_FN_NS_GC_STRING_NEW;

unsafe extern "C" {
    fn __rtsadp_to_string(a: u64) -> u64;
}

/// Intern a Rust string → GC string handle.
pub fn intern(s: &str) -> u64 {
    unsafe { __RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64) }
}

/// JS `String(value)` of a value word.
pub fn word_to_string(w: u64) -> String {
    let sw = unsafe { __rtsadp_to_string(w) };
    match poly_handle_normalize(sw) {
        Some(h) => read_string_handle(h).unwrap_or_default(),
        None => String::new(),
    }
}

/// Numeric value of a word (inline double, boxed INT32, else via ToString parse).
pub fn word_to_number(w: u64) -> f64 {
    if (w & POLY_BOX_BASE) != POLY_BOX_BASE {
        return f64::from_bits(w);
    }
    // A boxed INT32 payload is the low 32 bits, sign-extended.
    if let Some(_h) = poly_handle_normalize(w) {
        // A heap value (string/object) — ToString then parse.
        return word_to_string(w).trim().parse::<f64>().unwrap_or(f64::NAN);
    }
    (w & POLY_PAYLOAD_MASK) as u32 as i32 as f64
}

/// `util.toUSVString(str)` — replace lone surrogates with U+FFFD. A string that
/// reached here as engine UTF-8 has no lone surrogates, so this is identity;
/// kept for API completeness / future UTF-16-native strings.
pub fn to_usv_string(s: &str) -> String {
    s.to_string()
}

/// `util.stripVTControlCharacters(str)` — remove ANSI/VT escape sequences
/// (`ESC [ … final-byte`, and the two-char `ESC X` forms).
pub fn strip_vt(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\u{1b}' && i + 1 < chars.len() {
            if chars[i + 1] == '[' {
                // CSI: skip until a final byte in @..~ (0x40..0x7e).
                i += 2;
                while i < chars.len() && !('\u{40}'..='\u{7e}').contains(&chars[i]) {
                    i += 1;
                }
                i += 1; // consume the final byte
                continue;
            }
            // Two-char escape (ESC X).
            i += 2;
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// `util.getSystemErrorName(err)` — a libuv-style negative errno → its `E*` name.
pub fn error_name(err: i64) -> String {
    let code = (-err) as i32;
    let name = match code {
        1 => "EPERM",
        2 => "ENOENT",
        3 => "ESRCH",
        4 => "EINTR",
        5 => "EIO",
        9 => "EBADF",
        11 => "EAGAIN",
        12 => "ENOMEM",
        13 => "EACCES",
        16 => "EBUSY",
        17 => "EEXIST",
        20 => "ENOTDIR",
        21 => "EISDIR",
        22 => "EINVAL",
        24 => "EMFILE",
        28 => "ENOSPC",
        32 => "EPIPE",
        36 => "ENAMETOOLONG",
        39 => "ENOTEMPTY",
        104 => "ECONNRESET",
        110 => "ETIMEDOUT",
        111 => "ECONNREFUSED",
        _ => "",
    };
    if name.is_empty() {
        format!("Unknown system error {err}")
    } else {
        name.to_string()
    }
}

/// `util.styleText(format, text)` — wrap `text` in the named ANSI style's SGR
/// codes. Unknown style → the text unchanged.
pub fn style_text(format: &str, text: &str) -> String {
    let open = match format {
        "reset" => 0,
        "bold" => 1,
        "dim" => 2,
        "italic" => 3,
        "underline" => 4,
        "inverse" => 7,
        "hidden" => 8,
        "strikethrough" => 9,
        "black" => 30,
        "red" => 31,
        "green" => 32,
        "yellow" => 33,
        "blue" => 34,
        "magenta" => 35,
        "cyan" => 36,
        "white" => 37,
        "gray" | "grey" => 90,
        _ => return text.to_string(),
    };
    // Close code: modifiers reset with their specific close; colors with 39.
    let close = match open {
        1 | 2 => 22,
        3 => 23,
        4 => 24,
        7 => 27,
        8 => 28,
        9 => 29,
        0 => 0,
        _ => 39,
    };
    format!("\u{1b}[{open}m{text}\u{1b}[{close}m")
}
