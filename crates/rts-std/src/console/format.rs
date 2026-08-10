//! `%s %d %i %f %j %o %O %%` — applied only when the first argument is a
//! string and it contains one, exactly as `util.format` (what `console.log`
//! calls through to in Node) does. Checked against `node --experimental-strip-types`:
//! `console.log("%s|%d", "str", 42)` prints `str|42`, and `console.log("%s %s",
//! "only")` prints `%s only` — the leftover verb passes through literally, and
//! the argument it did not have is *not* consumed by the next one.

use rts_core::entry;

use super::inspect::{self, Poisoned};

/// The text of a call's arguments, joined the way `console.log` prints a line.
pub fn line(values: &[u64]) -> Result<String, Poisoned> {
    let Some((first, rest)) = values.split_first() else {
        return Ok(String::new());
    };
    if !is_format_string(*first) {
        return joined(values);
    }
    let template = entry::text_of(*first).unwrap_or_default();
    if !template.contains('%') {
        return joined(values);
    }
    let (formatted, consumed) = substitute(&template, rest)?;
    let mut parts = vec![formatted];
    for value in &rest[consumed..] {
        parts.push(inspect::top_level(*value)?);
    }
    Ok(parts.join(" "))
}

/// Every argument, space separated, each through top-level `inspect`.
fn joined(values: &[u64]) -> Result<String, Poisoned> {
    let mut parts = Vec::with_capacity(values.len());
    for value in values {
        parts.push(inspect::top_level(*value)?);
    }
    Ok(parts.join(" "))
}

/// A string, and not some other value `described` also has text for — the
/// convention every implementation of `util.format` follows: `console.log(1,
/// "%s")` does not format, because the FIRST argument decides.
fn is_format_string(value: u64) -> bool {
    entry::number_of(value).is_none() && entry::text_of(value).is_some() && !is_singleton(value)
}

/// Whether `text_of` answered because this is `true`/`false`/`null`/`undefined`
/// rather than an actual string — those convert to text too, and none of them
/// is a format string.
fn is_singleton(value: u64) -> bool {
    let undefined = entry::undefined_value();
    let null = entry::null_value();
    value == undefined || value == null || value == entry::boolean_value(true) || value == entry::boolean_value(false)
}

/// Replaces every recognised `%` verb in `template` with the next unconsumed
/// argument, formatted per verb. Answers the text and how many arguments were
/// used, so the caller appends the rest unconsumed.
fn substitute(template: &str, args: &[u64]) -> Result<(String, usize), Poisoned> {
    let mut out = String::with_capacity(template.len());
    let mut chars = template.chars().peekable();
    let mut at = 0usize;
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        let Some(&verb) = chars.peek() else {
            out.push('%');
            break;
        };
        if verb == '%' {
            chars.next();
            out.push('%');
            continue;
        }
        if !"sdifjoO".contains(verb) {
            out.push('%');
            continue;
        }
        let Some(&value) = args.get(at) else {
            // No argument left for this verb: it passes through literally,
            // exactly as Node's `util.format` leaves an unmatched `%s`.
            out.push('%');
            continue;
        };
        chars.next();
        at += 1;
        out.push_str(&formatted(verb, value)?);
    }
    Ok((out, at))
}

fn formatted(verb: char, value: u64) -> Result<String, Poisoned> {
    Ok(match verb {
        's' => entry::text_of(value).unwrap_or_else(|| inspect::top_level(value).unwrap_or_default()),
        'd' | 'i' => number_text(value, f64::trunc),
        'f' => number_text(value, |n| n),
        'j' => inspect::json_stringify(value)?,
        'o' | 'O' => inspect::top_level(value)?,
        _ => unreachable!("filtered by the caller's verb check"),
    })
}

/// `%d`/`%i`/`%f` — `Number(value)`, then `round` (`f64::trunc` for the two
/// integer verbs, identity for `%f`), printed as JavaScript would print it, or
/// `NaN` for anything that does not convert to one.
///
/// A string is parsed as a whole decimal literal, the shape every value this
/// engine's own `ToNumber` accepts; anything already numeric is read directly.
/// An object is neither — its conversion is `valueOf`/`toString`, which is user
/// code this formatter does not call for a `%`-verb, matching `described`'s own
/// boundary — so it answers `NaN`, which is what Node itself prints for
/// `util.format("%d", {})`.
fn text_of_number(value: f64) -> String {
    entry::text_of(entry::number_to_string(value)).unwrap_or_default()
}

fn number_text(value: u64, round: impl Fn(f64) -> f64) -> String {
    let number = entry::number_of(value).or_else(|| entry::text_of(value)?.trim().parse().ok());
    match number {
        Some(number) if number.is_finite() || number == 0.0 => text_of_number(round(number)),
        Some(number) => text_of_number(number), // ±Infinity
        None => "NaN".to_owned(),
    }
}
