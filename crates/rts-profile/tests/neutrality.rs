//! Rule 1, as something that fails rather than something that is read.
//!
//! This crate's whole premise is that it holds counts and no meaning, so the
//! rule is worth no more than the thing that enforces it. `rts-mir` states the
//! same rule as a `grep` a person runs and is clean; `rts-cranelift` had no
//! grep and had five leaks on 2026-10-03, every one a comment. The difference
//! was not discipline.
//!
//! # Why a second copy of this scan rather than a shared one
//!
//! Because the alternative is a crate whose only purpose is a forty-line
//! directory walk, depended on by three others as a dev-dependency. The scan is
//! shorter than the manifest that would share it, and this copy needs no
//! allowance list at all — a crate that may not name a language has nothing to
//! excuse, which is itself the difference worth keeping visible.

use std::fs;
use std::path::{Path, PathBuf};

/// Words that can only be a source language: the name of one, or the name of a
/// construct no record has.
///
/// `undefined` is deliberately absent for the reason `rts-cranelift`'s copy
/// gives: almost every occurrence of it in this workspace is a linker's
/// undefined symbol or an undefined bit pattern, so flagging it needs an
/// allowance per site, and an allowance list longer than the defect list stops
/// being read.
const LANGUAGE_WORDS: &[&str] = &[
    "javascript",
    "typescript",
    "ecmascript",
    "metatable",
    "prototype",
    "int32",
];

/// Whether `word` occurs in `haystack` at the start of a word.
///
/// The boundary is required at the start only. `lua` is not on the list above
/// precisely because this crate's own prose discusses it as the second language,
/// and a word that the design document is *about* cannot also be the tripwire.
fn names_word(haystack: &str, word: &str) -> bool {
    let bytes = haystack.as_bytes();
    haystack
        .match_indices(word)
        .any(|(at, _)| at == 0 || !bytes[at - 1].is_ascii_alphanumeric() && bytes[at - 1] != b'_')
}

fn rust_files(root: &Path, base: &Path, out: &mut Vec<(String, PathBuf)>) {
    for entry in fs::read_dir(root).unwrap_or_else(|e| panic!("read {}: {e}", root.display())) {
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

/// The record names no language, anywhere, including in a comment.
///
/// A failure means a decision in this crate was taken for one client. The next
/// client inherits it without being asked, and the counts it reads back are
/// shaped by a language it is not.
#[test]
fn the_record_names_no_language() {
    let base = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    rust_files(&base.join("src"), base, &mut files);
    assert!(
        !files.is_empty(),
        "the scan found no files, so it would pass by looking in the wrong place"
    );

    let mut found = Vec::new();
    for (rel, path) in &files {
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
        "this crate names a source language in {} place(s), and it may not — it \
         holds counts, and what a count MEANS is a language's own judgement:\n{}",
        found.len(),
        found.join("\n")
    );
}
