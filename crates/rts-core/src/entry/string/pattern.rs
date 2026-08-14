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
    /// The groups that have a name, in the order the pattern declares them.
    ///
    /// A list and not a map: this is what `m.groups` is built from, and the
    /// enumeration order of that object is the order the groups were written.
    /// A `BTreeMap` would have sorted them alphabetically, which is a different
    /// object.
    names: Vec<(String, Option<String>)>,
}

impl Found {
    /// One group by name, `None` when the pattern declares no such group.
    ///
    /// The outer `Option` is "there is no such name"; the inner one is "the
    /// group took part in no alternative", which the language spells
    /// `undefined` and a replacement template spells as nothing at all.
    fn named(&self, name: &str) -> Option<&Option<String>> {
        self.names
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value)
    }
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
            Some(template) => expand(&mut out, template, &subject, one),
            None => out.push_str(&produced(with, &subject, one)),
        }
        at = one.to;
    }
    out.push_str(&subject[at..]);
    with_current(|context| context.intern_value(Str::from_str(&out)).bits())
}

/// What a replacement function answered for one match.
///
/// The specification's list, in its order: `(matched, p1..pn, offset, string)`,
/// and a `groups` object at the end when the pattern has named groups. It used
/// to be `(matched, offset, string)` — the three that fit the four fixed slots
/// — so every captured group was missing AND the offset arrived where the first
/// group belongs, which is a wrong answer rather than a missing one:
/// `"John Doe".replace(/(\w+) (\w+)/, (m, a, b) => b + " " + a)` answered
/// `"John Doe 0"`.
///
/// The arity is not the obstacle it was: `call_with_args` takes a vector, which
/// is how `Function.prototype.apply` has always passed more than four.
fn produced(callee: u64, subject: &str, one: &Found) -> String {
    let (this, arguments) = with_current(|context| {
        let this = nothing(context);
        let mut values = vec![
            context
                .intern_value(Str::from_str(&subject[one.from..one.to]))
                .bits(),
        ];
        for group in one.groups.iter().skip(1) {
            values.push(match group {
                Some(text) => context.intern_value(Str::from_str(text)).bits(),
                None => this,
            });
        }
        values.push(Value::from_f64(units_before(subject, one.from) as f64).bits());
        values.push(context.intern_value(Str::from_str(subject)).bits());
        if !one.names.is_empty()
            && let Some(cell) = super::super::native::plain(context)
        {
            for (name, group) in &one.names {
                let key = context.well_known(name);
                let value = match group {
                    Some(text) => context.intern_value(Str::from_str(text)).bits(),
                    None => this,
                };
                super::super::objects::put(context, cell, key, value);
            }
            values.push(Value::from_slot(cell).bits());
        }
        (this, super::super::array::built_in(context, values))
    });
    // Outside every borrow: the callee is user code whose first act may be to
    // call the runtime.
    let answered = super::super::functions::call_with_args(callee, this, arguments);
    with_current(|context| {
        text_of(context, answered)
            .and_then(|text| text.to_rust())
            .unwrap_or_default()
    })
}

