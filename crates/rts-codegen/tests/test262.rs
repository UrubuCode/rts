//! Measured against the standard's own tests.
//!
//! # What this measures, exactly
//!
//! **Whether the front end reads each program correctly** — accepts the ones
//! test262 says are valid, and rejects the ones it says are not. Nothing runs,
//! so this is not a pass rate: a program that parses may still be compiled
//! wrongly, and that is a different measurement for a later phase.
//!
//! Being precise about this is the whole point. "94% of test262" would be a
//! sentence people repeat, and it would be false. What is true is narrower and
//! still worth having: a front end that mis-reads a program cannot possibly
//! compile it correctly, so this is the floor everything else stands on.
//!
//! Three outcomes are distinguished, because they mean different things:
//!
//! - **Correct** — a valid program was accepted, or an invalid one rejected.
//! - **Unsupported** — the bridge named a construct it does not lower yet. Our
//!   gap, and a list of them is a work queue.
//! - **Wrong** — a valid program was rejected, or an invalid one accepted. A
//!   defect, and the only category that should ever be zero.
//!
//! # Running it
//!
//! Ignored by default, because it needs the corpus:
//!
//! ```text
//! git clone --depth 1 --filter=blob:none --sparse -c core.longpaths=true \
//!     https://github.com/tc39/test262
//! cd test262 && git sparse-checkout set test/language
//! RTS_TEST262=<path-to-test262> cargo test -p rts-codegen --test test262 -- --ignored --nocapture
//! ```
//!
//! **`core.longpaths=true` is not optional on Windows**, and leaving it out
//! does not fail — it *warns*. Some test262 paths exceed the 260-character
//! limit, the checkout skips them, and everything downstream looks fine. The
//! first run of this harness was done that way: 503 of 24 007 files were
//! missing, and because they were concentrated in `import/import-defer/…`, they
//! were disproportionately files we get wrong. The score read 0.8 points higher
//! than the truth.
//!
//! That is why [`check_checkout_is_complete`] exists. A measurement that
//! quietly measures less than it claims to is worse than no measurement, and
//! the only defence is a check that compares what is on disk against what the
//! repository says should be.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use rts_codegen::names::Names;
use rts_codegen::parse::{Dialect, ParseError, parse_as};
use rts_codegen::syntax::Goal;

/// What test262 says about a file, read from its frontmatter.
#[derive(Default)]
struct Meta {
    /// The file must fail, at this phase.
    negative_phase: Option<String>,
    /// Parse as a module.
    module: bool,
    /// Not a test — an include another test pulls in.
    fixture: bool,
    /// Runs only in strict mode, so the harness must make it strict.
    only_strict: bool,
}

/// Read the `/*--- … ---*/` block.
///
/// Deliberately not a YAML parser. Three fields are needed and all three are
/// simple lines; pulling in a YAML dependency to read them would be a larger
/// commitment than the thing it buys.
fn meta_of(source: &str, path: &Path) -> Meta {
    let mut meta = Meta {
        fixture: path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.ends_with("_FIXTURE.js")),
        ..Default::default()
    };

    let Some(start) = source.find("/*---") else {
        return meta;
    };
    let Some(end) = source[start..].find("---*/") else {
        return meta;
    };
    let block = &source[start..start + end];

    let mut in_negative = false;
    for line in block.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("negative:") {
            in_negative = true;
            continue;
        }
        if in_negative {
            if let Some(phase) = trimmed.strip_prefix("phase:") {
                meta.negative_phase = Some(phase.trim().to_owned());
                continue;
            }
            if !trimmed.starts_with("type:") && !trimmed.is_empty() {
                in_negative = false;
            }
        }

        if let Some(flags) = trimmed.strip_prefix("flags:") {
            meta.module = flags.contains("module");
            meta.only_strict = flags.contains("onlyStrict");
        }
    }

    meta
}

fn collect(directory: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, out);
        } else if path.extension().is_some_and(|e| e == "js") {
            out.push(path);
        }
    }
}

