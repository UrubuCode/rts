//! `node:util` — a value formatter, and what stands on it.
//!
//! # Why `inspect` is the reason this file exists
//!
//! [`rts_core_rwk::entry::described`] answers `None` for an object — it is the
//! host's `ToString`, and an object's `ToString` runs user code an entry point
//! cannot call. That leaves every object printing as nothing useful anywhere a
//! program formats one, and `console.log`, `util.format` and `util.inspect` all
//! need the answer `described` correctly refuses to give. So this module walks
//! the STRUCTURE instead — own keys and indexed reads, never a `ToString` — and
//! is the only place in this crate that does.
//!
//! # Why the walk is bounded
//!
//! A cyclic object is not exotic: `const a = {}; a.self = a;` is one line of
//! JavaScript, and a formatter that recursed into every property would not
//! return. Two guards, matching what Node does: `seen` catches an object
//! revisited on the CURRENT path and answers `[Circular]`, and a depth counter
//! caps how far a non-cyclic object is walked at all — Node's own default
//! inspection depth is 2, so that is the cap here too, and a deeper object
//! answers `[Object]` or `[Array]` rather than its contents.
//!
//! # Why arrays are detected structurally
//!
//! The entry-point surface this module is built from does not expose the side
//! table `Array.isArray` reads — that lives in `rts-core-rwk`'s array-prototype
//! implementation and is not part of the API this crate is written against. So
//! an object is printed as an array when its own enumerable keys are exactly
//! `"0"`, `"1"`, … in order; a plain object shaped the same way — `{0: "a", 1:
//! "b"}` — prints as an array too. Named because it is the one place this
//! module's answer can disagree with `Array.isArray`'s.
//!
//! # Not implemented, by name
//!
//! `promisify`, `callbackify`, `deprecate`, `inherits`, `TextEncoder`,
//! `TextDecoder`, `parseArgs`, `styleText`, `debuglog`, `MIMEType`, and
//! `types` beyond `isArray` — each answers `undefined`, which a program sees,
//! rather than a plausible wrong answer.

use rts_core_rwk::entry::{Context, Provided};

/// Node's own default inspection depth, and this module's.
const INSPECT_DEPTH: u32 = 2;

/// The cap `isDeepStrictEqual` walks to.
///
/// Not `INSPECT_DEPTH`: a comparison is not a print, and a real program
/// compares structures deeper than two levels far more often than it prints
/// one that deep. The cap exists for the same reason `INSPECT_DEPTH` does — a
/// cyclic object is valid input — not to match a formatting default.
const EQUAL_DEPTH: u32 = 32;

/// The namespace `node:util` is.
pub fn namespace(context: &mut Context) -> u64 {
    let members: &[(&str, Provided)] = &[
        ("inspect", inspect),
        ("format", format),
        ("isDeepStrictEqual", is_deep_strict_equal),
    ];
    let namespace = rts_core_rwk::entry::make_namespace(context, members);
    let types = build_types(context);
    rts_core_rwk::entry::put_member(context, namespace, "types", types);
    namespace
}

/// `util.types` — a fraction of Node's, see the module doc for the rest.
fn build_types(context: &mut Context) -> u64 {
    let members: &[(&str, Provided)] = &[("isArray", types_is_array)];
    rts_core_rwk::entry::make_namespace(context, members)
}

/// `util.inspect(value)`.
extern "C" fn inspect(_e: u64, _this: u64, value: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let mut seen = Vec::new();
    string(&format_value(value, INSPECT_DEPTH, &mut seen))
}

/// `util.types.isArray(value)` — structural, see the module doc for why.
extern "C" fn types_is_array(_e: u64, _this: u64, value: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    bool_value(is_array_like(value))
}

