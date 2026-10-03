//! Rule 2, as something that fails rather than something that is read.
//!
//! The README's *What this crate is not* says it outright: "if an identifier, a
//! string, or even a comment in this crate names a source-language construct,
//! that is a defect in the layering". It had been true and unchecked, and on
//! 2026-10-03 the scan below found five — an instruction documented as what
//! "JavaScript's bitwise operators" read, a verifier comment calling a tagged
//! value "a JavaScript value", and three more.
//!
//! `rts-mir` states the same rule as a `grep` a person runs (its rule 1), and
//! that crate is clean while this one was not. The difference is not discipline;
//! it is that nothing here ran the grep. So this is the grep, in the test
//! binary, which is the form rule 7 asks for.
//!
//! # Why the word list is short, and why `undefined` is not on it
//!
//! Because the check has to be exactly right to be worth having. `undefined`
//! occurs nineteen times in this crate and almost every one is a LINKER's
//! undefined symbol or an undefined bit pattern — neither of which is a
//! language's `undefined`. A scan that flagged them would be silenced by an
//! allowance per site, and an allowance list longer than the defect list is how
//! a check stops being read.
//!
//! What is on the list is a word that can only be a source language: the name of
//! one, or the name of a construct no machine has.
//!
//! # Why a word boundary, which the first version of this file did not have
//!
//! `lua` occurs inside `evaluate`, so a plain substring scan failed on this
//! file's own neighbour and would have been "fixed" by dropping the shortest and
//! most important word on the list. The boundary is required at the START only —
//! a leak writes `prototypes` as readily as `prototype`, and a suffix never
//! changes which language is being named.

use std::fs;
use std::path::{Path, PathBuf};

/// Words that cannot appear in this crate except deliberately, matched
/// case-insensitively.
const LANGUAGE_WORDS: &[&str] = &[
    "javascript",
    "typescript",
    "ecmascript",
    "metatable",
    "prototype",
    "lua",
];

/// Occurrences this crate means, with the reason each is not the defect rule 2
/// describes.
///
/// Two kinds, and the distinction is the whole content of the allowance:
///
/// - **This crate stating its own non-scope.** `lib.rs` says it does not know
///   what `undefined`, `nil`, a prototype or a metatable is. A rule cannot be
///   written without naming what it excludes.
/// - **Citing another implementation as evidence.** "The discipline a production
///   JavaScript engine uses" is a fact about somebody else's engine, offered as
///   support for a decision here, and README rule 4 asks for exactly that —
///   name the alternative and where the fact came from. It is not this layer
///   describing its own semantics in a language's terms, which is what leaks.
///
/// Anything else is a defect. Adding a row here is a decision, which is the
/// point: the list is short enough that growing it is visible in a diff.
const ALLOWED: &[(&str, &str)] = &[
    (
        "src/lib.rs",
        "the crate's own statement of scope — the rule names what it excludes",
    ),
    (
        "src/frame/mod.rs",
        "prior art: what two named engines do with a frame record",
    ),
    (
        "src/sched/mod.rs",
        "prior art: the drain-to-empty discipline, cited from an engine that uses it",
    ),
];

/// Whether `word` occurs in `haystack` at the start of a word.
///
/// `haystack` is already lowercased. The preceding character deciding it means
/// `evaluate` does not contain `lua`, while `prototypes` still contains
/// `prototype`.
fn names_word(haystack: &str, word: &str) -> bool {
    let bytes = haystack.as_bytes();
    haystack.match_indices(word).any(|(at, _)| {
        at == 0 || !bytes[at - 1].is_ascii_alphanumeric() && bytes[at - 1] != b'_'
    })
}

/// Every `.rs` file under a directory, relative paths with `/` separators.
fn rust_files(root: &Path, base: &Path, out: &mut Vec<(String, PathBuf)>) {
    let entries = fs::read_dir(root).unwrap_or_else(|e| panic!("read {}: {e}", root.display()));
    for entry in entries {
        let path = entry.expect("directory entry").path();
        if path.is_dir() {
            rust_files(&path, base, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            let rel = path
                .strip_prefix(base)
                .expect("under the crate root")
                .to_string_lossy()
                .replace('\\', "/");
            out.push((rel, path));
        }
    }
}

/// The machine layer describes its own semantics without naming a source
/// language.
///
/// A failure is not a style complaint. The name is evidence that a decision in
/// this crate was taken for one client, and the next client inherits it without
/// being asked — which is the whole of `docs/engine/a-second-language.md`:
/// "a boundary with one client on each side is indistinguishable from no
/// boundary at all".
#[test]
fn no_module_describes_itself_in_a_language() {
    let base = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    rust_files(&base.join("src"), base, &mut files);
    assert!(
        files.len() > 20,
        "the scan found {} files, which means it is looking in the wrong place \
         and would pass by finding nothing",
        files.len()
    );

    let mut found = Vec::new();
    for (rel, path) in &files {
        if ALLOWED.iter().any(|(allowed, _)| allowed == rel) {
            continue;
        }
        let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("read {rel}: {e}"));
        for (number, line) in text.lines().enumerate() {
            let lowered = line.to_ascii_lowercase();
            for word in LANGUAGE_WORDS {
                if names_word(&lowered, word) {
                    found.push(format!("{rel}:{} names {word}: {}", number + 1, line.trim()));
                }
            }
        }
    }

    assert!(
        found.is_empty(),
        "the machine layer names a source language in {} place(s). Describe the \
         operation instead, or — if this is prior art or the crate's own \
         non-scope — add the file to ALLOWED with the reason:\n{}",
        found.len(),
        found.join("\n")
    );
}

/// An allowance that no longer matches anything is removed.
///
/// The failure this prevents is the one an allowance list always acquires: a row
/// outliving the occurrence it excused, so the next leak into that file is
/// permitted by a line nobody can date. Rule 6's "no dead code" applied to the
/// check itself.
#[test]
fn every_allowance_still_excuses_something() {
    let base = Path::new(env!("CARGO_MANIFEST_DIR"));
    for (rel, why) in ALLOWED {
        let path = base.join(rel);
        let text = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("allowance names {rel}, which cannot be read: {e}"));
        let lowered = text.to_ascii_lowercase();
        assert!(
            LANGUAGE_WORDS.iter().any(|word| names_word(&lowered, word)),
            "{rel} is allowed to name a language because {why}, and it no longer \
             names one. Remove the allowance."
        );
    }
}
