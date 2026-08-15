//! The string methods that take a pattern, and the scan all of them share.
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
//!
//! # Where `replace` and `split` went
//!
//! Into [`super::replace`] and [`super::split`]. This file reached 664 lines
//! against a 500-line ceiling, and the two that left are the ones whose bodies
//! are about assembling an ANSWER rather than about finding matches — which is
//! what everything remaining here is.

use super::super::native::Native;
use super::super::regex::methods::units_before;
use super::super::with_current;
use super::{nothing, text_of};
use crate::text::Str;
use crate::value::Value;

/// What a string's prototype holds that takes a pattern and reads the matches.
pub(super) const NATIVES: &[(&str, Native)] =
    &[("search", search), ("match", match_), ("matchAll", match_all)];

/// Where a match begins and ends, in bytes, with its groups.
pub(super) struct Found {
    pub(super) from: usize,
    pub(super) to: usize,
    pub(super) groups: Vec<Option<String>>,
    /// The groups that have a name, in the order the pattern declares them.
    ///
    /// A list and not a map: this is what `m.groups` is built from, and the
    /// enumeration order of that object is the order the groups were written.
    /// A `BTreeMap` would have sorted them alphabetically, which is a different
    /// object.
    pub(super) names: Vec<(String, Option<String>)>,
}

impl Found {
    /// One group by name, `None` when the pattern declares no such group.
    ///
    /// The outer `Option` is "there is no such name"; the inner one is "the
    /// group took part in no alternative", which the language spells
    /// `undefined` and a replacement template spells as nothing at all.
    pub(super) fn named(&self, name: &str) -> Option<&Option<String>> {
        self.names
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value)
    }
}

/// What a method was given to look for.
pub(super) enum Sought {
    /// A compiled pattern, by the cell that holds it.
    Pattern(u32),
    /// Plain text, which matches itself.
    Text(String),
}

/// `s.search(re)` — where the first match is, or -1.
extern "C" fn search(_e: u64, this: u64, pattern: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    with_current(|context| {
        let Some((subject, sought)) = staged_as_regex(context, this, pattern, false) else {
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
        let (subject, sought) = staged_as_regex(context, this, pattern, false)?;
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
        let named = match global {
            true => Vec::new(),
            false => first.names.clone(),
        };
        Some((parts, at, subject, global, named))
    });

    let Some((parts, at, subject, global, named)) = collected else {
        return with_current(null_of);
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
            // O mesmo objeto que o `exec` monta, pelo mesmo helper: as duas
            // formas de correr um padrao contra uma string tem de responder o
            // mesmo `groups`, e duas construcoes seriam duas respostas.
            let groups = super::super::regex::groups_object(context, &named);
            let key = context.well_known("groups");
            super::super::objects::put(context, cell, key, groups);
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
        let (subject, sought) = staged_as_regex(context, this, pattern, true)?;
        let found = scan(context, &subject, &sought, true);
        let each: Vec<(Vec<Option<String>>, usize, Vec<(String, Option<String>)>)> = found
            .iter()
            .map(|one| {
                (
                    one.groups.clone(),
                    units_before(&subject, one.from),
                    one.names.clone(),
                )
            })
            .collect();
        Some((each, subject))
    });

    let Some((each, subject)) = collected else {
        return super::super::array_proto::built(Vec::new());
    };
    let matches: Vec<u64> = each
        .into_iter()
        .map(|(groups, at, named)| {
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
                    // O mesmo `groups` que o `exec` monta: quem percorre
                    // `matchAll` le os grupos nomeados de cada volta, e sem isto
                    // lia `undefined` em todas.
                    let groups = super::super::regex::groups_object(context, &named);
                    let key = context.well_known("groups");
                    super::super::objects::put(context, cell, key, groups);
                }
            });
            array
        })
        .collect();
    super::super::array_proto::built(matches)
}

/// The receiver's text and what to look for in it.
pub(super) fn staged(
    context: &super::Context,
    this: u64,
    pattern: u64,
) -> Option<(String, Sought)> {
    let subject = text_of(context, this)?.to_rust()?;
    Some((subject, pattern_of(context, pattern)?))
}