/// `util.format(fmt, a, b, c)` — three substitution arguments, because the
/// fourth of the convention's four slots is the format string itself.
///
/// # The two cases a naive implementation gets wrong
///
/// A `%s` with no argument left is printed LITERALLY — `format("%s %s", "a")`
/// is `"a %s"`, not `"a undefined"` — and an argument past the last specifier
/// is appended, space-separated, rather than dropped. Both are Node's
/// behaviour and neither falls out of "walk the string and substitute".
extern "C" fn format(_e: u64, _this: u64, a: u64, b: u64, c: u64, d: u64) -> u64 {
    let undefined = rts_core_rwk::entry::undefined_value();
    let Some(pattern) = string_of(a) else {
        // The first argument is not a string: Node inspects and joins every
        // argument given rather than refusing the call.
        let parts: Vec<String> = [a, b, c, d]
            .into_iter()
            .filter(|value| *value != undefined)
            .map(|value| {
                let mut seen = Vec::new();
                format_value(value, INSPECT_DEPTH, &mut seen)
            })
            .collect();
        return string(&parts.join(" "));
    };

    let mut rest: std::collections::VecDeque<u64> = [b, c, d]
        .into_iter()
        .filter(|value| *value != undefined)
        .collect();

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
            Some(spec @ ('s' | 'd' | 'i' | 'f' | 'j' | 'o' | 'O')) => {
                chars.next();
                match rest.pop_front() {
                    // No argument left: the specifier stays literal text.
                    None => {
                        out.push('%');
                        out.push(spec);
                    }
                    Some(arg) => out.push_str(&render_specifier(spec, arg)),
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
        out.push_str(&format_value(arg, INSPECT_DEPTH, &mut seen));
    }
    string(&out)
}

/// One `%` specifier substituted for one argument.
fn render_specifier(spec: char, arg: u64) -> String {
    let mut seen = Vec::new();
    match spec {
        // `%s` prints a string bare — unquoted — and inspects anything else.
        's' if kind_of(arg) == "string" => rts_core_rwk::entry::described(arg).unwrap_or_default(),
        's' | 'j' | 'o' | 'O' => format_value(arg, INSPECT_DEPTH, &mut seen),
        'd' | 'i' | 'f' => rts_core_rwk::entry::described(arg).unwrap_or_else(|| "NaN".to_owned()),
        _ => unreachable!("render_specifier is only called for a matched specifier"),
    }
}

/// `util.isDeepStrictEqual(a, b)`.
extern "C" fn is_deep_strict_equal(_e: u64, _this: u64, a: u64, b: u64, _a2: u64, _a3: u64) -> u64 {
    bool_value(deep_equal(a, b, EQUAL_DEPTH))
}

/// One value, formatted the way `util.inspect` and `util.format`'s `%o` do.
///
/// # The rule this crate is written around
///
/// A native must not call user code while holding a runtime borrow: entry
/// points such as [`rts_core_rwk::entry::with_runtime`] hold a `RefCell`
/// borrow for their body, and an `extern "C"` frame cannot unwind past a
/// panic — a re-entrant borrow here ABORTS the process rather than erroring.
/// `own_keys` and `get_indexed` are entry points that take their OWN borrow
/// internally, which is why every call to them in this file stands alone,
/// never nested inside a `with_runtime` this file opened itself.
fn format_value(value: u64, depth: u32, seen: &mut Vec<u64>) -> String {
    let undefined = rts_core_rwk::entry::undefined_value();
    let null = rts_core_rwk::entry::null_value();
    if value == undefined {
        return "undefined".to_owned();
    }
    if value == null {
        return "null".to_owned();
    }
    match kind_of(value).as_str() {
        "string" => format!("'{}'", rts_core_rwk::entry::described(value).unwrap_or_default()),
        "number" | "boolean" | "bigint" => {
            rts_core_rwk::entry::described(value).unwrap_or_else(|| "NaN".to_owned())
        }
        "symbol" => "Symbol()".to_owned(),
        // A name is not reachable from this crate's API without calling back
        // into the function to read one — so every function prints alike,
        // which is the divergence from Node's `[Function: name]`.
        "function" => "[Function]".to_owned(),
        _ => format_object(value, depth, seen),
    }
}

/// The object case of [`format_value`] — an array as `[ 1, 2 ]`, a plain
/// object as `{ a: 1 }`.
fn format_object(value: u64, depth: u32, seen: &mut Vec<u64>) -> String {
    if seen.contains(&value) {
        return "[Circular]".to_owned();
    }
    let array_like = is_array_like(value);
    if depth == 0 {
        return match array_like {
            true => "[Array]".to_owned(),
            false => "[Object]".to_owned(),
        };
    }
    seen.push(value);
    let keys = own_key_strings(value);
    let parts: Vec<String> = keys
        .iter()
        .map(|key| {
            let key_value = string(key);
            // `own_keys` is called by `own_key_strings`, above; this
            // `get_indexed` is its own, separate borrow — never nested
            // inside the other's.
            let item = rts_core_rwk::entry::get_indexed(value, key_value);
            let rendered = format_value(item, depth - 1, seen);
            match array_like {
                true => rendered,
                false => format!("{key}: {rendered}"),
            }
        })
        .collect();
    seen.pop();
    let (open, close) = match array_like {
        true => ('[', ']'),
        false => ('{', '}'),
    };
    match parts.is_empty() {
        true => format!("{open}{close}"),
        false => format!("{open} {} {close}", parts.join(", ")),
    }
}

/// A structural, depth-bounded `Object.is`-adjacent walk — see the module doc
/// for why an object needs a cap and this crate cannot use `strict_equals`
/// for the leaves that need real `ToString`-free equality.
fn deep_equal(a: u64, b: u64, depth: u32) -> bool {
    if a == b {
        return true;
    }
    let null = rts_core_rwk::entry::null_value();
    // `a == b` above already caught two nulls; reaching here with either one
    // `null` means the other is not, and `typeof null` reporting `"object"`
    // would otherwise let it fall into the structural branch and compare
    // equal to `{}` by both having no keys.
    if a == null || b == null {
        return false;
    }
    let ta = kind_of(a);
    let tb = kind_of(b);
    if ta != tb {
        return false;
    }
    match ta.as_str() {
        "number" | "string" | "boolean" | "bigint" | "symbol" => {
            rts_core_rwk::entry::described(a) == rts_core_rwk::entry::described(b)
        }
        "undefined" => true,
        // Two functions are equal only by identity, which `a == b` above
        // already decided.
        "function" => false,
        _ => {
            if depth == 0 {
                return a == b;
            }
            let mut keys_a = own_key_strings(a);
            let mut keys_b = own_key_strings(b);
            keys_a.sort();
            keys_b.sort();
            if keys_a != keys_b {
                return false;
            }
            keys_a.iter().all(|key| {
                let va = rts_core_rwk::entry::get_indexed(a, string(key));
                let vb = rts_core_rwk::entry::get_indexed(b, string(key));
                deep_equal(va, vb, depth - 1)
            })
        }
    }
}

/// Whether a value's own enumerable keys are exactly `"0"`, `"1"`, … in
/// order — see the module doc for why this is structural rather than a real
/// `Array.isArray`.
fn is_array_like(value: u64) -> bool {
    let keys = own_key_strings(value);
    !keys.is_empty() && keys.iter().enumerate().all(|(index, key)| *key == index.to_string())
}

/// `typeof value`, as Rust text.
fn kind_of(value: u64) -> String {
    let kind = rts_core_rwk::entry::type_of(value);
    rts_core_rwk::entry::described(kind).unwrap_or_default()
}

/// A value's own enumerable keys, as Rust text.
///
/// Walks the array `own_keys` answers by length and indexed read — the same
/// two entry points every element access in this file uses, called in
/// sequence rather than nested.
fn own_key_strings(value: u64) -> Vec<String> {
    let keys_array = rts_core_rwk::entry::own_keys(value);
    array_items(keys_array)
        .into_iter()
        .filter_map(|key| rts_core_rwk::entry::described(key))
        .collect()
}

/// Every element of an array value, read by index.
fn array_items(array_value: u64) -> Vec<u64> {
    let length = length_of(array_value);
    (0..length)
        .map(|index| rts_core_rwk::entry::get_indexed(array_value, string(&index.to_string())))
        .collect()
}

/// An array value's `length`, as a Rust count.
fn length_of(array_value: u64) -> usize {
    let value = rts_core_rwk::entry::get_indexed(array_value, string("length"));
    rts_core_rwk::entry::described(value)
        .and_then(|text| text.parse::<f64>().ok())
        .map(|count| count as usize)
        .unwrap_or(0)
}

/// A string argument, or `None` for anything that is not a string —
/// `undefined` included, so a caller does not have to check for it first.
fn string_of(value: u64) -> Option<String> {
    let undefined = rts_core_rwk::entry::undefined_value();
    match value == undefined || kind_of(value) != "string" {
        true => None,
        false => rts_core_rwk::entry::described(value),
    }
}

/// A string value.
fn string(text: &str) -> u64 {
    rts_core_rwk::entry::with_runtime(|context| rts_core_rwk::entry::make_string(context, text))
}

/// A boolean value.
fn bool_value(held: bool) -> u64 {
    rts_core_rwk::entry::boolean_value(held)
}