#[derive(Default)]
struct Tally {
    correct: usize,
    unsupported: usize,
    wrongly_rejected: usize,
    wrongly_accepted: usize,
    by_construct: BTreeMap<String, usize>,
    /// A few examples of each defect, so the report is actionable.
    rejection_examples: Vec<String>,
    acceptance_examples: Vec<String>,
    /// Wrongly-accepted files by the area of the corpus they live in.
    ///
    /// Fifteen examples say what the first defect is, alphabetically, and
    /// nothing about how big it is. Every early error we are missing is a rule
    /// stated in one place in the specification, and test262 is laid out by
    /// that structure — so counting by directory is counting by rule, which is
    /// what says which rule to write next.
    acceptance_areas: BTreeMap<String, usize>,
}

/// Refuse to report a score for a corpus that is missing files.
///
/// Asks git what `test/language` should contain and compares it with what was
/// found on disk. A short checkout is not a smaller measurement of the same
/// thing — the files that go missing are the ones with the longest paths, which
/// are the deeply-nested feature directories, so the loss is biased toward
/// exactly the constructs least likely to be handled.
///
/// If git cannot answer — not a checkout, git absent — this says so and lets
/// the run continue. An unverifiable corpus is worth reporting with a caveat;
/// a corpus verified as incomplete is not worth reporting at all.
fn check_checkout_is_complete(root: &Path, found: usize) {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["ls-files", "test/language/**/*.js"])
        .output();

    let Ok(output) = output else {
        println!("note: git not available — corpus completeness unverified");
        return;
    };
    if !output.status.success() {
        println!("note: not a git checkout — corpus completeness unverified");
        return;
    }

    let expected = String::from_utf8_lossy(&output.stdout).lines().count();
    if expected == 0 {
        println!("note: git listed no files — corpus completeness unverified");
        return;
    }

    assert_eq!(
        found,
        expected,
        "the checkout is missing {} of {expected} files.\n\
         On Windows this is almost always the 260-character path limit: re-clone \
         with `-c core.longpaths=true`.\n\
         Reporting a score for a partial corpus would overstate it, because the \
         files that go missing are the deeply-nested ones.",
        expected.saturating_sub(found)
    );
}

