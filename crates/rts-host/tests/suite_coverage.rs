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
//! # Two numbers, and the first is the real one now
//!
//! - **As written.** Every file, unchanged. This is what `rts test` faces. It
//!   was 0 of 818 when this harness was written — an `import` was a syntax
//!   error inside the function body the host wrapped every source in — and it
//!   is 586 of 818 now that a module compiles as one.
//! - **Without the import lines.** The same files with their `import`
//!   statements removed. It was the useful column while the first was zero, and
//!   it is kept because it still separates two different failures: a name the
//!   test surface does not provide, and a construct the emitter does not lower.
//!
//! Neither is a pass rate. Nothing here runs.
//!
//! # Running it
//!
//! Ignored by default: it walks 818 files and takes seconds rather than
//! milliseconds, which is not what the rest of this crate's tests are.
//!
//! ```text
//! cargo test -p rts-host --test suite_coverage -- --ignored --nocapture
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
    /// A name was used and nothing introduced it, and which one.
    Unbound(String),
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
    match rts_host::compile(source) {
        Ok(_) => Outcome::Compiled,
        Err(error) => {
            let text = format!("{error:?}");
            if let Some(at) = text.find("Unsupported { construct: \"") {
                let rest = &text[at + "Unsupported { construct: \"".len()..];
                let end = rest.find('"').unwrap_or(rest.len());
                return Outcome::Unsupported(rest[..end].to_owned());
            }
            if let Some(at) = text.find("Unbound(\"") {
                // The NAME, not just the fact: it is the largest bucket and
                // useless without saying which, which is what made the first
                // version of this measurement unable to rank the work behind it.
                let rest = &text[at + "Unbound(\"".len()..];
                let end = rest.find('"').unwrap_or(rest.len());
                return Outcome::Unbound(rest[..end].to_owned());
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
            Outcome::Unbound(name) => {
                *reasons.entry(format!("<unbound> {name}")).or_default() += 1;
            }
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

#[test]
fn a_module_binds_what_it_imports() {
    // The smallest program the corpus's first two lines are: an import, then a
    // call of what it bound. If this refuses, every file in the suite does.
    let source = "import { describe } from \"rts:test\";\ndescribe(\"a\", function () { return 1; });\n";
    rts_host::compile(source).expect("a module that imports what it calls");
}

#[test]
fn the_shape_the_corpus_actually_writes() {
    let source = "import { describe, test, expect } from \"rts:test\";\nimport { io } from \"rts\";\n\nlet captured: string = \"\";\nfunction print(value: string): void { captured += value; }\n\ndescribe(\"a\", function () { test(\"b\", function () { expect(1).toBe(1); }); });\n";
    rts_host::compile(source).expect("what every file in the suite starts with");
}

/// Every `.ts` under `tests/`, not only the `.test.ts` at its top level.
///
/// # Why a second measurement rather than a wider first one
///
/// Because they answer different questions and one of them was going
/// unanswered. [`what_the_new_engine_compiles_of_the_suite`] reads the 818
/// top-level `.test.ts` — the harness corpus, the files `rts test` runs — and
/// that is the number this crate's documentation quotes. Underneath them sit
/// another seven hundred: `tests/cross-runtime/` and its siblings, driven by
/// `examples/run_fixture.rs` in a separate process.
///
/// Nearly half the corpus was therefore outside the only committed measurement,
/// and it showed. A change that took the top-level number from 673 to 673 moved
/// this one by twelve; a change that broke 739 files here was invisible there
/// until someone happened to look. Both were measured with a throwaway probe
/// written from scratch each time, which is exactly the shape this repository's
/// rule about coverage exists to prevent — a number produced by running
/// something, stated with what produced it, rather than by a script that does
/// not survive the session that needed it.
///
/// Ignored by default like its neighbour: it compiles fifteen hundred files.
#[test]
#[ignore = "walks every .ts under tests/; run with --ignored --nocapture"]
fn what_the_new_engine_compiles_of_every_file() {
    let mut files: Vec<(PathBuf, String)> = Vec::new();
    collect(&suite(), &mut files);

    // The same guard the top-level measurement carries, for the same reason: a
    // corpus that quietly shrank is a number measured against less than it
    // claims.
    assert!(
        files.len() > 1400,
        "the tree holds about 1542 .ts files; {} were read, so this would report \
         a score for a corpus smaller than the one being claimed",
        files.len()
    );

    let total = files.len();
    let (compiled, reasons) = measure(&files, false);
    report("Every .ts under tests/", total, compiled, &reasons);
}

/// Every `.ts` under a directory, walked.
///
/// Recursive where the top-level measurement is not, which is the whole
/// difference between the two.
fn collect(at: &Path, into: &mut Vec<(PathBuf, String)>) {
    let Ok(entries) = fs::read_dir(at) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, into);
            continue;
        }
        if path.extension().is_some_and(|kind| kind == "ts")
            && let Ok(source) = fs::read_to_string(&path)
        {
            into.push((path, source));
        }
    }
}
