//! A regular expression literal is a program, and may not be one.
//!
//! `/(?<a>x)(?<a>y)/` parses as a JavaScript *expression* perfectly well — the
//! lexer's whole job there is to find the closing slash — and it is not a
//! program. The pattern between the slashes has a grammar of its own, and every
//! rule it breaks is an early error rather than something the match fails at.
//!
//! # Nothing else answers this
//!
//! Searched before writing, under the crate's anti-duplication rule. The only
//! module in the repository that looks inside a pattern is
//! `rts_core::entry::regex::compile`, and what it does is hand the text to the
//! `regex` crate and, on failure, to `fancy_regex` — so the structure is the
//! external engine's opinion rather than the language's. That has two
//! consequences this module exists because of: a pattern those crates decline
//! becomes `undefined` rather than a `SyntaxError`, and a pattern they *accept*
//! with different meaning is never questioned at all. Neither is a place a rule
//! could be added, because the rule is about ECMAScript's grammar and they
//! implement their own.
//!
//! # Conservative on purpose
//!
//! Refusing a valid pattern is much worse than accepting an invalid one: the
//! first breaks a program that runs everywhere, the second only fails to report
//! something. So this refuses what it can name and accepts everything else —
//! and the unicode-only rules are asked only when `u` or `v` is on, because
//! Annex B makes half of them legal without it.

use std::collections::HashSet;

/// Check one literal, and say what is wrong with it.
pub(super) fn check(pattern: &str, flags: &str) -> Option<String> {
    let unicode = match unicode_mode(flags) {
        Ok(unicode) => unicode,
        Err(message) => return Some(message),
    };
    let text: Vec<char> = pattern.chars().collect();
    let mut reader = Reader {
        text: &text,
        at: 0,
        unicode,
        declared: declared_group_names(&text),
        referenced: Vec::new(),
        capture_groups: count_capture_groups(&text),
        error: None,
        open: Vec::new(),
    };
    reader.disjunction();
    if reader.error.is_none() {
        reader.check_references();
    }
    reader.error
}

/// Whether the flags turn unicode mode on, or what is wrong with them.
///
/// Asked first, and not only because the answer is needed: the flags are their
/// own early error, so a pattern with `gg` after it is refused whatever the
/// pattern says. Which also means this cannot answer with a bare `bool` — a
/// failure here is a refusal, not "no unicode".
fn unicode_mode(flags: &str) -> Result<bool, String> {
    let mut seen: Vec<char> = Vec::new();
    for letter in flags.chars() {
        if !matches!(letter, 'd' | 'g' | 'i' | 'm' | 's' | 'u' | 'v' | 'y') {
            return Err(format!("`{letter}` is not a regular expression flag"));
        }
        if seen.contains(&letter) {
            return Err(format!("the flag `{letter}` is given twice"));
        }
        seen.push(letter);
    }
    // `u` and `v` are two answers to one question — which character set the
    // pattern is written against — so a pattern claiming both has none.
    if seen.contains(&'u') && seen.contains(&'v') {
        return Err("the flags `u` and `v` cannot both be given".to_owned());
    }
    Ok(seen.contains(&'u') || seen.contains(&'v'))
}

/// What a term turned out to be, which decides whether it may be repeated.
#[derive(Clone, Copy, PartialEq)]
enum Term {
    /// Something a quantifier may follow.
    Atom,
    /// `(?=` / `(?!` — quantifiable in Annex B, and not in unicode mode.
    Lookahead,
    /// Everything else that matches no characters. Never quantifiable.
    Assertion,
    /// The alternative ended.
    Nothing,
}

struct Reader<'a> {
    text: &'a [char],
    at: usize,
    unicode: bool,
    /// Every group name the pattern declares, found before parsing so that a
    /// backreference may precede its group: `/\k<a>(?<a>x)/` is valid.
    declared: HashSet<String>,
    referenced: Vec<String>,
    capture_groups: usize,
    error: Option<String>,
    /// The group names in scope for the duplicate rule, innermost last.
    ///
    /// A pair per open disjunction: what the current alternative has declared,
    /// and what the disjunction has declared across all of its alternatives.
    /// Two groups of one name are legal when they are in different
    /// alternatives, since only one of them can ever match — which is why this
    /// is a stack of alternatives rather than one set.
    open: Vec<(HashSet<String>, HashSet<String>)>,
}