#[test]
#[ignore = "needs the test262 corpus; set RTS_TEST262"]
fn the_front_end_reads_test262() {
    let Ok(root) = std::env::var("RTS_TEST262") else {
        panic!("set RTS_TEST262 to a test262 checkout");
    };
    let language = Path::new(&root).join("test").join("language");
    assert!(
        language.is_dir(),
        "{} is not a directory — sparse-checkout test/language",
        language.display()
    );

    let mut files = Vec::new();
    collect(&language, &mut files);
    files.sort();
    assert!(!files.is_empty(), "found no tests");

    check_checkout_is_complete(Path::new(&root), files.len());

    let mut tally = Tally::default();
    let mut considered = 0usize;

    for path in &files {
        let Ok(source) = fs::read_to_string(path) else {
            continue;
        };
        let meta = meta_of(&source, path);
        if meta.fixture {
            continue;
        }
        considered += 1;

        let goal = if meta.module {
            Goal::Module
        } else {
            Goal::Script
        };

        // test262 runs an `onlyStrict` file with a strict prologue prepended.
        // Without it, a test whose whole point is a strict-mode syntax error
        // is handed to a sloppy parse, which correctly accepts it — and the
        // harness records our correct answer as a defect.
        let prepared;
        let text = if meta.only_strict {
            prepared = format!(
                "\"use strict\";
{source}"
            );
            &prepared
        } else {
            &source
        };

        let mut names = Names::new();
        // JavaScript, not TypeScript: the corpus is JavaScript, and TypeScript
        // syntax accepts programs it says must be rejected.
        let result = parse_as(text, goal, Dialect::JavaScript, &mut names);

        let must_fail_to_parse = meta.negative_phase.as_deref() == Some("parse");

        match (&result, must_fail_to_parse) {
            // Correctly read a valid program.
            (Ok(_), false) => tally.correct += 1,

            // Correctly refused an invalid one.
            (Err(ParseError::Syntax(_)), true) => tally.correct += 1,

            // Our gap, named.
            (Err(ParseError::Unsupported { construct, .. }), _) => {
                tally.unsupported += 1;
                *tally
                    .by_construct
                    .entry((*construct).to_owned())
                    .or_default() += 1;
            }

            // A valid program we refused. A defect.
            (Err(ParseError::Syntax(message)), false) => {
                tally.wrongly_rejected += 1;
                if tally.rejection_examples.len() < 12 {
                    tally.rejection_examples.push(format!(
                        "{}: {message}",
                        path.strip_prefix(&language).unwrap_or(path).display()
                    ));
                }
            }

            // An invalid program we accepted. Also a defect, and a worse one:
            // it means something downstream is compiling a program the language
            // says does not exist.
            (Ok(_), true) => {
                tally.wrongly_accepted += 1;
                let relative = path.strip_prefix(&language).unwrap_or(path);
                let area: Vec<_> = relative
                    .components()
                    .take(2)
                    .map(|part| part.as_os_str().to_string_lossy().into_owned())
                    .collect();
                *tally.acceptance_areas.entry(area.join("/")).or_default() += 1;
                if tally.acceptance_examples.len() < 15 {
                    tally.acceptance_examples.push(
                        path.strip_prefix(&language)
                            .unwrap_or(path)
                            .display()
                            .to_string(),
                    );
                }
            }
        }
    }

    let readable = tally.correct + tally.unsupported;
    let rate = 100.0 * tally.correct as f64 / considered as f64;

    println!("\n=== test262 test/language — front end reading ===");
    println!("files considered      {considered}");
    println!("read correctly        {} ({rate:.1}%)", tally.correct);
    println!("refused, named        {}", tally.unsupported);
    println!("wrongly rejected      {}", tally.wrongly_rejected);
    println!("wrongly accepted      {}", tally.wrongly_accepted);
    println!(
        "                      ({} of {considered} produced an answer rather than a defect)",
        readable
    );

    if !tally.by_construct.is_empty() {
        println!("\n--- what the bridge refuses, most first ---");
        let mut ranked: Vec<_> = tally.by_construct.iter().collect();
        ranked.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
        for (construct, count) in ranked.iter().take(20) {
            println!("{count:>6}  {construct}");
        }
    }

    if !tally.acceptance_areas.is_empty() {
        println!("\n--- invalid programs we accepted, by area, biggest first ---");
        let mut ranked: Vec<_> = tally.acceptance_areas.iter().collect();
        ranked.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
        for (area, count) in ranked.iter().take(25) {
            println!("{count:>6}  {area}");
        }
    }

    if !tally.rejection_examples.is_empty() {
        println!("\n--- valid programs we rejected (a defect; first few) ---");
        for example in &tally.rejection_examples {
            println!("  {example}");
        }
    }
    if !tally.acceptance_examples.is_empty() {
        println!(
            "
--- invalid programs we accepted (a defect; first few) ---"
        );
        for example in &tally.acceptance_examples {
            println!("  {example}");
        }
    }
    println!();

    // No threshold asserted. A number that a test can fail on becomes a number
    // people tune, and this one is here to be read.
    assert!(considered > 1000, "the corpus looks truncated");
}

#[test]
fn the_frontmatter_reader_finds_what_it_needs() {
    let negative = r#"/*---
esid: sec-x
negative:
  phase: parse
  type: SyntaxError
flags: [module]
---*/
let x = ;"#;

    let meta = meta_of(negative, Path::new("a.js"));
    assert_eq!(meta.negative_phase.as_deref(), Some("parse"));
    assert!(meta.module);
    assert!(!meta.fixture);

    let runtime_failure = r#"/*---
negative:
  phase: runtime
  type: TypeError
---*/
null.x;"#;
    let meta = meta_of(runtime_failure, Path::new("b.js"));
    assert_eq!(
        meta.negative_phase.as_deref(),
        Some("runtime"),
        "a runtime failure must still parse, so it is not the same expectation"
    );

    let fixture = meta_of("", Path::new("thing_FIXTURE.js"));
    assert!(fixture.fixture, "an include is not a test");
}
