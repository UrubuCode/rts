//! The string methods that take a pattern.
//!
//! # Why these live here and not on the regular expression
//!
//! Because the receiver is the string. `"a-b".split("-")` and `"a-b".split(/-/)`
//! are one method with two kinds of separator, and the specification writes them
//! as one — a separate implementation per kind is where `split` on a string
//! would come to disagree with `split` on a pattern about the empty separator.
//!
//! # Why the search is its own scan rather than repeated `exec`
//!
//! `exec` drives `lastIndex`, which is state on the object. A string method that
//! reached for it would make `"aa".replace(/a/g, "b")` depend on where an
//! unrelated earlier `test` left the pattern — and the specification does say
//! these methods set `lastIndex`, which is a divergence stated here: they scan
//! from the start and leave it alone.
//!
//! # Why a match that consumed nothing advances anyway
//!
//! `/x*/` matches the empty string at every position. A loop that resumed at the
//! end of the previous match would resume where it started and never finish.
//! Advancing one character is what the specification does and what keeps this
//! terminating.

use super::super::native::Native;
use super::super::regex::methods::units_before;
use super::super::with_current;
use super::{absent, nothing, text_of};
use crate::text::Str;
use crate::value::Value;

/// What a string's prototype holds that takes a pattern.
pub(super) const NATIVES: &[(&str, Native)] = &[
    ("search", search),
    ("match", match_),
    ("matchAll", match_all),
    ("replace", replace),
    ("replaceAll", replace_all),
    ("split", split),
];

/// Where a match begins and ends, in bytes, with its groups.
struct Found {
    from: usize,
    to: usize,
    groups: Vec<Option<String>>,
}

/// What a method was given to look for.
enum Sought {
    /// A compiled pattern, by the cell that holds it.
    Pattern(u32),
    /// Plain text, which matches itself.
    Text(String),
}

/// `s.search(re)` — where the first match is, or -1.
extern "C" fn search(_e: u64, this: u64, pattern: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    with_current(|context| {
        let Some((subject, sought)) = staged(context, this, pattern) else {
            return nothing(context);
        };
        let found = scan(context, &subject, &sought, false);
        let at = found
            .first()
            .map_or(-1.0, |first| units_before(&subject, first.from) as f64);
        Value::from_f64(at).bits()
    })
}

/// `s.match(re)` — the match and its groups, or every match when `g` is set.
///
/// Two different answers from one method, which is the language rather than a
/// choice: without `g` it is `exec`, with `g` it is the list of matched text and
/// no groups at all.
extern "C" fn match_(_e: u64, this: u64, pattern: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let collected = with_current(|context| {
        let (subject, sought) = staged(context, this, pattern)?;
        let global = match sought {
            Sought::Pattern(cell) => context.regexp_at(cell)?.is_global(),
            Sought::Text(_) => false,
        };
        let found = scan(context, &subject, &sought, global);
        let first = found.first()?;
        let at = units_before(&subject, first.from);
        let parts: Vec<Option<String>> = if global {
            found
                .iter()
                .map(|one| Some(subject[one.from..one.to].to_string()))
                .collect()
        } else {
            first.groups.clone()
        };
        Some((parts, at, subject, global))
    });

    let Some((parts, at, subject, global)) = collected else {
        return with_current(|context| null_of(context));
    };
    let array = super::super::array::array_new(parts.len() as i64);
    with_current(|context| {
        fill(context, array, parts);
        // `index` and `input` belong to a single match. A global one is a list
        // of strings and has neither, which is the difference programs actually
        // trip over.
        if !global
            && let Some(cell) = Value(array).as_slot()
        {
            let index = Value::from_f64(at as f64).bits();
            let key = context.well_known("index");
            super::super::objects::put(context, cell, key, index);
            let input = context.intern_value(Str::from_str(&subject)).bits();
            let key = context.well_known("input");
            super::super::objects::put(context, cell, key, input);
        }
        array
    })
}

/// `s.matchAll(re)` — every match, each with its groups.
///
/// What `match` with `g` throws away: that form answers a flat list of matched
/// text and no groups at all, which is why this exists beside it rather than as
/// a flag on it.
///
/// The language answers an iterator of match arrays and this answers an array of
/// them, for the reason [`super::super::iterate`] records — and it is the shape
/// `for-of` and `...` both accept. The divergence, named: `.next()` is not a
/// function on the result, and the matches are all found before the first is
/// looked at.
extern "C" fn match_all(_e: u64, this: u64, pattern: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let collected = with_current(|context| {
        let (subject, sought) = staged(context, this, pattern)?;
        let found = scan(context, &subject, &sought, true);
        let each: Vec<(Vec<Option<String>>, usize)> = found
            .iter()
            .map(|one| (one.groups.clone(), units_before(&subject, one.from)))
            .collect();
        Some((each, subject))
    });

    let Some((each, subject)) = collected else {
        return super::super::array_proto::built(Vec::new());
    };
    let matches: Vec<u64> = each
        .into_iter()
        .map(|(groups, at)| {
            let array = super::super::array::array_new(groups.len() as i64);
            with_current(|context| {
                fill(context, array, groups);
                // `index` and `input`, which a single match carries and the
                // global form of `match` drops — the whole reason a program
                // reaches for this method.
                if let Some(cell) = Value(array).as_slot() {
                    let index = Value::from_f64(at as f64).bits();
                    let key = context.well_known("index");
                    super::super::objects::put(context, cell, key, index);
                    let input = context.intern_value(Str::from_str(&subject)).bits();
                    let key = context.well_known("input");
                    super::super::objects::put(context, cell, key, input);
                }
            });
            array
        })
        .collect();
    super::super::array_proto::built(matches)
}

