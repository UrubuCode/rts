//! `inspect`, `format`, `formatWithOptions` — the value formatter, and what
//! stands on it.
//!
//! # Why this walks the structure instead of asking the value
//!
//! [`rts_core_rwk::entry::described`] answers `None` for an object — it is the
//! host's `ToString`, and an object's `ToString` runs user code an entry point
//! cannot call. That leaves every object printing as nothing useful anywhere a
//! program formats one, and `console.log`, `util.format` and `util.inspect` all
//! need the answer `described` correctly refuses to give. So this module walks
//! own keys and indexed reads, never a `ToString`, and is the only place in this
//! crate that does.
//!
//! # Why the walk is bounded
//!
//! `const a = {}; a.self = a;` is one line of JavaScript, and a formatter that
//! recursed into every property would not return. Two guards, matching Node:
//! `seen` catches an object revisited on the CURRENT path and answers
//! `[Circular]`, and a depth counter caps how far a non-cyclic object is walked
//! at all.
//!
//! Every function here is ambient — see [`super::values`] for the rule.

use rts_core_rwk::entry;

use super::values::{
    array_items, get, is_object_value, kind_of, length_of, number_of, option, own_key_strings,
    present, string, string_of,
};

/// Node's own default inspection depth, and this module's.
pub(super) const INSPECT_DEPTH: u32 = 2;

/// Node's default `maxArrayLength`.
const MAX_ARRAY: usize = 100;

/// What `depth: null` — "unlimited" — is actually walked to.
///
/// Unlimited is not implementable as written: `seen` guarantees termination on
/// a CYCLE, and nothing guarantees it on a deep acyclic structure, where an
/// unbounded recursion is a native stack overflow rather than a slow print. A
/// stated finite cap is the honest reading of "no limit"; the divergence is
/// named in the module's refusal list.
const UNLIMITED_DEPTH: u32 = 64;

/// The subset of `InspectOptions` this formatter honours.
#[derive(Clone, Copy)]
pub(super) struct Options {
    pub(super) depth: u32,
    pub(super) sorted: bool,
    pub(super) max_array: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self { depth: INSPECT_DEPTH, sorted: false, max_array: MAX_ARRAY }
    }
}

impl Options {
    /// The same options one level deeper in.
    fn inner(self) -> Self {
        Self { depth: self.depth.saturating_sub(1), ..self }
    }
}

/// `util.inspect(object[, options])`, and the legacy positional form
/// `util.inspect(object[, showHidden[, depth[, colors]]])`.
///
/// The two forms are told apart by whether the second argument is an object,
/// which is what Node itself does — `showHidden` is a boolean there and an
/// options bag is not.
pub(super) extern "C" fn inspect(
    _e: u64,
    _this: u64,
    value: u64,
    options: u64,
    depth: u64,
    _colors: u64,
) -> u64 {
    let mut seen = Vec::new();
    let options = read_options(options, depth);
    string(&format_value(value, options, &mut seen))
}

/// The options an argument asks for, defaulted for everything it omits.
pub(super) fn read_options(options: u64, legacy_depth: u64) -> Options {
    let mut held = Options::default();
    if !is_object_value(options) {
        // The legacy positional form: the third argument is `depth`.
        if let Some(depth) = number_of(legacy_depth) {
            held.depth = finite_depth(depth);
        }
        return held;
    }
    let asked = option(options, "depth");
    if asked == entry::null_value() {
        held.depth = UNLIMITED_DEPTH;
    } else if let Some(depth) = number_of(asked) {
        held.depth = finite_depth(depth);
    }
    let cap = option(options, "maxArrayLength");
    if cap == entry::null_value() {
        held.max_array = usize::MAX;
    } else if let Some(cap) = number_of(cap) {
        held.max_array = cap.max(0.0) as usize;
    }
    // `to_boolean` and not a bit comparison: `sorted` is documented as
    // `boolean | comparator`, and a comparator is truthy — this module cannot
    // call one, so it sorts by the default order and says so.
    held.sorted = entry::to_boolean(option(options, "sorted"));
    held
}

/// A requested depth as a walk bound. A negative depth means "print nothing but
/// the kind", which is `0` here.
fn finite_depth(depth: f64) -> u32 {
    match depth.is_finite() {
        true => depth.max(0.0).min(f64::from(UNLIMITED_DEPTH)) as u32,
        false => UNLIMITED_DEPTH,
    }
}

/// One value, formatted the way `util.inspect` and `util.format`'s `%O` do.
pub(super) fn format_value(value: u64, options: Options, seen: &mut Vec<u64>) -> String {
    if value == entry::undefined_value() {
        return "undefined".to_owned();
    }
    if value == entry::null_value() {
        return "null".to_owned();
    }
    match kind_of(value).as_str() {
        "string" => quoted(&string_of(value).unwrap_or_default()),
        "number" | "boolean" => entry::described(value).unwrap_or_else(|| "NaN".to_owned()),
        "bigint" => format!("{}n", entry::described(value).unwrap_or_default()),
        "symbol" => entry::described(value).unwrap_or_else(|| "Symbol()".to_owned()),
        "function" => match string_of(get(value, "name")).filter(|name| !name.is_empty()) {
            Some(name) => format!("[Function: {name}]"),
            None => "[Function (anonymous)]".to_owned(),
        },
        _ => format_object(value, options, seen),
    }
}

