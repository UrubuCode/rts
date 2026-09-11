//! Two Annex B corners of a digit after a backslash: which of them are octal,
//! and which of the remaining genuine backreferences point at a group that
//! has not opened yet.
//!
//! Split out of [`super::super::translate`] rather than added to it, for the
//! same reason that file gives for staying flat otherwise: each of these two
//! needs its own notion of "how many capturing groups does this pattern
//! declare, and where does each one open", which neither of the existing
//! rewrites there needs at all.

/// The byte offset of every CAPTURING group's opening `(`, in the order the
/// groups are numbered.
///
/// `(?:`, `(?=`, `(?!`, `(?<=` and `(?<!` do not count — none of them can be
/// referred to by a backreference. `(?<name>` does: a named group is still a
/// capturing one, numbered the same as an unnamed one would be at that
/// position.
fn capturing_group_positions(pattern: &str) -> Vec<usize> {
    let mut positions = Vec::new();
    let mut inside = false;
    let mut characters = pattern.char_indices();
    while let Some((at, character)) = characters.next() {
        if character == '\\' {
            characters.next();
            continue;
        }
        match character {
            '[' if !inside => inside = true,
            ']' if inside => inside = false,
            '(' if !inside => {
                let rest = &pattern[at + 1..];
                let non_capturing = rest.starts_with(":")
                    || rest.starts_with('=')
                    || rest.starts_with('!')
                    || rest.starts_with("<=")
                    || rest.starts_with("<!");
                if !non_capturing {
                    positions.push(at);
                }
            }
            _ => {}
        }
    }
    positions
}

/// Rewrites every digit escape Annex B reads as OCTAL rather than as a
/// backreference.
///
/// Two contexts and one rule between them: a class has no such thing as a
/// backreference at all, so every `\N` inside `[...]` is
/// `LegacyOctalEscapeSequence`. Outside a class it is a backreference exactly
/// when its number does not exceed however many capturing groups the WHOLE
/// pattern declares — Annex B's own test, independent of whether that group
/// is before or after this escape — and every number past that count reads
/// as octal too, which is what turns `(a)\2` into `(a)` followed by a literal
/// STX character rather than a refusal.
///
/// Skipped entirely under the `u`/`v` grammar: there both readings are
/// refused rather than guessed at, which is what the flag is FOR — see
/// [`super::super::translate::identity_escapes`] for the same gate on the
/// property-escape question.
pub(in crate::entry::regex) fn legacy_octal_escapes(pattern: &str) -> String {
    let group_count = capturing_group_positions(pattern).len();
    let characters: Vec<char> = pattern.chars().collect();
    let mut out = String::with_capacity(pattern.len());
    let mut inside = false;
    let mut at = 0;
    while at < characters.len() {
        let character = characters[at];
        if character == '\\'
            && characters
                .get(at + 1)
                .is_some_and(char::is_ascii_digit)
        {
            let digit_start = at + 1;
            if inside {
                at = emit_octal(&characters, digit_start, &mut out);
                continue;
            }
            let mut end = digit_start;
            while characters.get(end).is_some_and(char::is_ascii_digit) {
                end += 1;
            }
            let number: String = characters[digit_start..end].iter().collect();
            let value: u64 = number.parse().unwrap_or(u64::MAX);
            if value != 0 && (value as usize) <= group_count {
                // A genuine backreference. Left for whichever of this
                // module's two functions runs next to decide.
                out.push('\\');
                out.push_str(&number);
                at = end;
                continue;
            }
            at = emit_octal(&characters, digit_start, &mut out);
            continue;
        }
        if character == '\\' {
            out.push(character);
            if let Some(&next) = characters.get(at + 1) {
                out.push(next);
                at += 2;
            } else {
                at += 1;
            }
            continue;
        }
        match character {
            '[' if !inside => inside = true,
            ']' if inside => inside = false,
            _ => {}
        }
        out.push(character);
        at += 1;
    }
    out
}

/// Consumes the octal digits starting at `digit_start` — a backslash the
/// caller already wrote — and appends the character they spell, or the bare
/// digit for `8`/`9`, which Annex B falls back to `IdentityEscape` for rather
/// than any octal reading. Returns the index just past what this consumed;
/// any digit that did not fit the escape is left for the next iteration to
/// read as an ordinary character, which is what makes `\400` two characters
/// rather than one out-of-range codepoint.
///
/// The `0`–`3` / `4`–`7` split is Annex B's own limit on the three-digit
/// form, which is what keeps the value at or under 255 (`\3` then two more
/// octal digits is at most `0o377`, while `\4`.. stops at one more digit so
/// the same bound holds from the other side).
fn emit_octal(characters: &[char], digit_start: usize, out: &mut String) -> usize {
    let first = characters[digit_start];
    let Some(first_value) = first.to_digit(8) else {
        out.push(first);
        return digit_start + 1;
    };
    let max_extra = if first_value <= 3 { 2 } else { 1 };
    let mut value = first_value;
    let mut consumed = 1;
    while consumed <= max_extra {
        match characters.get(digit_start + consumed).and_then(|c| c.to_digit(8)) {
            Some(digit) => {
                value = value * 8 + digit;
                consumed += 1;
            }
            None => break,
        }
    }
    out.push_str(&format!("\\x{{{value:02x}}}"));
    digit_start + consumed
}