impl Reader<'_> {
    fn fail(&mut self, message: &str) {
        if self.error.is_none() {
            self.error = Some(message.to_owned());
        }
    }

    fn peek(&self) -> Option<char> {
        self.text.get(self.at).copied()
    }

    fn eat(&mut self, c: char) -> bool {
        if self.peek() == Some(c) {
            self.at += 1;
            return true;
        }
        false
    }

    fn check_references(&mut self) {
        // Without `u`, `\k` is a backreference only when the pattern declares a
        // name somewhere — otherwise Annex B reads it as the letter `k`, and a
        // rule that did not know this would refuse `/\k/`.
        if !self.unicode && self.declared.is_empty() {
            return;
        }
        let unknown = self
            .referenced
            .iter()
            .find(|name| !self.declared.contains(*name))
            .cloned();
        if let Some(name) = unknown {
            self.fail(&format!("`\\k<{name}>` names no group in this pattern"));
        }
    }

    /// `Alternative ('|' Alternative)*`.
    fn disjunction(&mut self) {
        self.open.push((HashSet::new(), HashSet::new()));
        loop {
            self.alternative();
            if self.error.is_some() {
                break;
            }
            if !self.eat('|') {
                break;
            }
            // A new alternative: what the last one declared no longer collides,
            // because at most one of them matches.
            if let Some((current, total)) = self.open.last_mut() {
                total.extend(current.iter().cloned());
                current.clear();
            }
        }
        if let Some((current, mut total)) = self.open.pop() {
            total.extend(current);
            // The whole disjunction is one term of the alternative around it,
            // so everything it declared collides with that alternative's names.
            if let Some((outer, _)) = self.open.last_mut() {
                outer.extend(total);
            }
        }
    }

    fn alternative(&mut self) {
        loop {
            if self.error.is_some() {
                return;
            }
            match self.peek() {
                None | Some('|') | Some(')') => return,
                _ => {}
            }
            let term = self.term();
            if self.error.is_some() {
                return;
            }
            if term == Term::Nothing {
                return;
            }
            self.quantifier(term);
        }
    }

    fn term(&mut self) -> Term {
        let Some(c) = self.peek() else {
            return Term::Nothing;
        };
        match c {
            '^' | '$' => {
                self.at += 1;
                Term::Assertion
            }
            '*' | '+' | '?' => {
                self.at += 1;
                self.fail("a quantifier has nothing to repeat");
                Term::Nothing
            }
            '{' => {
                // A `{` that opens a valid quantifier here has nothing before
                // it to repeat. One that does not is a literal brace — legal
                // without `u`, and not with it.
                let start = self.at;
                self.at += 1;
                if self.braced_quantifier() {
                    self.fail("a quantifier has nothing to repeat");
                } else if self.unicode {
                    self.fail("a lone `{` is not allowed in a unicode pattern");
                } else {
                    self.at = start + 1;
                }
                Term::Atom
            }
            '(' => self.group(),
            '[' => {
                self.character_class();
                Term::Atom
            }
            '\\' => self.escape(),
            _ => {
                self.at += 1;
                Term::Atom
            }
        }
    }

    /// Everything that starts with `(`.
    fn group(&mut self) -> Term {
        self.at += 1;
        if !self.eat('?') {
            self.declare_capture_name(None);
            self.body_of_group();
            return Term::Atom;
        }

        if self.eat(':') {
            self.body_of_group();
            return Term::Atom;
        }
        if self.eat('=') || self.eat('!') {
            self.body_of_group();
            return Term::Lookahead;
        }
        if self.peek() == Some('<') {
            self.at += 1;
            if self.eat('=') || self.eat('!') {
                self.body_of_group();
                return Term::Assertion;
            }
            match self.group_name() {
                Some(name) => self.declare_capture_name(Some(name)),
                None => return Term::Nothing,
            }
            self.body_of_group();
            return Term::Atom;
        }

        self.modifiers();
        if self.error.is_some() {
            return Term::Nothing;
        }
        self.body_of_group();
        Term::Atom
    }

    fn body_of_group(&mut self) {
        self.disjunction();
        if self.error.is_some() {
            return;
        }
        if !self.eat(')') {
            self.fail("a group is not closed");
        }
    }

    /// `(?ims-ims:` — the flags a group turns on and off for itself.
    ///
    /// Every rule here says the same thing: the modifier has to *mean*
    /// something. A letter on both sides both adds and removes it, a repeated
    /// letter says one thing twice, and `(?-:x)` changes nothing at all — so
    /// each is refused rather than given an arbitrary reading.
    fn modifiers(&mut self) {
        let mut added: Vec<char> = Vec::new();
        let mut removed: Vec<char> = Vec::new();
        let mut removing = false;
        loop {
            match self.peek() {
                Some(':') => break,
                Some('-') if !removing => {
                    removing = true;
                    self.at += 1;
                }
                Some(letter @ ('i' | 'm' | 's')) => {
                    self.at += 1;
                    let side = if removing { &mut removed } else { &mut added };
                    if side.contains(&letter) {
                        return self.fail("a modifier is given twice");
                    }
                    side.push(letter);
                }
                _ => return self.fail("this is not a modifier"),
            }
        }
        if added.is_empty() && removed.is_empty() {
            return self.fail("a modifier group adds and removes nothing");
        }
        if removing && removed.is_empty() {
            return self.fail("a modifier group removes nothing");
        }
        if let Some(letter) = added.iter().find(|letter| removed.contains(letter)) {
            return self.fail(&format!("`{letter}` is both added and removed"));
        }
        self.at += 1;
    }

    /// The name of a `(?<name>` group, with the `>` consumed.
    fn group_name(&mut self) -> Option<String> {
        let mut name = String::new();
        loop {
            match self.peek() {
                Some('>') => {
                    self.at += 1;
                    break;
                }
                None => {
                    self.fail("a group name is not closed");
                    return None;
                }
                Some(c) => {
                    self.at += 1;
                    name.push(c);
                }
            }
        }
        if !is_identifier_text(&name) {
            self.fail("a group name is not an identifier");
            return None;
        }
        Some(name)
    }

    fn declare_capture_name(&mut self, name: Option<String>) {
        let Some(name) = name else {
            return;
        };
        let taken = self.open.iter().any(|(current, _)| current.contains(&name));
        if taken {
            return self.fail(&format!("`{name}` names two groups of one alternative"));
        }
        if let Some((current, _)) = self.open.last_mut() {
            current.insert(name);
        }
    }

    /// A quantifier, if one follows, and whether the term takes it.
    fn quantifier(&mut self, term: Term) {
        let start = self.at;
        let present = match self.peek() {
            Some('*' | '+' | '?') => {
                self.at += 1;
                true
            }
            Some('{') => {
                self.at += 1;
                if self.braced_quantifier() {
                    true
                } else {
                    self.at = start;
                    false
                }
            }
            _ => false,
        };
        if !present || self.error.is_some() {
            return;
        }
        // `?` again is the lazy form, not a second quantifier.
        self.eat('?');

        let quantifiable = match term {
            Term::Atom => true,
            // Annex B lets a lookahead be repeated, which means nothing useful
            // and is what the web does. Unicode mode withdraws it.
            Term::Lookahead => !self.unicode,
            Term::Assertion | Term::Nothing => false,
        };
        if !quantifiable {
            self.fail("this cannot be quantified");
        }
    }

    /// `{n}`, `{n,}` or `{n,m}`, having consumed the `{`.
    ///
    /// Answers whether one was there at all: without `u` a `{` that does not
    /// form one is an ordinary character, so this has to be able to say no
    /// without failing.
    fn braced_quantifier(&mut self) -> bool {
        let start = self.at;
        let Some(least) = self.digits() else {
            self.at = start;
            return false;
        };
        // `{n}` is `{n,n}`; `{n,}` has no upper bound at all, which is why the
        // absent digits are `None` rather than a large number.
        let most = if self.eat(',') {
            self.digits()
        } else {
            Some(least)
        };
        if !self.eat('}') {
            self.at = start;
            return false;
        }
        if let Some(most) = most
            && most < least
        {
            self.fail("a quantifier counts down");
        }
        true
    }

    fn digits(&mut self) -> Option<u64> {
        let start = self.at;
        let mut value: u64 = 0;
        while let Some(c) = self.peek() {
            let Some(digit) = c.to_digit(10) else {
                break;
            };
            self.at += 1;
            value = value.saturating_mul(10).saturating_add(u64::from(digit));
        }
        (self.at > start).then_some(value)
    }

    /// Everything that starts with a backslash, outside a class.
    fn escape(&mut self) -> Term {
        self.at += 1;
        let Some(c) = self.peek() else {
            self.fail("a pattern ends in a backslash");
            return Term::Nothing;
        };
        match c {
            'b' | 'B' => {
                self.at += 1;
                Term::Assertion
            }
            'k' => {
                self.at += 1;
                self.backreference_name();
                Term::Atom
            }
            _ => {
                self.escape_body();
                Term::Atom
            }
        }
    }

    /// The `<name>` of a `\k`, if this pattern is one where it is required.
    fn backreference_name(&mut self) {
        if !self.unicode && self.declared.is_empty() {
            return;
        }
        if !self.eat('<') {
            return self.fail("`\\k` is not followed by a group name");
        }
        let mut name = String::new();
        loop {
            match self.peek() {
                Some('>') => {
                    self.at += 1;
                    break;
                }
                None => return self.fail("a group name is not closed"),
                Some(c) => {
                    self.at += 1;
                    name.push(c);
                }
            }
        }
        if !is_identifier_text(&name) {
            return self.fail("a group name is not an identifier");
        }
        self.referenced.push(name);
    }

    /// One escape's body, having consumed the backslash and knowing it is not
    /// `b`, `B` or `k`.
    ///
    /// Only unicode mode is checked. Without it Annex B says almost every
    /// escape means the character it precedes, so there is nothing to refuse.
    fn escape_body(&mut self) {
        let Some(c) = self.peek() else {
            return;
        };
        self.at += 1;
        match c {
            '0'..='9' => {
                if !self.unicode {
                    return;
                }
                if c == '0' {
                    // `\0` is the null character, and a digit after it makes it
                    // a legacy octal escape instead.
                    if self.peek().is_some_and(|next| next.is_ascii_digit()) {
                        self.fail("a legacy octal escape is not allowed in a unicode pattern");
                    }
                    return;
                }
                if c == '8' || c == '9' {
                    return self.fail("`\\8` and `\\9` are not allowed in a unicode pattern");
                }
                let mut value = c.to_digit(10).unwrap_or(0) as usize;
                while let Some(digit) = self.peek().and_then(|next| next.to_digit(10)) {
                    self.at += 1;
                    value = value * 10 + digit as usize;
                }
                if value > self.capture_groups {
                    self.fail("a backreference names no group in this pattern");
                }
            }
            'c' => {
                // `\cX` is a control escape, and only a letter follows it.
                if !self.peek().is_some_and(|next| next.is_ascii_alphabetic()) && self.unicode {
                    self.fail("`\\c` is not followed by a letter");
                }
            }
            'u' => self.unicode_escape(),
            'x' => {
                if self.unicode && !self.hex_digits(2) {
                    self.fail("`\\x` needs two hexadecimal digits");
                }
            }
            'p' | 'P' if self.unicode => {
                if !self.eat('{') {
                    return self.fail("`\\p` needs a property name in braces");
                }
                while self.peek().is_some_and(|next| next != '}') {
                    self.at += 1;
                }
                if !self.eat('}') {
                    self.fail("`\\p` needs a property name in braces");
                }
            }
            _ => {
                if self.unicode && !is_identity_escape(c) {
                    self.fail("this escape has no meaning in a unicode pattern");
                }
            }
        }
    }

    /// `\uHHHH` or `\u{H…}`, having consumed the `u`.
    fn unicode_escape(&mut self) {
        if self.eat('{') {
            let start = self.at;
            let mut value: u32 = 0;
            let mut any = false;
            while let Some(c) = self.peek() {
                let Some(digit) = c.to_digit(16) else {
                    break;
                };
                self.at += 1;
                any = true;
                value = value.saturating_mul(16).saturating_add(digit);
            }
            if !any || !self.eat('}') {
                self.at = start;
                if self.unicode {
                    self.fail("`\\u{…}` needs hexadecimal digits and a closing brace");
                }
                return;
            }
            if value > 0x10_FFFF {
                self.fail("a code point escape is past the last code point");
            }
            return;
        }
        if self.unicode && !self.hex_digits(4) {
            self.fail("`\\u` needs four hexadecimal digits");
        }
    }

    fn hex_digits(&mut self, count: usize) -> bool {
        let start = self.at;
        for _ in 0..count {
            if self.peek().is_some_and(|c| c.is_ascii_hexdigit()) {
                self.at += 1;
            } else {
                self.at = start;
                return false;
            }
        }
        true
    }

    /// `[…]`, whose contents follow a grammar of their own.
    ///
    /// The rule worth having here is the one about ranges: a range's ends are
    /// single characters, so `[\d-a]` asks for a range from a *set*, which has
    /// no first character. Annex B reads it as three literals instead, so this
    /// is a unicode-mode rule.
    fn character_class(&mut self) {
        self.at += 1;
        self.eat('^');
        // What the last item was, for the range rule: `Some(true)` for a class
        // escape, `Some(false)` for a single character.
        let mut previous: Option<bool> = None;
        loop {
            match self.peek() {
                None => return self.fail("a character class is not closed"),
                Some(']') => {
                    self.at += 1;
                    return;
                }
                Some('-') => {
                    self.at += 1;
                    // A `-` at either end of the class is a literal, and one
                    // between two items is a range.
                    if previous.is_none() || self.peek() == Some(']') {
                        previous = Some(false);
                        continue;
                    }
                    let left_is_a_set = previous == Some(true);
                    let right_is_a_set = self.class_item();
                    if self.error.is_some() {
                        return;
                    }
                    if self.unicode && (left_is_a_set || right_is_a_set) {
                        return self.fail("a character class range needs single characters");
                    }
                    previous = None;
                }
                _ => previous = Some(self.class_item()),
            }
            if self.error.is_some() {
                return;
            }
        }
    }

    /// One item of a class, answering whether it stands for a *set* of
    /// characters rather than one.
    fn class_item(&mut self) -> bool {
        let Some(c) = self.peek() else {
            return false;
        };
        if c != '\\' {
            self.at += 1;
            return false;
        }
        self.at += 1;
        let Some(next) = self.peek() else {
            self.fail("a pattern ends in a backslash");
            return false;
        };
        if matches!(next, 'd' | 'D' | 's' | 'S' | 'w' | 'W' | 'p' | 'P') {
            let set = next != 'p' && next != 'P';
            self.escape_body();
            return set || self.unicode;
        }
        // `\b` is a backspace inside a class rather than a word boundary, which
        // is the one place the two spellings mean different things.
        if next == 'b' {
            self.at += 1;
            return false;
        }
        self.escape_body();
        false
    }
}

