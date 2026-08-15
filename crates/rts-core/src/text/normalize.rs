//! The four Unicode normalisation forms.
//!
//! # Why a table is carried at all, when `localeCompare`'s is refused
//!
//! They are not the same kind of data. A collation table is **per locale** — it
//! is what a program has to be shipped with, so it fails rule 1 of this crate,
//! which is availability on every target. Normalisation is the opposite: there
//! is exactly one answer, the same everywhere, defined by the Unicode database
//! and not by anybody's language. `"e\u{301}".normalize() === "\u{e9}"` is true
//! in every engine, and a runtime that answers false is not making a locale
//! choice, it is wrong.
//!
//! `normalize` was the identity here, stated as a wrong answer a program could
//! find. What it cost is not cosmetic: normalising before comparing is the whole
//! reason the method exists, so every program that did it compared unnormalised
//! text and silently found two equal strings unequal.
//!
//! # Why `unicode-normalization` and not a table written here
//!
//! The decomposition and composition data is ~40 KB of generated tables that
//! change with each Unicode release. Writing them here would be a copy nobody
//! regenerates. The crate satisfies what rule 5 asks of a dependency — it is
//! pure Rust over no operating system, so it exists on wasm exactly as here, and
//! it decides nothing the machine decides.
//!
//! # Why this is not written over `&str`
//!
//! Because a JavaScript string need not be valid Unicode, and `normalize` is
//! defined for one that is not: `"\u{D800}".normalize()` answers the same lone
//! surrogate in every engine rather than throwing. [`super::mapped_runs`] is
//! what that costs — the valid runs are normalised and the unpaired surrogates
//! between them carried across — and it is shared with case mapping, which has
//! the same problem for the same reason.

use unicode_normalization::UnicodeNormalization;

use super::{Str, mapped_runs};

/// Which normalisation a program asked for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Form {
    /// Canonical decomposition, then canonical composition. The default.
    Nfc,
    /// Canonical decomposition.
    Nfd,
    /// Compatibility decomposition, then canonical composition.
    Nfkc,
    /// Compatibility decomposition.
    Nfkd,
}

impl Form {
    /// The form a program named, or `None` for a name the language rejects.
    ///
    /// Exactly the four spellings, case-sensitively: the specification lists
    /// them as literal strings and makes anything else a `RangeError`, so
    /// accepting `"nfc"` here would be a program that runs on this engine and
    /// throws on every other.
    pub fn named(name: &str) -> Option<Form> {
        match name {
            "NFC" => Some(Form::Nfc),
            "NFD" => Some(Form::Nfd),
            "NFKC" => Some(Form::Nfkc),
            "NFKD" => Some(Form::Nfkd),
            _ => None,
        }
    }
}

/// The text, normalised.
///
/// Answers a clone for text that is already in the form, which is the common
/// call: ASCII is invariant under all four, so the check below is what keeps
/// `"hello".normalize()` from touching a table at all.
pub fn normalized(text: &Str, form: Form) -> Str {
    // Every ASCII code point is a starter with no decomposition and no
    // compatibility mapping, so all four forms are the identity over it. This
    // is not a heuristic — it is the reason the narrow representation exists,
    // read out for the one operation that would otherwise walk a table per
    // character of nearly every string a program normalises.
    if text.narrow().is_some_and(<[u8]>::is_ascii) {
        return text.clone();
    }
    mapped_runs(text, |run| match form {
        Form::Nfc => run.nfc().collect(),
        Form::Nfd => run.nfd().collect(),
        Form::Nfkc => run.nfkc().collect(),
        Form::Nfkd => run.nfkd().collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composed_and_decomposed_text_meet_in_both_directions() {
        let composed = Str::from_str("\u{e9}");
        let decomposed = Str::from_str("e\u{301}");
        assert!(normalized(&decomposed, Form::Nfc).same_units(&composed));
        assert!(normalized(&composed, Form::Nfd).same_units(&decomposed));
        assert_eq!(normalized(&composed, Form::Nfd).len(), 2, "one unit becomes two");
    }

    #[test]
    fn compatibility_forms_change_what_canonical_ones_leave_alone() {
        // The ligature is canonically distinct from the two letters and
        // compatibly the same, which is the whole difference between the K
        // forms and the others.
        let ligature = Str::from_str("\u{fb01}");
        assert!(normalized(&ligature, Form::Nfc).same_units(&ligature));
        assert!(normalized(&ligature, Form::Nfkc).same_units(&Str::from_str("fi")));
    }

    #[test]
    fn a_lone_surrogate_survives_normalisation() {
        // The case that decides the shape of this module: a string that is not
        // valid Unicode still normalises, and the half-character comes back
        // as itself rather than as U+FFFD.
        let broken = Str::from_utf16(&[0x0065, 0x0301, 0xD800, 0x0065, 0x0301]);
        let answered = normalized(&broken, Form::Nfc);
        assert!(answered.same_units(&Str::from_utf16(&[0x00E9, 0xD800, 0x00E9])));
    }

    #[test]
    fn ascii_is_answered_without_consulting_a_table() {
        let plain = Str::from_str("hello");
        for form in [Form::Nfc, Form::Nfd, Form::Nfkc, Form::Nfkd] {
            assert!(normalized(&plain, form).same_units(&plain));
        }
    }
}