/// The receiver's text and what to look for in it, for `search`/`match`/
/// `matchAll` — where a plain string names a *pattern* rather than literal
/// text.
///
/// The specification treats these three methods' string argument as
/// `RegExp(searchValue)` (`matchAll` as `RegExp(searchValue, "g")`): a real
/// `RegExp` is used as-is, and any other value — including an ordinary string
/// — is compiled as a regular expression source. `"Hello".search("[Hh]ello")`
/// matches at 0 because `"[Hh]ello"` is a bracket class, not four literal
/// characters `[`, `H`, `h`, `]`. `replace`/`replaceAll`/`split` do not go
/// through this: the specification has those treat a plain string literally.
fn staged_as_regex(
    context: &mut super::Context,
    this: u64,
    pattern: u64,
    force_global: bool,
) -> Option<(String, Sought)> {
    let subject = text_of(context, this)?.to_rust()?;
    let sought = match pattern_of(context, pattern)? {
        Sought::Pattern(cell) => Sought::Pattern(cell),
        Sought::Text(source) => {
            let letters = if force_global { "g" } else { "" };
            let object = super::super::regex::make(context, &source, letters);
            let cell = Value(object).as_slot()?;
            Sought::Pattern(cell)
        }
    };
    Some((subject, sought))
}

/// A pattern, however it was spelled.
pub(super) fn pattern_of(context: &super::Context, pattern: u64) -> Option<Sought> {
    if let Some(cell) = Value(pattern).as_slot()
        && context.regexp_at(cell).is_some()
    {
        return Some(Sought::Pattern(cell));
    }
    Some(Sought::Text(text_of(context, pattern)?.to_rust()?))
}

/// Every match, or only the first.
pub(super) fn scan(
    context: &super::Context,
    subject: &str,
    sought: &Sought,
    all: bool,
) -> Vec<Found> {
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
                let groups: Vec<Option<String>> = spans
                    .iter()
                    .map(|span| span.map(|(from, to)| subject[from..to].to_string()))
                    .collect();
                // Os nomes vêm do motor, que sempre os teve — `Spans` é indexado
                // por POSIÇÃO e não carrega nenhum, então um grupo nomeado
                // chegava aqui anónimo.
                let names = context
                    .regexp_at(*cell)
                    .map(|rx| rx.names())
                    .unwrap_or_default()
                    .into_iter()
                    .enumerate()
                    .filter_map(|(at, name)| Some((name?, groups.get(at).cloned().flatten())))
                    .collect();
                Found {
                    from,
                    to,
                    groups,
                    names,
                }
            }
            // Looked for with memmem, the same two-way search with an SIMD
            // prefilter that Buffer.indexOf here already uses, against the
            // byte-at-a-time window compare str::find falls back to for a
            // multi-byte needle.
            //
            // The group is the SLICE of the subject and not a copy of the
            // needle. It was text.clone(): one heap allocation per match, on a
            // path whose heaviest caller — split, which produces one match per
            // piece — never reads the groups at all.
            Sought::Text(text) => {
                match memchr::memmem::find(&subject.as_bytes()[at..], text.as_bytes()) {
                    Some(offset) => Found {
                        from: at + offset,
                        to: at + offset + text.len(),
                        groups: vec![Some(
                            subject[at + offset..at + offset + text.len()].to_string(),
                        )],
                        names: Vec::new(),
                    },
                    None => break,
                }
            }
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

/// Writes strings into an array that has already been made.
pub(super) fn fill(context: &mut super::Context, array: u64, parts: Vec<Option<String>>) {
    let missing = super::nothing(context);
    // Written STRAIGHT INTO the array, one piece at a time: interning
    // ALLOCATES and an allocation collects, and a piece that has landed in the
    // array is reachable through it, where one in a second `Vec` on the Rust
    // heap was not. That second vector was also a second allocation for one
    // list, against the one `array_new` had already sized.
    if let Some(cell) = Value(array).as_slot() {
        for (at, part) in parts.into_iter().enumerate() {
            let value = match part {
                Some(text) => context.intern_value(Str::from_str(&text)).bits(),
                None => missing,
            };
            if let Some(elements) = context.elements_at_mut(cell) {
                elements[at] = value;
            }
        }
    }
}

/// The encoded `null`, which `match` answers when nothing matched.
fn null_of(context: &mut super::Context) -> u64 {
    rts_cranelift::tags::encode(
        rts_cranelift::tags::TAG_SINGLETON,
        u64::from(context.singletons.null),
    )
}
