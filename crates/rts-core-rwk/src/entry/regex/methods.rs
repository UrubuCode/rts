//! `test` and `exec`, as functions a compiled program can call.
//!
//! # Why these are not entry points
//!
//! An entry point is what the *compiler* emits a call to, and the compiler emits
//! no call for `re.test(s)` — it emits a property read and a call of whatever
//! came back, because that is what the expression means. So these are reached
//! the way any other method is, and their shape is the compiled calling
//! convention rather than an ABI of their own.
//!
//! # Why positions are converted
//!
//! Both engines count bytes and JavaScript counts UTF-16 code units. For text
//! that is entirely ASCII the two agree, which is why getting this wrong
//! survives most tests — and `"é".length` is 1 while its UTF-8 length is 2, so
//! an index handed back unconverted would name the middle of a character.

use super::super::objects::{put, read_property, undefined_of};
use super::super::{Context, with_current};
use crate::text::Str;
use crate::value::Value;

use super::super::native::Native;

/// What a regular expression's prototype holds.
pub(super) const NATIVES: &[(&str, Native)] = &[("test", test), ("exec", exec)];

/// `RegExp(p, f)` and `new RegExp(p, f)`, which are the same thing here.
///
/// # Why one function serves both
///
/// The language says `RegExp` called without `new` behaves as if it had one —
/// it is one of the few constructors that does. And `construct` answers the
/// object a callee *returned* when the callee returned one, so a native that
/// ignores the receiver it was handed and answers its own object satisfies both
/// spellings without either knowing about the other.
///
/// # Why absent flags are the empty string rather than a refusal
///
/// `new RegExp("a")` is ordinary JavaScript and its second argument is
/// `undefined`, which the specification reads as no flags. Requiring a string
/// would refuse the common spelling.
pub(super) extern "C" fn construct(
    _environment: u64,
    _this: u64,
    pattern: u64,
    flags: u64,
    _a2: u64,
    _a3: u64,
) -> u64 {
    with_current(|context| {
        // `new RegExp(existingRegExp)` copies `source`/`flags` from the
        // original rather than `ToString`-ing it: `String(/ab+c/)` is the
        // literal text `"/ab+c/"`, slashes included, and compiling *that*
        // would build a pattern that matches the four characters "ab+c"
        // between literal slashes — not a copy of the original at all. An
        // explicit second argument still overrides the copied flags, which is
        // what the specification says and what `new RegExp(/a/i, "g")` needs.
        if let Some(cell) = Value(pattern).as_slot()
            && let Some(original) = context.regexp_at(cell)
        {
            let source = original.source().to_owned();
            let inherited = original.flags().to_owned();
            let letters = match text_of(context, flags) {
                Some(explicit) => explicit,
                None => inherited,
            };
            return super::make(context, &source, &letters);
        }
        let Some(source) = text_of(context, pattern) else {
            // A pattern that is not a string. `new RegExp(1)` runs `ToString`
            // on it, and `ToString` of an object calls user code an entry point
            // cannot call — so the conversion stops where that becomes true,
            // rather than half-doing it for the cases that would work.
            return undefined_of(context);
        };
        let letters = text_of(context, flags).unwrap_or_default();
        super::make(context, &source, &letters)
    })
}

/// `re.test(s)` — whether there is a match.
///
/// Answers `false` for a receiver that is not a regular expression, where the
/// language throws a `TypeError`. The same stated gap the rest of this crate
/// has while a throw cannot find a handler in a caller.
extern "C" fn test(_environment: u64, this: u64, subject: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    with_current(|context| {
        let found = search(context, this, subject);
        Value::from_bool(found.is_some()).bits()
    })
}

/// `re.exec(s)` — the match, its groups, and where it was; or `null`.
///
/// # Why the array is built in two steps
///
/// `array_new` is an entry point and takes the context itself. Calling it from
/// inside a borrow of the context re-enters the `RefCell` and panics, which is
/// the deadlock this repository has already paid for once — so what the match
/// says is collected first, the borrow is released, and the array is filled in a
/// second one. The same two-step shape `own_keys` has, for the same reason.
extern "C" fn exec(_environment: u64, this: u64, subject: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let found = with_current(|context| {
        let spans = search(context, this, subject)?;
        let text = text_of(context, subject)?;
        let parts: Vec<Option<String>> = spans
            .iter()
            .map(|span| span.map(|(from, to)| text[from..to].to_string()))
            .collect();
        let at = units_before(&text, spans[0]?.0);
        Some((parts, at, text))
    });

    let Some((parts, at, text)) = found else {
        return with_current(|context| null_of(context));
    };

    let array = super::super::array::array_new(parts.len() as i64);
    with_current(|context| {
        let absent = undefined_of(context);
        let values: Vec<u64> = parts
            .into_iter()
            .map(|part| match part {
                Some(text) => context.intern_value(Str::from_str(&text)).bits(),
                // A group that took part in no alternative. `undefined`, which
                // is not the empty string and does not compare like one.
                None => absent,
            })
            .collect();
        let Some(cell) = Value(array).as_slot() else {
            return array;
        };
        if let Some(elements) = context.elements_at_mut(cell) {
            *elements = values;
        }
        // `index` and `input` are properties of the array the language
        // promises, and a program reading `m.index` is how a match is located
        // at all.
        let index = Value::from_f64(at as f64).bits();
        let key = context.well_known("index");
        put(context, cell, key, index);
        let input = context.intern_value(Str::from_str(&text)).bits();
        let key = context.well_known("input");
        put(context, cell, key, input);
        array
    })
}