/// `s.split(separator, limit)`.
/// # Why this one keeps `to_rust`, when five string methods stopped
///
/// Because its cost is not the input. Measured, release, 2026-08-12, 5e5 calls:
/// a subject of 8 pieces takes 2.58 us and one of 64 takes 14.1 us — 220 to 320
/// ns PER PIECE and roughly flat, so the work scales with what comes out rather
/// than with what goes in. A narrow path over the subject would touch the
/// smaller half of that.
///
/// What each piece costs is an owned `String`, an interned cell and a slot in
/// the array. Removing that is a different change — the pieces would have to be
/// built as narrow strings directly and `scan` works over `&str` — and it is a
/// rework of the pattern machinery rather than the borrow the other five took.
///
/// Recorded with the number because four of the six searches DID take that
/// borrow and the fifth (`lastIndexOf`) measured slower on it. Neither outcome
/// transfers here, and assuming either would be the mistake both of those
/// measurements exist to prevent.
extern "C" fn split(_e: u64, this: u64, separator: u64, limit: u64, _a2: u64, _a3: u64) -> u64 {
    // Narrow subject, narrow literal separator: the pieces are slices of the
    // subject until each becomes a cell, and none of the allocation below
    // happens. `super::split` answers `None` for every other shape, which is
    // why the rules stay stated once, here.
    if let Some(array) = super::split::split(this, separator, limit) {
        return array;
    }
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
            // Um match VAZIO onde a peça anterior acabou, ou no fim do sujeito,
            // não separa nada — a especificação avança sem cortar. Cortar ali
            // produzia peças vazias a mais nas duas pontas.
            if one.from == one.to && (one.from == at || one.from == subject.len()) {
                continue;
            }
            pieces.push(Some(subject[at..one.from].to_string()));
            // As capturas entram ENTRE as peças, que é o que faz
            // `"a1b22c".split(/(\d+)/)` responder `["a","1","b","22","c"]`. Eram
            // deitadas fora, e com elas metade do que o `split` com grupos serve
            // para fazer.
            pieces.extend(one.groups.iter().skip(1).cloned());
            at = one.to;
        }
        pieces.push(Some(subject[at..].to_string()));
        Some(pieces)
    });

    let Some(mut pieces) = collected else {
        return with_current(|context| nothing(context));
    };
    // O limite é aplicado a TODOS os caminhos, e era aplicado a um. Os dois
    // retornos antecipados lá em cima — separador ausente e separador vazio —
    // saíam antes de ele ser lido, então `"abc".split(undefined, 0)` respondia
    // uma peça em vez de nenhuma.
    if let Some(wanted) = Value(limit).numeric().filter(|wanted| *wanted >= 0.0) {
        pieces.truncate(wanted as usize);
    }
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
fn staged_as_regex(context: &mut super::Context, this: u64, pattern: u64, force_global: bool) -> Option<(String, Sought)> {
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
                    .filter_map(|(at, name)| {
                        Some((name?, groups.get(at).cloned().flatten()))
                    })
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

/// A replacement template, with what the match filled in.
///
/// The seven the language defines: `$$` is a dollar sign, `` $` `` is
/// everything before the match, `$'` everything after, `$&` the match itself,
/// `$1`..`$99` the groups, and `$<name>` a named one. A `$` followed by
/// anything else stands for itself, which is what makes
/// `"a".replace("a", "$100")` produce `"$100"` when there is no first group
/// rather than swallowing the digits.
///
/// It knew four of them, and the other three fell into the literal branch — so
/// `"abc".replace(/b/, "[$`]")` answered `"a[$`]c"`, printing the token instead
/// of the text before the match. The subject and the match bounds are passed in
/// for exactly those two, which is why this takes the whole [`Found`] rather
/// than the matched slice it took before.
fn expand(out: &mut String, template: &str, subject: &str, one: &Found) {
    let matched = &subject[one.from..one.to];
    let groups = &one.groups;
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
            Some('`') => {
                characters.next();
                out.push_str(&subject[..one.from]);
            }
            Some('\'') => {
                characters.next();
                out.push_str(&subject[one.to..]);
            }
            Some('<') => {
                // A named group, and the whole token stays literal when the
                // pattern has none — the specification's own rule, and the one
                // that keeps `"$<x>"` meaning itself for a pattern without
                // names rather than silently disappearing.
                let mut name = String::new();
                let mut closed = false;
                let mut ahead = characters.clone();
                ahead.next();
                for character in ahead.by_ref() {
                    if character == '>' {
                        closed = true;
                        break;
                    }
                    name.push(character);
                }
                match closed.then(|| one.named(&name)).flatten() {
                    Some(group) => {
                        characters = ahead;
                        out.push_str(group.as_deref().unwrap_or(""));
                    }
                    None => out.push('$'),
                }
            }
            Some(digit) if digit.is_ascii_digit() => {
                // Two digits FIRST when that group exists: `$12` is the twelfth
                // group where there are twelve, and `$1` followed by a literal
                // `2` where there are not. One digit only was the reading that
                // made every pattern past nine groups unreachable.
                let mut ahead = characters.clone();
                ahead.next();
                let second = ahead.peek().copied().filter(char::is_ascii_digit);
                let two = second.and_then(|second| {
                    let which = digit.to_digit(10)? as usize * 10 + second.to_digit(10)? as usize;
                    groups.get(which).is_some().then_some(which)
                });
                let which = match two {
                    Some(which) => {
                        ahead.next();
                        characters = ahead;
                        which
                    }
                    None => {
                        characters.next();
                        digit.to_digit(10).expect("an ascii digit") as usize
                    }
                };
                // `$0` is not a group — index zero is the whole match, which
                // `$&` already spells — so it stays literal.
                match groups.get(which).filter(|_| which > 0) {
                    // A group that took part in no alternative contributes
                    // nothing, which is not the same as the text "undefined".
                    Some(group) => out.push_str(group.as_deref().unwrap_or("")),
                    None => {
                        out.push('$');
                        out.push_str(&which.to_string());
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
fn null_of(context: &super::Context) -> u64 {
    rts_cranelift::tags::encode(
        rts_cranelift::tags::TAG_SINGLETON,
        u64::from(context.singletons.null),
    )
}