/// A backreference to a group that has not opened yet, rewritten to `(?:)` —
/// the empty match a program observes, because the language says a
/// backreference to a group that has not participated succeeds without
/// consuming anything.
///
/// # Why this engine cannot simply run the pattern as written
///
/// `fancy-regex`'s own static analysis refuses a backreference whose group
/// number is not already open at the point it appears
/// (`CompileError::InvalidBackref`, checked against a running count of groups
/// seen so far) — not a parser quirk but the shape of its backtracking VM,
/// which addresses a capture slot that has to already exist. So a forward
/// reference is not a pattern this engine declines and a second one accepts;
/// it is one NEITHER accepts, and the rewrite exists because the answer is
/// otherwise unreachable rather than because it is convenient.
///
/// # Why this is gated on the WHOLE pattern having no quantifier
///
/// The rewrite is sound only where the referenced group cannot have
/// participated by the time control reaches this point a SECOND time either —
/// and a quantifier is exactly what could make that happen: `(?:\1(a))+`
/// forwards on the first pass but `\1` reads what the FIRST iteration's `(a)`
/// captured on the second. Proving that never applies to one particular
/// group would need tracking which quantifiers enclose which groups; refusing
/// to touch a pattern that has any quantifier at all is the coarser rule that
/// is provably safe instead, at the cost of leaving a repeating forward
/// reference with the refusal it already had — which is not a regression,
/// only a gap still open.
pub(in crate::entry::regex) fn forward_backreferences_as_empty(pattern: &str) -> String {
    if pattern.contains(['*', '+', '{']) {
        return pattern.to_owned();
    }
    let group_positions = capturing_group_positions(pattern);
    let indexed: Vec<(usize, char)> = pattern.char_indices().collect();
    let mut out = String::with_capacity(pattern.len());
    let mut inside = false;
    let mut at = 0;
    while at < indexed.len() {
        let (byte_at, character) = indexed[at];
        if character == '\\' && !inside {
            if let Some(&(_, next)) = indexed.get(at + 1)
                && next.is_ascii_digit()
            {
                let digit_start = at + 1;
                let mut end = digit_start;
                while indexed.get(end).is_some_and(|(_, c)| c.is_ascii_digit()) {
                    end += 1;
                }
                let number: String = indexed[digit_start..end].iter().map(|&(_, c)| c).collect();
                let forward = number
                    .parse::<usize>()
                    .ok()
                    .filter(|&group| group >= 1 && group <= group_positions.len())
                    .is_some_and(|group| group_positions[group - 1] > byte_at);
                if forward {
                    out.push_str("(?:)");
                } else {
                    out.push('\\');
                    out.push_str(&number);
                }
                at = end;
                continue;
            }
            out.push(character);
            if let Some(&(_, next)) = indexed.get(at + 1) {
                out.push(next);
                at += 2;
            } else {
                at += 1;
            }
            continue;
        }
        match character {
            '[' if !inside => inside = true,
            ']' if inside => inside = false,
            _ => {}
        }
        out.push(character);
        at += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_digit_escape_inside_a_class_is_always_octal() {
        assert_eq!(legacy_octal_escapes(r"[\101]"), "[\\x{41}]");
    }

    #[test]
    fn a_digit_run_past_the_group_count_is_octal_outside_a_class_too() {
        // No group at all: `\1` cannot be a backreference.
        assert_eq!(legacy_octal_escapes(r"^\1a$"), "^\\x{1}a$");
        // One group: `\2` is one past it.
        assert_eq!(legacy_octal_escapes(r"(a)\2"), "(a)\\x{2}");
        // In range: left untouched for the caller after this one.
        assert_eq!(legacy_octal_escapes(r"(a)\1"), r"(a)\1");
    }

    #[test]
    fn three_digits_take_the_three_digit_form_only_from_zero_to_three() {
        assert_eq!(legacy_octal_escapes(r"\101"), "\\x{41}");
        // `\77`: first digit 7 is the two-digit branch, value 0o77 = 63.
        assert_eq!(legacy_octal_escapes(r"\77"), "\\x{3f}");
        // `\400`: 0o40 = 32, then a literal `0` left for the next character.
        assert_eq!(legacy_octal_escapes(r"\400"), "\\x{20}0");
    }

    #[test]
    fn eight_and_nine_are_never_octal_and_never_a_backreference() {
        assert_eq!(legacy_octal_escapes(r"\8"), "8");
        assert_eq!(legacy_octal_escapes(r"\9"), "9");
    }

    #[test]
    fn the_unicode_grammar_is_left_for_the_engine_to_refuse() {
        // Callers gate this function on the flag; it does not gate itself,
        // so this pins that a caller who forgets sees the unsafe rewrite
        // rather than a silent no-op — the gate belongs in `compile.rs`.
        assert_ne!(legacy_octal_escapes(r"\1"), r"\1");
    }

    #[test]
    fn a_forward_reference_becomes_an_empty_group() {
        assert_eq!(forward_backreferences_as_empty(r"\1(a)"), "(?:)(a)");
        assert_eq!(forward_backreferences_as_empty(r"^\1(a)$"), "^(?:)(a)$");
    }

    #[test]
    fn a_backward_reference_is_left_alone() {
        assert_eq!(forward_backreferences_as_empty(r"(a)\1"), r"(a)\1");
    }

    #[test]
    fn a_quantified_pattern_is_left_alone_entirely() {
        // Unsound to rewrite: `\1` could read a LATER iteration's capture.
        let pattern = r"(?:\1(a))+";
        assert_eq!(forward_backreferences_as_empty(pattern), pattern);
    }

    #[test]
    fn a_named_group_counts_the_same_as_an_unnamed_one() {
        assert_eq!(capturing_group_positions("(?<n>a)"), vec![0]);
        assert_eq!(capturing_group_positions("(?:a)"), Vec::<usize>::new());
        assert_eq!(capturing_group_positions("(?=a)"), Vec::<usize>::new());
        assert_eq!(capturing_group_positions("(?<=a)"), Vec::<usize>::new());
        assert_eq!(capturing_group_positions("(a)(b)"), vec![0, 3]);
    }
}
