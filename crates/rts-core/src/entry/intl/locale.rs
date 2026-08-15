//! The locale a formatter was asked for, and the one it will use.
//!
//! # Why this is four functions and not a table
//!
//! The first version of this file WAS a table: three English tags, a
//! prefix match, and a hand-written rule saying that `en-GB` writes the day
//! first. It was written under the belief that this crate could not carry CLDR
//! data, and it would have made `Intl.NumberFormat("pt-BR")` format in English
//! while reporting that it had — the wrong answer that looks right, and the
//! exact failure the `sync` namespace was deleted for.
//!
//! ICU4X carries the data, in Rust, compiled into the binary. So the locale is
//! negotiated by the library that owns the question, this file only carries a
//! tag across the boundary, and `resolvedOptions().locale` reports what the
//! library resolved rather than what this crate guessed.

use icu::locale::Locale;

/// The tag used when a program names none.
///
/// `und` — the root locale, which is what ICU4X falls back to and what the
/// specification calls the default when there is no host preference. Not
/// `en-US`: naming a real locale as the default is a claim about the reader.
pub(super) const ROOT: &str = "und";

/// The locale a tag names, or the root when it names nothing usable.
///
/// A malformed tag is not a refusal here. `Intl.NumberFormat("not a tag")`
/// throws a `RangeError` in the specification, and that refusal belongs at the
/// constructor where the argument was written — this answers what to format
/// with, and every caller has already decided what a bad tag means.
pub(super) fn parse(tag: Option<&str>) -> Locale {
    tag.and_then(|tag| Locale::try_from_str(tag.trim()).ok())
        .unwrap_or_else(|| Locale::try_from_str(ROOT).expect("the root tag parses"))
}

/// The canonical spelling of a tag — `EN-us` is `en-US`.
pub(super) fn canonical(tag: &str) -> Option<String> {
    Locale::try_from_str(tag.trim())
        .ok()
        .map(|locale| locale.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tag nobody wrote falls back to the root rather than to a language.
    #[test]
    fn the_default_is_the_root_and_not_a_language() {
        assert_eq!(parse(None).to_string(), ROOT);
    }

    /// The canonical form is the library's, not a case fold written here.
    #[test]
    fn a_tag_is_canonicalised_by_the_library() {
        assert_eq!(canonical("EN-us").as_deref(), Some("en-US"));
        assert_eq!(canonical("pt-br").as_deref(), Some("pt-BR"));
        assert_eq!(canonical("not a tag"), None);
    }
}
