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
    // A LONE argument is never a format string. Node decides this by arity
    // (`formatWithOptionsInternal` answers `first` when `args.length === 1`),
    // and the half a walk-and-substitute implementation has no reason to write
    // is `%%`: with nothing to substitute, an unmatched `%s` already passes
    // through below, but `%%` was still collapsed to one `%`. So
    // `console.log("50%%")` printed `50%` here and `50%%` everywhere else.
    if rest.is_empty() {
        return joined(values);
    }
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
        's' => string_text(value)?,
        'j' => inspect::json_stringify(value)?,
        'o' | 'O' => inspect::top_level(value)?,
        _ => number_text(verb, value),
    })
}

/// `%s` — the text, and an object only sometimes.
///
/// # Why an object is not always inspected
///
/// Because Node's `%s` asks WHO WROTE the `toString`: an object carrying its
/// own is printed by calling it, and one whose `toString` is a built-in is
/// inspected. That is what makes `console.log("%s", {toString: () => "TS"})`
/// print `TS` while `console.log("%s", [1, 2])` prints `[ 1, 2 ]` — the array's
/// `toString` would answer `1,2`, and no runtime prints that here.
///
/// The question is asked with [`entry::is_user_function`], which reads the same
/// membership `Function.prototype.toString` reads to choose between
/// `[bytecode]` and `[native code]`. Matching on that rendered text instead
/// would be reading something a program can rewrite.
///
/// The conversion is `string_for_host`, which runs the `toString` — that is the
/// point — so a `toString` that threw leaves the throw in flight and this
/// answers nothing rather than spending it.
fn string_text(value: u64) -> Result<String, Poisoned> {
    if let Some(text) = entry::text_of(value) {
        return Ok(text);
    }
    if !entry::with_runtime(|context| entry::is_object(context, value)) {
        return Ok(inspect::top_level(value)?);
    }
    let method = entry::with_runtime(|context| entry::get_member(context, value, "toString"));
    if !entry::is_user_function(method) {
        return Ok(inspect::top_level(value)?);
    }
    match entry::string_for_host(value) {
        Ok(Some(text)) => Ok(text),
        // A symbol cannot be here (it is not an object) and a throw is the
        // caller's to propagate, so both remaining cases mean "no text".
        _ => Ok(inspect::top_level(value)?),
    }
}

/// `%d`, `%i` and `%f` — three CONVERSIONS, and not one reading with a rounding
/// on top.
///
/// Node's are `Number(arg)`, `parseInt(arg)` and `parseFloat(arg)` respectively,
/// and the differences are the ones a program meets first: `%d` of `"0x10"` is
/// 16 and of `""` is 0, `%d` of `4.7` is `4.7` where `%i` is `4`, and `%d` of an
/// object runs its `valueOf`. This read the value instead — answering `NaN`
/// unless it already WAS a number or parsed as a whole decimal literal — and
/// truncated `%d` as well, so five of those six printed the wrong thing.
///
/// A bigint takes none of the three and prints its own digits; a SYMBOL prints
/// `NaN` rather than raising, which is why it is answered before a conversion
/// that would.
fn number_text(verb: char, value: u64) -> String {
    match entry::text_of(entry::type_of(value)).as_deref() {
        Some("bigint") => return format!("{}n", entry::described(value).unwrap_or_default()),
        Some("symbol") => return "NaN".to_owned(),
        _ => {}
    }
    let number = match verb {
        'd' => entry::number_for_host(value),
        'i' => entry::parse_int_for_host(value, 0),
        'f' => entry::parse_float_for_host(value),
        _ => unreachable!("filtered by the caller's verb check"),
    };
    entry::text_of(entry::number_to_string(number)).unwrap_or_default()
}