/// `s.replace(pattern, replacement)`.
extern "C" fn replace(_e: u64, this: u64, pattern: u64, with: u64, _a2: u64, _a3: u64) -> u64 {
    replaced(this, pattern, with, false)
}

/// `s.replaceAll(pattern, replacement)`.
///
/// The language requires a `g` pattern here and throws otherwise. This replaces
/// every occurrence whatever the flags say, which is the stated gap throwing
/// leaves — and it is the answer the name asks for.
extern "C" fn replace_all(_e: u64, this: u64, pattern: u64, with: u64, _a2: u64, _a3: u64) -> u64 {
    replaced(this, pattern, with, true)
}

/// Both replacements, which differ only in how many matches they consume.
///
/// # Why this is three stages
///
/// The middle one may call user code — `s.replace(/a/g, m => m + "!")` is
/// ordinary JavaScript — and calling from inside a borrow of the context
/// re-enters the `RefCell`. So the matches are collected, the borrow is
/// released, each replacement is computed, and the result is assembled in a
/// fresh borrow.
fn replaced(this: u64, pattern: u64, with: u64, every: bool) -> u64 {
    let collected = with_current(|context| {
        let (subject, sought) = staged(context, this, pattern)?;
        let all = match &sought {
            Sought::Pattern(cell) => every || context.regexp_at(*cell)?.is_global(),
            Sought::Text(_) => every,
        };
        let found = scan(context, &subject, &sought, all);
        let callee = Value(with)
            .as_slot()
            .filter(|cell| context.callable_at(*cell).is_some());
        let template = match callee {
            Some(_) => None,
            None => Some(text_of(context, with)?.to_rust()?),
        };
        Some((subject, found, template))
    });

    let Some((subject, found, template)) = collected else {
        return with_current(|context| nothing(context));
    };

    let mut out = String::with_capacity(subject.len());
    let mut at = 0;
    for one in &found {
        out.push_str(&subject[at..one.from]);
        match &template {
            Some(template) => expand(&mut out, template, &subject[one.from..one.to], &one.groups),
            None => out.push_str(&produced(with, &subject, one)),
        }
        at = one.to;
    }
    out.push_str(&subject[at..]);
    with_current(|context| context.intern_value(Str::from_str(&out)).bits())
}

/// What a replacement function answered for one match.
///
/// Called with the matched text, where it was, and the whole subject — the three
/// the specification passes before the groups, which the fixed arity has no room
/// for beyond that. A call with more is what `ARGUMENT_SLOTS` refuses.
fn produced(callee: u64, subject: &str, one: &Found) -> String {
    let (this, matched, at, input) = with_current(|context| {
        let this = nothing(context);
        let matched = context
            .intern_value(Str::from_str(&subject[one.from..one.to]))
            .bits();
        let at = Value::from_f64(units_before(subject, one.from) as f64).bits();
        let input = context.intern_value(Str::from_str(subject)).bits();
        (this, matched, at, input)
    });
    // Outside every borrow: the callee is user code whose first act may be to
    // call the runtime.
    let answered = super::super::functions::call(callee, this, matched, at, input, this);
    with_current(|context| {
        text_of(context, answered)
            .and_then(|text| text.to_rust())
            .unwrap_or_default()
    })
}

/// `s.split(separator, limit)`.
extern "C" fn split(_e: u64, this: u64, separator: u64, limit: u64, _a2: u64, _a3: u64) -> u64 {
    let collected = with_current(|context| {
        let subject = text_of(context, this)?.to_rust()?;
        // No separator at all is the whole string as one piece — not every
        // character, which is what an empty separator means. The two are a
        // sentence apart in the specification and a common confusion.
        if absent(context, separator) {
            return Some(vec![Some(subject)]);
        }
        let sought = pattern_of(context, separator)?;
        // An empty separator splits between every code unit. Falling through to
        // the scan would find an empty match at every position and produce the
        // same thing by a longer route — but it would also produce a trailing
        // empty piece, which the language does not.
        if let Sought::Text(text) = &sought
            && text.is_empty()
        {
            let units: Vec<Option<String>> = Str::from_str(&subject)
                .units()
                .map(|unit| Some(String::from_utf16_lossy(&[unit])))
                .collect();
            return Some(units);
        }
        let found = scan(context, &subject, &sought, true);
        let mut pieces = Vec::new();
        let mut at = 0;
        for one in &found {
            pieces.push(Some(subject[at..one.from].to_string()));
            at = one.to;
        }
        pieces.push(Some(subject[at..].to_string()));
        let wanted = Value(limit).numeric();
        if let Some(wanted) = wanted.filter(|wanted| *wanted >= 0.0) {
            pieces.truncate(wanted as usize);
        }
        Some(pieces)
    });

    let Some(pieces) = collected else {
        return with_current(|context| nothing(context));
    };
    let array = super::super::array::array_new(pieces.len() as i64);
    with_current(|context| {
        fill(context, array, pieces);
        array
    })
}