/// Runs a search, honouring and advancing `lastIndex`.
///
/// # Why `lastIndex` is read from the object rather than kept beside it
///
/// Because a program may write it. `re.lastIndex = 0` is how a stateful search
/// is reset, and a copy held beside the cell would be the one the search reads
/// while the program wrote the other — two answers to where the next match
/// starts.
fn search(context: &mut Context, this: u64, subject: u64) -> Option<super::compile::Spans> {
    let cell = Value(this).as_slot()?;
    let text = text_of(context, subject)?;

    let stateful = context.regexp_at(cell)?.tracks_last_index();
    let start = if stateful {
        let at = last_index(context, cell);
        // Past the end is not a match, and the language resets rather than
        // searching from an impossible place.
        match bytes_before(&text, at) {
            Some(offset) => offset,
            None => {
                set_last_index(context, cell, 0.0);
                return None;
            }
        }
    } else {
        0
    };

    let spans = context.regexp_at(cell)?.find_at(&text, start);
    if stateful {
        // Advanced to the end of what matched, or reset — which is what makes a
        // `g` pattern walk a string across repeated calls, and what makes the
        // walk terminate.
        let next = match spans.as_ref().and_then(|spans| spans[0]) {
            Some((_, to)) => units_before(&text, to) as f64,
            None => 0.0,
        };
        set_last_index(context, cell, next);
    }
    spans
}

/// `re.lastIndex`, as a count of code units.
///
/// A value the program may have written to anything at all, so a negative or
/// fractional one reads as zero rather than as a position — the specification's
/// `ToLength`, which clamps rather than refusing.
fn last_index(context: &mut Context, cell: u32) -> usize {
    let key = context.well_known("lastIndex");
    match read_property(context, cell, key).and_then(|value| value.numeric()) {
        Some(number) if number > 0.0 => number as usize,
        _ => 0,
    }
}

/// Writes it back.
fn set_last_index(context: &mut Context, cell: u32, at: f64) {
    let key = context.well_known("lastIndex");
    let value = Value::from_f64(at).bits();
    put(context, cell, key, value);
}

/// The text a value has, when it is genuinely a string.
fn text_of(context: &Context, value: u64) -> Option<String> {
    context.text_at(Value(value).as_slot()?)?.to_rust()
}

/// How many UTF-16 code units precede a byte offset.
pub(in crate::entry) fn units_before(text: &str, offset: usize) -> usize {
    text[..offset].encode_utf16().count()
}

/// The byte offset a count of code units names, if it names one.
///
/// `None` for a count past the end — which is a `lastIndex` a program wrote
/// beyond the subject, and the caller resets rather than searching from it. Also
/// `None` for a position inside a surrogate pair: that is not a place a match
/// can begin, and answering the nearest byte would begin one inside a character.
pub(in crate::entry) fn bytes_before(text: &str, units: usize) -> Option<usize> {
    if units == 0 {
        return Some(0);
    }
    let mut counted = 0;
    for (offset, character) in text.char_indices() {
        if counted == units {
            return Some(offset);
        }
        counted += character.len_utf16();
    }
    (counted == units).then_some(text.len())
}

/// The encoded `null`, from the numbering the language declared.
///
/// `exec` answers it and not `undefined`, and the difference is load-bearing:
/// `while ((m = re.exec(s)) !== null)` is how a global pattern is walked, and a
/// loop written that way against `undefined` never ends.
fn null_of(context: &Context) -> u64 {
    rts_cranelift::tags::encode(
        rts_cranelift::tags::TAG_SINGLETON,
        u64::from(context.singletons.null),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions_are_counted_in_code_units_not_bytes() {
        // `"é".length` is 1 and its UTF-8 length is 2, so a position handed back
        // unconverted would name the middle of a character.
        let text = "éa";
        assert_eq!(units_before(text, 2), 1, "one unit precedes the `a`");
        assert_eq!(bytes_before(text, 1), Some(2));
        assert_eq!(bytes_before(text, 2), Some(3));
        assert_eq!(bytes_before(text, 3), None, "past the end");
    }

    #[test]
    fn a_position_inside_a_surrogate_pair_is_not_a_place_a_match_begins() {
        // An astral character is two code units and one character. Unit 1 is
        // inside it, and the nearest byte would begin a match mid-character.
        let text = "😀a";
        assert_eq!(bytes_before(text, 0), Some(0));
        assert_eq!(bytes_before(text, 1), None);
        assert_eq!(bytes_before(text, 2), Some(4));
    }
}
