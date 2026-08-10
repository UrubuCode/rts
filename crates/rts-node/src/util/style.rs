//! Terminal styling and the small pure parsers: `styleText`,
//! `stripVTControlCharacters`, `parseEnv`.
//!
//! # Why the ANSI table is here and stated once
//!
//! `util.inspect.colors` is a documented, program-readable table, and
//! `styleText` is the function that applies it. Two copies of the pair
//! `[open, close]` would be two answers to what `red` is, and a program that
//! reads `util.inspect.colors.red` to build its own escape would then disagree
//! with `util.styleText("red", …)`. So the table below is the only one, and the
//! namespace member is built out of it.

use rts_core::entry;

use super::values::{is_object_value, option, string, string_items, string_of};

/// The SGR pairs, exactly Node's `util.inspect.colors`.
const COLORS: &[(&str, u8, u8)] = &[
    ("reset", 0, 0),
    ("bold", 1, 22),
    ("dim", 2, 22),
    ("italic", 3, 23),
    ("underline", 4, 24),
    ("blink", 5, 25),
    ("inverse", 7, 27),
    ("hidden", 8, 28),
    ("strikethrough", 9, 29),
    ("strikeThrough", 9, 29),
    ("doubleunderline", 21, 24),
    ("framed", 51, 54),
    ("overlined", 53, 55),
    ("black", 30, 39),
    ("red", 31, 39),
    ("green", 32, 39),
    ("yellow", 33, 39),
    ("blue", 34, 39),
    ("magenta", 35, 39),
    ("cyan", 36, 39),
    ("white", 37, 39),
    ("gray", 90, 39),
    ("grey", 90, 39),
    ("blackBright", 90, 39),
    ("redBright", 91, 39),
    ("greenBright", 92, 39),
    ("yellowBright", 93, 39),
    ("blueBright", 94, 39),
    ("magentaBright", 95, 39),
    ("cyanBright", 96, 39),
    ("whiteBright", 97, 39),
    ("bgBlack", 40, 49),
    ("bgRed", 41, 49),
    ("bgGreen", 42, 49),
    ("bgYellow", 43, 49),
    ("bgBlue", 44, 49),
    ("bgMagenta", 45, 49),
    ("bgCyan", 46, 49),
    ("bgWhite", 47, 49),
    ("bgGray", 100, 49),
    ("bgGrey", 100, 49),
    ("bgBlackBright", 100, 49),
    ("bgRedBright", 101, 49),
    ("bgGreenBright", 102, 49),
    ("bgYellowBright", 103, 49),
    ("bgBlueBright", 104, 49),
    ("bgMagentaBright", 105, 49),
    ("bgCyanBright", 106, 49),
    ("bgWhiteBright", 107, 49),
];

/// `util.inspect.styles` — semantic name to a key of [`COLORS`].
const STYLES: &[(&str, &str)] = &[
    ("bigint", "yellow"),
    ("boolean", "yellow"),
    ("date", "magenta"),
    ("module", "underline"),
    ("name", "undefined"),
    ("null", "bold"),
    ("number", "yellow"),
    ("regexp", "red"),
    ("special", "cyan"),
    ("string", "green"),
    ("symbol", "green"),
    ("undefined", "grey"),
];

/// `util.inspect.colors`, as a value.
pub(super) fn colors_object(context: &mut entry::Context) -> u64 {
    let object = entry::make_object(context);
    for (name, open, close) in COLORS {
        let pair = vec![
            entry::make_number(f64::from(*open)),
            entry::make_number(f64::from(*close)),
        ];
        let pair = entry::make_array_in(context, pair);
        entry::put_member(context, object, name, pair);
    }
    object
}

/// `util.inspect.styles`, as a value.
pub(super) fn styles_object(context: &mut entry::Context) -> u64 {
    let object = entry::make_object(context);
    for (name, color) in STYLES {
        let value = entry::make_string(context, color);
        entry::put_member(context, object, name, value);
    }
    object
}

/// `util.styleText(format, text[, options])`.
///
/// # Why the stream capability is asked of this process's stdout
///
/// `validateStream` defaults to true, and what it validates is whether the
/// destination can show colour at all. `options.stream` is a JavaScript stream
/// object, and nothing on the host surface maps one back to a file descriptor —
/// so the probe here is always this process's stdout, and a caller passing
/// `process.stderr` gets stdout's answer. Named in the module doc; the rejected
/// alternative was ignoring `validateStream` entirely, which would emit escape
/// codes into a redirected file and corrupt it.
pub(super) extern "C" fn style_text(
    _e: u64,
    _this: u64,
    format: u64,
    text: u64,
    options: u64,
    _d: u64,
) -> u64 {
    let Some(text) = string_of(text) else {
        return entry::undefined_value();
    };
    let names = match string_of(format) {
        Some(one) => vec![one],
        None => string_items(format),
    };
    // Every name must be known: Node throws for an unrecognised one, and
    // styling with the subset it did recognise would be a partial answer that
    // looks complete.
    let mut pairs = Vec::with_capacity(names.len());
    for name in &names {
        let Some(pair) = COLORS.iter().find(|(held, _, _)| *held == name.as_str()) else {
            return entry::undefined_value();
        };
        pairs.push(pair);
    }
    if pairs.is_empty() {
        return entry::undefined_value();
    }
    let validate = match is_object_value(options) {
        true => entry::to_boolean(option(options, "validateStream")),
        false => true,
    };
    if validate && !colour_capable() {
        return string(&text);
    }
    let mut out = String::new();
    for (_, open, _) in &pairs {
        out.push_str(&format!("\u{1b}[{open}m"));
    }
    out.push_str(&text);
    for (_, _, close) in pairs.iter().rev() {
        out.push_str(&format!("\u{1b}[{close}m"));
    }
    string(&out)
}