/// The receiver's text and what to look for in it.
fn staged(context: &super::Context, this: u64, pattern: u64) -> Option<(String, Sought)> {
    let subject = text_of(context, this)?.to_rust()?;
    Some((subject, pattern_of(context, pattern)?))
}

/// A pattern, however it was spelled.
fn pattern_of(context: &super::Context, pattern: u64) -> Option<Sought> {
    if let Some(cell) = Value(pattern).as_slot()
        && context.regexp_at(cell).is_some()
    {
        return Some(Sought::Pattern(cell));
    }
    Some(Sought::Text(text_of(context, pattern)?.to_rust()?))
}

/// Every match, or only the first.
fn scan(context: &super::Context, subject: &str, sought: &Sought, all: bool) -> Vec<Found> {
    let mut found = Vec::new();
    let mut at = 0;
    while at <= subject.len() {
        let one = match sought {
            Sought::Pattern(cell) => {
                let spans = match context.regexp_at(*cell).and_then(|rx| rx.find_at(subject, at)) {
                    Some(spans) => spans,
                    None => break,
                };
                let Some((from, to)) = spans[0] else { break };
                Found {
                    from,
                    to,
                    groups: spans
                        .iter()
                        .map(|span| span.map(|(from, to)| subject[from..to].to_string()))
                        .collect(),
                }
            }
            Sought::Text(text) => match subject[at..].find(text.as_str()) {
                Some(offset) => Found {
                    from: at + offset,
                    to: at + offset + text.len(),
                    groups: vec![Some(text.clone())],
                },
                None => break,
            },
        };
        let empty = one.to == one.from;
        let resume = one.to;
        found.push(one);
        if !all {
            break;
        }
        // A match that consumed nothing would be found again at the same place.
        // One character forward is what the specification does, and it is what
        // makes `"ab".replace(/x*/g, "-")` terminate.
        at = if empty {
            match subject[resume..].chars().next() {
                Some(character) => resume + character.len_utf8(),
                None => break,
            }
        } else {
            resume
        };
    }
    found
}

/// A replacement template, with what the match filled in.
///
/// `$&` is the whole match, `$1`..`$9` are the groups, `$$` is a dollar sign.
/// A `$` followed by anything else stands for itself, which is what the language
/// says and what makes `"a".replace("a", "$100")` produce `"$100"` when there is
/// no first group rather than swallowing the digits.
fn expand(out: &mut String, template: &str, matched: &str, groups: &[Option<String>]) {
    let mut characters = template.chars().peekable();
    while let Some(character) = characters.next() {
        if character != '$' {
            out.push(character);
            continue;
        }
        match characters.peek().copied() {
            Some('$') => {
                characters.next();
                out.push('$');
            }
            Some('&') => {
                characters.next();
                out.push_str(matched);
            }
            Some(digit) if digit.is_ascii_digit() => {
                characters.next();
                let which = digit.to_digit(10).expect("an ascii digit") as usize;
                match groups.get(which) {
                    // A group that took part in no alternative contributes
                    // nothing, which is not the same as the text "undefined".
                    Some(group) => out.push_str(group.as_deref().unwrap_or("")),
                    None => {
                        out.push('$');
                        out.push(digit);
                    }
                }
            }
            _ => out.push('$'),
        }
    }
}

/// Writes strings into an array that has already been made.
fn fill(context: &mut super::Context, array: u64, parts: Vec<Option<String>>) {
    let missing = super::nothing(context);
    let values: Vec<u64> = parts
        .into_iter()
        .map(|part| match part {
            Some(text) => context.intern_value(Str::from_str(&text)).bits(),
            None => missing,
        })
        .collect();
    if let Some(cell) = Value(array).as_slot()
        && let Some(elements) = context.elements_at_mut(cell)
    {
        *elements = values;
    }
}

/// The encoded `null`, which `match` answers when nothing matched.
fn null_of(context: &super::Context) -> u64 {
    rts_cranelift::tags::encode(
        rts_cranelift::tags::TAG_SINGLETON,
        u64::from(context.singletons.null),
    )
}