/// A JavaScript string literal, single-quoted the way Node's inspector writes
/// one.
fn quoted(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('\'');
    for character in text.chars() {
        match character {
            '\'' => out.push_str("\\'"),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(character),
        }
    }
    out.push('\'');
    out
}

/// The object case of [`format_value`] — an array as `[ 1, 2 ]`, a plain object
/// as `{ a: 1 }`.
fn format_object(value: u64, options: Options, seen: &mut Vec<u64>) -> String {
    if seen.contains(&value) {
        return "[Circular]".to_owned();
    }
    // `is_array` is the real `Array.isArray`, not a shape guess: the host
    // surface exposes the array side table now, so `{0: "a"}` no longer prints
    // as an array the way an earlier structural test here made it.
    let array = entry::is_array(value);
    if options.depth == 0 {
        return match array {
            true => "[Array]".to_owned(),
            false => "[Object]".to_owned(),
        };
    }
    seen.push(value);
    let rendered = match array {
        true => format_elements(value, options, seen),
        false => format_members(value, options, seen),
    };
    seen.pop();
    rendered
}

/// The elements of a real array, capped at `maxArrayLength`.
fn format_elements(value: u64, options: Options, seen: &mut Vec<u64>) -> String {
    let length = length_of(value);
    let shown = length.min(options.max_array);
    let mut parts: Vec<String> = (0..shown)
        .map(|index| format_value(get(value, &index.to_string()), options.inner(), seen))
        .collect();
    if length > shown {
        let more = length - shown;
        let plural = match more == 1 {
            true => "item",
            false => "items",
        };
        parts.push(format!("... {more} more {plural}"));
    }
    wrapped('[', ']', &parts)
}

/// The own enumerable properties of a plain object.
fn format_members(value: u64, options: Options, seen: &mut Vec<u64>) -> String {
    let mut keys = own_key_strings(value);
    if options.sorted {
        keys.sort();
    }
    let parts: Vec<String> = keys
        .iter()
        .map(|key| {
            let rendered = format_value(get(value, key), options.inner(), seen);
            format!("{}: {rendered}", key_text(key))
        })
        .collect();
    wrapped('{', '}', &parts)
}

/// A property name as Node writes it: bare when it is an identifier, quoted
/// otherwise, so `{ "a b": 1 }` does not print as something that would not
/// parse back.
fn key_text(key: &str) -> String {
    let identifier = !key.is_empty()
        && !key.starts_with(|c: char| c.is_ascii_digit())
        && key.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '$');
    match identifier {
        true => key.to_owned(),
        false => quoted(key),
    }
}

/// Entries between their brackets, with Node's inner spacing.
fn wrapped(open: char, close: char, parts: &[String]) -> String {
    match parts.is_empty() {
        true => format!("{open}{close}"),
        false => format!("{open} {} {close}", parts.join(", ")),
    }
}

/// `util.format(fmt, a, b, c)` — three substitution arguments, because the
/// fourth of the convention's four slots is the format string itself. A program
/// needing more is not silently truncated: the extras are simply not there to
/// read, and the limit is named in the module's refusal list.
pub(super) extern "C" fn format(_e: u64, _this: u64, a: u64, b: u64, c: u64, d: u64) -> u64 {
    string(&formatted(Options::default(), a, [b, c, d]))
}

/// `util.formatWithOptions(inspectOptions, format, a, b)` — one substitution
/// argument fewer than [`format`], because the options bag takes a slot.
pub(super) extern "C" fn format_with_options(
    _e: u64,
    _this: u64,
    options: u64,
    pattern: u64,
    a: u64,
    b: u64,
) -> u64 {
    let options = read_options(options, entry::undefined_value());
    string(&formatted(options, pattern, [a, b, entry::undefined_value()]))
}

