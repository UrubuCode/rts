//! `replace` and `replaceAll`, and the replacement template.
//!
//! # Why these are apart from the scan that feeds them
//!
//! `pattern.rs` reached 664 lines against a 500-line ceiling. These two are the
//! cohesive half to leave: everything here is about turning matches into an
//! ANSWER — a template expanded, a callback called, the pieces between matches
//! copied across — where everything left there is about finding the matches.
//! The seam is [`super::pattern::scan`], which both sides already went through.

use super::super::native::Native;
use super::super::regex::methods::units_before;
use super::super::with_current;
use super::pattern::{Found, Sought, scan, staged};
use super::{nothing, text_of};
use crate::text::Str;
use crate::value::Value;

/// The two replacements a string's prototype holds.
pub(super) const NATIVES: &[(&str, Native)] =
    &[("replace", replace), ("replaceAll", replace_all)];

/// `s.replace(pattern, replacement)`.
extern "C" fn replace(_e: u64, this: u64, pattern: u64, with: u64, _a2: u64, _a3: u64) -> u64 {
    // `ToString(RequireObjectCoercible(this))`, before any borrow — see
    // `super::coerce_receiver`.
    let Some(this) = super::coerce_receiver(this) else {
        return super::refused();
    };
    replaced(this, pattern, with, false)
}


/// `s.replaceAll(pattern, replacement)`.
///
/// The language requires a `g` pattern here and throws otherwise. This replaces
/// every occurrence whatever the flags say, which is the stated gap throwing
/// leaves — and it is the answer the name asks for.
extern "C" fn replace_all(_e: u64, this: u64, pattern: u64, with: u64, _a2: u64, _a3: u64) -> u64 {
    // `ToString(RequireObjectCoercible(this))`, before any borrow — see
    // `super::coerce_receiver`.
    let Some(this) = super::coerce_receiver(this) else {
        return super::refused();
    };
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
    // `Symbol.replace` FIRST, and for both spellings: `replaceAll` differs from
    // `replace` in requiring a global pattern, not in which protocol it asks.
    // See [`super::pattern::hooked`] for why asking comes before deciding the
    // argument is a pattern at all.
    if let Some(answered) = super::pattern::hooked(this, pattern, super::super::symbol::REPLACE, Some(with)) {
        return answered;
    }
    let collected = with_current(|context| {
        let (subject, sought) = staged(context, this, pattern)?;
        let all = match &sought {
            Sought::Pattern(cell) => every || context.regexp_at(*cell)?.is_global(),
            Sought::Text(_) => every,
        };
        let found = scan(context, &subject, &sought, all);
        // A global pattern ends a replacement with `lastIndex` back at zero —
        // see `super::super::regex::methods::reset_global`.
        if let Sought::Pattern(cell) = &sought {
            super::super::regex::methods::reset_global(context, *cell);
        }
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
        out.push_str(&subject[at..one.from()]);
        match &template {
            Some(template) => expand(&mut out, template, &subject, one),
            None => out.push_str(&produced(with, &subject, one)),
        }
        at = one.to();
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
                .intern_value(Str::from_str(&subject[one.from()..one.to()]))
                .bits(),
        ];
        for group in one.groups.iter().skip(1) {
            values.push(match group {
                Some(text) => context.intern_value(Str::from_str(text)).bits(),
                None => this,
            });
        }
        values.push(Value::from_f64(units_before(subject, one.from()) as f64).bits());
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
    let matched = &subject[one.from()..one.to()];
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
                out.push_str(&subject[..one.from()]);
            }
            Some('\'') => {
                characters.next();
                out.push_str(&subject[one.to()..]);
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
                // A pattern with NO named groups keeps the token literal; a
                // pattern that HAS them consumes `$<zz>` and produces the
                // empty string for a name it does not declare. The two were
                // one branch, so an unknown name printed `$<zz>` in a program
                // whose pattern named something else — which is the shape a
                // template with a typo in it takes, and the specification
                // makes the two cases different on purpose.
                match closed && !one.names.is_empty() {
                    true => {
                        characters = ahead;
                        let group = one.named(&name).cloned().flatten();
                        out.push_str(group.as_deref().unwrap_or(""));
                    }
                    false => out.push('$'),
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