/// Whether the text between `<` and `>` is a legal group name.
///
/// Deliberately not the full `IdentifierName` grammar with escapes: what is
/// checked is that it starts with something a name may start with and contains
/// nothing obviously outside one. A name this accepts and the specification
/// does not is an accepted invalid program, which is the direction this module
/// errs in on purpose.
fn is_identifier_text(name: &str) -> bool {
    let mut characters = name.chars();
    let Some(first) = characters.next() else {
        return false;
    };
    if !(first.is_alphabetic() || first == '_' || first == '$' || first == '\\') {
        return false;
    }
    characters
        .all(|c| c.is_alphanumeric() || c == '_' || c == '$' || c == '\\' || c == '{' || c == '}')
}

/// Whether a character may follow a backslash and mean itself.
fn is_identity_escape(c: char) -> bool {
    matches!(
        c,
        '^' | '$'
            | '\\'
            | '.'
            | '*'
            | '+'
            | '?'
            | '('
            | ')'
            | '['
            | ']'
            | '{'
            | '}'
            | '|'
            | '/'
            | 'd'
            | 'D'
            | 's'
            | 'S'
            | 'w'
            | 'W'
            | 'f'
            | 'n'
            | 'r'
            | 't'
            | 'v'
    )
}

/// Every `(?<name>` in the pattern, found before it is parsed.
///
/// A backreference may precede the group it names — `/\k<a>(?<a>x)/` is valid —
/// so the set has to exist before the walk that would check one.
fn declared_group_names(text: &[char]) -> HashSet<String> {
    let mut names = HashSet::new();
    let mut index = 0;
    while index + 2 < text.len() {
        if text[index] == '\\' {
            index += 2;
            continue;
        }
        if text[index] == '(' && text[index + 1] == '?' && text[index + 2] == '<' {
            let mut at = index + 3;
            // `(?<=` and `(?<!` are lookbehinds and name nothing.
            if matches!(text.get(at), Some('=') | Some('!')) {
                index = at;
                continue;
            }
            let mut name = String::new();
            while let Some(&c) = text.get(at) {
                at += 1;
                if c == '>' {
                    names.insert(name);
                    break;
                }
                name.push(c);
            }
            index = at;
            continue;
        }
        index += 1;
    }
    names
}

/// How many capturing groups the pattern has, for the backreference rule.
fn count_capture_groups(text: &[char]) -> usize {
    let mut count = 0;
    let mut index = 0;
    while index < text.len() {
        match text[index] {
            '\\' => index += 1,
            '[' => {
                // A `(` inside a class is a literal, so the scan skips classes
                // whole rather than counting what they contain.
                index += 1;
                while index < text.len() && text[index] != ']' {
                    index += if text[index] == '\\' { 2 } else { 1 };
                }
            }
            '(' if is_capturing(text, index) => count += 1,
            _ => {}
        }
        index += 1;
    }
    count
}

/// Whether a `(` at this position opens a capturing group.
///
/// `(` alone does, and so does `(?<name>` — the named form captures exactly as
/// the plain one does, which is the case a scan looking only for "not followed
/// by `?`" gets wrong, and getting it wrong makes `/(?<a>x)\1/` look like a
/// backreference to nothing.
fn is_capturing(text: &[char], index: usize) -> bool {
    match text.get(index + 1) {
        Some('?') => {
            text.get(index + 2) == Some(&'<')
                && !matches!(text.get(index + 3), Some('=') | Some('!'))
        }
        _ => true,
    }
}
