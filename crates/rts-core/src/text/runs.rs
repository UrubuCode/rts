//! Applying a text operation to a string that may not be valid Unicode.
//!
//! # The problem this exists for
//!
//! Case mapping and normalisation are both defined over TEXT — `str::to_lowercase`
//! and the normalisation forms both take a `&str` — and a JavaScript string need
//! not be one. `Str::to_rust` answers nothing for a lone surrogate, so every
//! method written over it answered `undefined` for a string the language handles
//! perfectly well: `("a" + halfOfAnEmoji).toUpperCase()` was a missing method
//! rather than a wrong answer.
//!
//! # Why runs, and not one code point at a time
//!
//! Because both operations have rules that reach ACROSS characters, and mapping
//! each in isolation loses them. `"ΑΣ".toLowerCase()` is `"ας"` — a final sigma,
//! decided by what follows it — and `char::to_lowercase('Σ')` cannot know. The
//! same for normalisation, where composition is exactly a rule about a character
//! and its neighbour.
//!
//! So the longest runs of valid text are handed to the operation whole, and only
//! the unpaired surrogates between them are carried across untouched. That is
//! not an approximation: an unpaired surrogate has no case mapping and combining
//! class zero, so it ends a run by the definitions rather than by our choice.

use super::Str;

/// The text with `body` applied to each run of valid Unicode in it.
///
/// Unpaired surrogates pass through as themselves, at the positions they held.
pub fn mapped_runs(text: &Str, body: impl Fn(&str) -> String) -> Str {
    let mut out: Vec<u16> = Vec::with_capacity(text.len());
    let mut run = String::new();
    for decoded in char::decode_utf16(text.units()) {
        match decoded {
            Ok(character) => run.push(character),
            Err(unpaired) => {
                push_mapped(&mut out, &run, &body);
                run.clear();
                out.push(unpaired.unpaired_surrogate());
            }
        }
    }
    push_mapped(&mut out, &run, &body);
    Str::from_utf16(&out)
}

/// One run, mapped and appended as code units.
///
/// The empty check is not an optimisation: a string that STARTS with a lone
/// surrogate has an empty run before it, and handing `""` to a normalisation
/// would allocate a `String` per half-character.
fn push_mapped(out: &mut Vec<u16>, run: &str, body: impl Fn(&str) -> String) {
    if run.is_empty() {
        return;
    }
    out.extend(body(run).encode_utf16());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rule_that_reaches_across_characters_still_applies() {
        // The case a per-code-point map gets wrong: sigma lowercases to the
        // FINAL form at the end of a word, which is decided by what follows it.
        let word = Str::from_str("ΑΣ");
        assert_eq!(
            mapped_runs(&word, str::to_lowercase).to_rust().as_deref(),
            Some("ας")
        );
    }

    #[test]
    fn an_unpaired_surrogate_splits_the_runs_and_survives() {
        let broken = Str::from_utf16(&[0x0061, 0xD800, 0x0062]);
        let mapped = mapped_runs(&broken, str::to_uppercase);
        assert!(mapped.same_units(&Str::from_utf16(&[0x0041, 0xD800, 0x0042])));
    }

    #[test]
    fn a_string_that_is_only_surrogates_is_answered_unchanged() {
        let halves = Str::from_utf16(&[0xD800, 0xDBFF]);
        assert!(mapped_runs(&halves, str::to_uppercase).same_units(&halves));
    }
}