/// Whether this process's stdout is a terminal that can show colour.
///
/// `std::io::IsTerminal` and not an environment probe: `NO_COLOR`/`TERM` say
/// what a user WANTS, and this question is what the destination CAN do. The
/// preference belongs to whoever writes, and `node:tty` is where it is asked.
fn colour_capable() -> bool {
    use std::io::IsTerminal;
    std::io::stdout().is_terminal()
}

/// `util.stripVTControlCharacters(str)`.
///
/// A hand-written scan rather than a regular expression: this crate's `RegExp`
/// is the program's, reachable only by calling into user-visible code from a
/// native frame, and the grammar is small — `ESC [` then parameter and
/// intermediate bytes then one final byte, plus the two-character `ESC` forms.
pub(super) extern "C" fn strip_vt(
    _e: u64,
    _this: u64,
    value: u64,
    _b: u64,
    _c: u64,
    _d: u64,
) -> u64 {
    let Some(text) = string_of(value) else {
        return entry::undefined_value();
    };
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(character) = chars.next() {
        if character != '\u{1b}' {
            out.push(character);
            continue;
        }
        match chars.next() {
            // CSI: parameters and intermediates, then one final byte in
            // `@`..`~` that ends the sequence.
            Some('[') => {
                for held in chars.by_ref() {
                    if ('\u{40}'..='\u{7e}').contains(&held) {
                        break;
                    }
                }
            }
            // OSC: ends at BEL, or at `ESC \`.
            Some(']') => {
                while let Some(held) = chars.next() {
                    if held == '\u{7}' {
                        break;
                    }
                    if held == '\u{1b}' {
                        chars.next();
                        break;
                    }
                }
            }
            // Any other two-character escape is consumed whole.
            Some(_) | None => {}
        }
    }
    string(&out)
}

/// `util.parseEnv(content)` — the `.env` dialect, as an object.
pub(super) extern "C" fn parse_env(
    _e: u64,
    _this: u64,
    content: u64,
    _b: u64,
    _c: u64,
    _d: u64,
) -> u64 {
    let Some(text) = string_of(content) else {
        return entry::undefined_value();
    };
    let pairs = env_pairs(&text);
    entry::with_runtime(|context| {
        let object = entry::make_object(context);
        for (key, value) in pairs {
            let value = entry::make_string(context, &value);
            entry::put_member(context, object, &key, value);
        }
        object
    })
}

/// The parse itself, in Rust, so the value building above happens in one borrow.
///
/// The dialect: blank lines and `#` comments are skipped, an `export ` prefix is
/// tolerated, a single- or double-quoted value may span lines, and an unquoted
/// value is trimmed and loses a trailing `#` comment.
fn env_pairs(text: &str) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    let mut at = 0usize;
    while at < text.len() {
        let line_end = line_end_from(text, at);
        let line = text[at..line_end].trim_end_matches('\r');
        let mut next = past_newline(text, line_end);
        let statement = line.trim_start();
        let statement = statement.strip_prefix("export ").unwrap_or(statement);
        if statement.is_empty() || statement.starts_with('#') {
            at = next;
            continue;
        }
        if let Some((name, value)) = statement.split_once('=') {
            let name = name.trim();
            let value = value.trim_start();
            if !name.is_empty() {
                match value.chars().next() {
                    Some(quote @ ('"' | '\'')) => {
                        // Searched over the WHOLE remaining content and not
                        // over the line: a quoted value is allowed to span
                        // lines, and truncating it at the newline is the one
                        // thing a line-by-line parser gets silently wrong.
                        let opened = offset_in(text, value) + quote.len_utf8();
                        let (held, after) = match text[opened..].find(quote) {
                            Some(found) => {
                                (&text[opened..opened + found], opened + found + quote.len_utf8())
                            }
                            // No closing quote at all: the rest of the line,
                            // verbatim, which is what leaving it unterminated
                            // means.
                            None => (&text[opened..line_end], line_end),
                        };
                        pairs.push((name.to_owned(), held.to_owned()));
                        next = past_newline(text, line_end_from(text, after));
                    }
                    // An unquoted value ends at a `#` comment and is trimmed.
                    _ => {
                        let bare = value.split('#').next().unwrap_or("").trim_end();
                        pairs.push((name.to_owned(), bare.to_owned()));
                    }
                }
            }
        }
        at = next;
    }
    pairs
}

/// Where the line containing `at` ends.
fn line_end_from(text: &str, at: usize) -> usize {
    text[at..].find('\n').map_or(text.len(), |found| at + found)
}

/// The start of the next line, given where this one ended.
fn past_newline(text: &str, line_end: usize) -> usize {
    (line_end + 1).min(text.len()).max(line_end)
}

/// Where a subslice starts inside the text it was cut from.
///
/// By address, because `trim_start`/`split_once` hand back borrows of the
/// original and re-searching for the text would find the wrong occurrence when
/// the same value appears twice.
fn offset_in(text: &str, part: &str) -> usize {
    part.as_ptr() as usize - text.as_ptr() as usize
}
