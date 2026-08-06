//! What fraction of the repository's own test suite the new engine compiles.
//!
//! # What this measures, exactly
//!
//! Whether `rts-codegen` **emits** each of the 818 files in `tests/`, not
//! whether running one passes. Nothing is executed here, so this is not the
//! score `rts test` reports and calling it one would be false. It is the floor
//! underneath that score: a file the engine cannot compile cannot pass.
//!
//! It exists because the question "what does the new engine score on the suite"
//! has a blocking answer nobody had written down — every file begins with
//! `import { test, expect } from "rts:test"`, and the host compiles a function
//! body rather than a module. So the honest first number is not "how many pass"
//! but "what stops them", ranked.
//!
//! # Two numbers, and the second is the useful one
//!
//! - **As written.** Every file, unchanged. This is what `rts test` would face.
//! - **Without the import lines.** The same files with their `import`
//!   statements removed, which is not a fix and does not run — a body calling
//!   `test(…)` reads a name nothing introduced. What it does is see PAST the
//!   module gap to the constructs behind it, so the work queue is ranked by
//!   what is actually in the way rather than by the first thing hit.
//!
//! Both are reported. Quoting the second as a coverage figure would be the
//! measurement wearing clothes it did not earn.
//!
//! # Running it
//!
//! Ignored by default: it walks 818 files and takes seconds rather than
//! milliseconds, which is not what the rest of this crate's tests are.
//!
//! ```text
//! cargo test -p rts-host-rwk --test suite_coverage -- --ignored --nocapture
//! ```

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Where the suite lives, from this crate rather than from the caller's
/// directory: `cargo test` runs with the crate root as the working directory,
/// and a relative path would find a different corpus depending on where it was
/// invoked — which is how a number gets measured against less than it claims.
fn suite() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
}

/// What happened to one file.
enum Outcome {
    /// It compiled. Nothing was run.
    Compiled,
    /// The emitter named a construct it does not lower.
    Unsupported(String),
    /// A name was used and nothing introduced it.
    Unbound,
    /// The front end refused to read it.
    Refused,
    /// The emitter built something the machine rejected — a defect here.
    Build(String),
}

/// Compiles one source and says what happened.
///
/// Every kind is kept apart because they mean different things: a gap is a work
/// queue, a build error is a bug in the emitter, and a name that is unbound is
/// mostly the import line having been removed.
fn attempt(source: &str) -> Outcome {
    match rts_host_rwk::compile(source) {
        Ok(_) => Outcome::Compiled,
        Err(error) => {
            let text = format!("{error:?}");
            if let Some(at) = text.find("Unsupported { construct: \"") {
                let rest = &text[at + "Unsupported { construct: \"".len()..];
                let end = rest.find('"').unwrap_or(rest.len());
                return Outcome::Unsupported(rest[..end].to_owned());
            }
            if text.contains("UnboundName") {
                return Outcome::Unbound;
            }
            if text.contains("Build(") {
                return Outcome::Build(text.chars().take(120).collect());
            }
            Outcome::Refused
        }
    }
}

/// The same file with its `import` statements dropped.
///
/// Line-wise and deliberately crude: this is not a module implementation and
/// must not look like one. It removes the first thing every file hits so the
/// measurement can see what is behind it.
fn without_imports(source: &str) -> String {
    source
        .lines()
        .filter(|line| !line.trim_start().starts_with("import "))
        .collect::<Vec<_>>()
        .join("\n")
}

/// One pass over the corpus.
fn measure(files: &[(PathBuf, String)], strip: bool) -> (usize, BTreeMap<String, usize>) {
    let mut compiled = 0;
    let mut reasons: BTreeMap<String, usize> = BTreeMap::new();
    for (_, source) in files {
        let source = if strip {
            without_imports(source)
        } else {
            source.clone()
        };
        match attempt(&source) {
            Outcome::Compiled => compiled += 1,
            Outcome::Unsupported(what) => *reasons.entry(what).or_default() += 1,
            Outcome::Unbound => *reasons.entry("<a name nothing introduced>".into()).or_default() += 1,
            Outcome::Refused => *reasons.entry("<the front end refused it>".into()).or_default() += 1,
            Outcome::Build(what) => *reasons.entry(format!("<BUILD> {what}")).or_default() += 1,
        }
    }
    (compiled, reasons)
}

/// Reports one pass, worst first — a work queue is read from the top.
fn report(title: &str, total: usize, compiled: usize, reasons: &BTreeMap<String, usize>) {
    let share = compiled as f64 / total as f64 * 100.0;
    println!("\n{title}: {compiled}/{total} compiled ({share:.1} %)");
    let mut ranked: Vec<_> = reasons.iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    for (what, count) in ranked.iter().take(25) {
        println!("  {count:>5}  {what}");
    }
}

#[test]
#[ignore = "walks the whole suite; run with --ignored --nocapture"]
fn what_the_new_engine_compiles_of_the_suite() {
    let mut files: Vec<(PathBuf, String)> = Vec::new();
    let directory = suite();
    for entry in fs::read_dir(&directory).expect("the suite directory") {
        let path = entry.expect("a directory entry").path();
        let is_test = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(".test.ts"));
        if !is_test {
            continue;
        }
        let source = fs::read_to_string(&path).expect("a readable test");
        files.push((path, source));
    }
    // A corpus that quietly shrank is a number measured against less than it
    // claims — the failure `test262.rs` records paying for once already.
    assert!(
        files.len() > 700,
        "the suite has 818 files; {} were read, so this would report a score \
         for a corpus smaller than the one being claimed",
        files.len()
    );

    let total = files.len();
    let (as_written, written_reasons) = measure(&files, false);
    let (stripped, stripped_reasons) = measure(&files, true);

    report("As written", total, as_written, &written_reasons);
    report(
        "With the import lines removed (NOT a score — see the module docs)",
        total,
        stripped,
        &stripped_reasons,
    );
}