/// The shared body of `format` and `formatWithOptions`.
///
/// # The two cases a naive implementation gets wrong
///
/// A `%s` with no argument left is printed LITERALLY — `format("%s %s", "a")` is
/// `"a %s"`, not `"a undefined"` — and an argument past the last specifier is
/// appended, space-separated, rather than dropped. Both are Node's behaviour and
/// neither falls out of "walk the string and substitute".
pub(super) fn formatted(options: Options, pattern: u64, rest: [u64; 3]) -> String {
    let mut rest: std::collections::VecDeque<u64> =
        rest.into_iter().filter(|value| present(*value)).collect();
    let Some(pattern) = string_of(pattern) else {
        // The first argument is not a string: Node inspects and joins every
        // argument given rather than refusing the call.
        let mut parts = Vec::new();
        if present(pattern) {
            let mut seen = Vec::new();
            parts.push(format_value(pattern, options, &mut seen));
        }
        parts.extend(rest.iter().map(|value| {
            let mut seen = Vec::new();
            format_value(*value, options, &mut seen)
        }));
        return parts.join(" ");
    };

    let mut out = String::new();
    let mut chars = pattern.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '%' {
            out.push(ch);
            continue;
        }
        match chars.peek().copied() {
            Some('%') => {
                out.push('%');
                chars.next();
            }
            Some(spec @ ('s' | 'd' | 'i' | 'f' | 'j' | 'o' | 'O' | 'c')) => {
                chars.next();
                match rest.pop_front() {
                    // No argument left: the specifier stays literal text.
                    None => {
                        out.push('%');
                        out.push(spec);
                    }
                    // `%c` consumes its argument and emits nothing — it is a
                    // browser CSS directive with no terminal meaning.
                    Some(_) if spec == 'c' => {}
                    Some(arg) => out.push_str(&render_specifier(spec, arg, options)),
                }
            }
            _ => out.push('%'),
        }
    }
    // Anything left over is not dropped — it is Node's trailing,
    // `console.log`-style tail.
    for arg in rest {
        let mut seen = Vec::new();
        out.push(' ');
        out.push_str(&format_value(arg, options, &mut seen));
    }
    out
}

/// One `%` specifier substituted for one argument.
fn render_specifier(spec: char, arg: u64, options: Options) -> String {
    let mut seen = Vec::new();
    match spec {
        // `%s` prints a string bare — unquoted — and inspects anything else.
        's' => match string_of(arg) {
            Some(text) => text,
            None => format_value(arg, options, &mut seen),
        },
        // `%j` is `JSON.stringify`, which is a different grammar from
        // `inspect`'s: double quotes, no trailing commentary, and `[Circular]`
        // where `JSON.stringify` would throw.
        'j' => json_text(arg, &mut seen).unwrap_or_else(|| "undefined".to_owned()),
        'o' | 'O' => format_value(arg, options, &mut seen),
        'i' => match number_of(arg) {
            Some(number) => format!("{}", number.trunc()),
            None => "NaN".to_owned(),
        },
        'd' | 'f' => match kind_of(arg).as_str() {
            "bigint" => format!("{}n", entry::described(arg).unwrap_or_default()),
            _ => match number_of(arg) {
                Some(number) => entry::described(entry::make_number(number))
                    .unwrap_or_else(|| "NaN".to_owned()),
                None => "NaN".to_owned(),
            },
        },
        _ => unreachable!("render_specifier is only called for a matched specifier"),
    }
}

/// `JSON.stringify(value)` over the same structural walk, for `%j`.
///
/// `None` where `JSON.stringify` answers `undefined` — a function, a symbol, or
/// `undefined` itself. Written here rather than reached for because
/// `rts-core-rwk`'s `JSON` is a class installed for the PROGRAM to call, and a
/// native calling it would be calling user-reachable code from a native frame.
fn json_text(value: u64, seen: &mut Vec<u64>) -> Option<String> {
    if value == entry::undefined_value() {
        return None;
    }
    if value == entry::null_value() {
        return Some("null".to_owned());
    }
    match kind_of(value).as_str() {
        "string" => Some(json_string(&string_of(value).unwrap_or_default())),
        "boolean" => entry::described(value),
        // Non-finite numbers are `null` in JSON, not `NaN`.
        "number" => Some(match number_of(value).filter(|held| held.is_finite()) {
            Some(_) => entry::described(value).unwrap_or_else(|| "null".to_owned()),
            None => "null".to_owned(),
        }),
        "function" | "symbol" | "undefined" => None,
        // `JSON.stringify` throws on a cycle; `%j` is documented to print this
        // literal instead, which is why the flat form lives here and the
        // inspector's lives above.
        _ if seen.contains(&value) => Some("[Circular]".to_owned()),
        _ => {
            seen.push(value);
            let text = match entry::is_array(value) {
                true => {
                    let parts: Vec<String> = array_items(value)
                        .into_iter()
                        .map(|item| json_text(item, seen).unwrap_or_else(|| "null".to_owned()))
                        .collect();
                    format!("[{}]", parts.join(","))
                }
                false => {
                    let parts: Vec<String> = own_key_strings(value)
                        .into_iter()
                        .filter_map(|key| {
                            let rendered = json_text(get(value, &key), seen)?;
                            Some(format!("{}:{rendered}", json_string(&key)))
                        })
                        .collect();
                    format!("{{{}}}", parts.join(","))
                }
            };
            seen.pop();
            Some(text)
        }
    }
}

/// A JSON string literal.
fn json_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            control if (control as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", control as u32));
            }
            _ => out.push(character),
        }
    }
    out.push('"');
    out
}
