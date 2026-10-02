//! The conformance suite: every fixture in `tests/suite/`, actually run.
//!
//! # Why a fixture is JavaScript that names its own failures
//!
//! `running.rs` asserts from Rust, one `assert_eq!` per behaviour, and that is
//! the right shape for a behaviour whose *encoding* matters — a boolean has to
//! come back as `TAG_BOOL`, and only Rust can check that.
//!
//! It is the wrong shape for coverage. A hundred assertions about `Array`
//! semantics do not each want a Rust function around them, and writing them in
//! Rust means every one is a quoted string with its escapes doubled. So a
//! fixture is a `.js` file, it checks itself, and it answers **the names of what
//! failed** — which is what makes a failure report say `flat-depth` rather than
//! `assertion 47`.
//!
//! # Why the answer is a string and not a count
//!
//! Because a count says a fixture is broken and a name says which line to read.
//! The cost is that the host has to read text out of a heap that is gone by the
//! time `run` returns, which is why `Compiled::described` exists.
//!
//! # What a fixture may not use
//!
//! The compiler refuses a long list by name, and a fixture is subject to all of
//! it: no `async`/`await`, no generators, no destructuring anywhere, no optional
//! chaining, no default parameters, no spread in an object literal, no `this`
//! inside an arrow, and no function of more than four parameters. The host wraps
//! the source in a function, so a fixture `return`s rather than exporting.
//!
//! A fixture that fails to COMPILE is a failure, not a skip. A suite that
//! quietly skipped what it could not build would report a number about the
//! subset it happened to like, which is the failure mode the honesty floor names
//! by name.

use std::path::{Path, PathBuf};
use std::process::Command;

use rts_host::compile;

/// The fixture a re-executed copy of this binary is to run.
///
/// A path and not an index, so the child's job does not depend on it and the
/// parent enumerating the directory the same way.
const ONE: &str = "RTS_SUITE_ONE";

/// What the child prints when the fixture answered its report.
const REPORT: &str = "[suite] report ";
/// What the child prints when the fixture did not compile.
const REFUSED: &str = "[suite] refused ";
/// What the child prints when the fixture answered something that is not a
/// report at all.
const NOT_A_REPORT: &str = "[suite] not-a-report ";

/// Every fixture, run, with the failures named.
///
/// One test rather than one per file, and deliberately: a fixture is a unit of
/// *topic*, not of assertion, and the report below already names both the file
/// and the checks inside it. The alternative — generating a Rust test per file —
/// needs a build script to enumerate them, which is a second place the list of
/// fixtures would live.
///
/// # Why each fixture is its own PROCESS
///
/// Because an uncaught exception takes the process with it, and this test ran
/// every fixture in one. `collections.js` wrote `for (let v of new WeakSet())`
/// and asserted that the body never ran — Node and Bun both raise `TypeError`
/// there, so the assertion described a third behaviour that is neither of
/// theirs — and the day this engine started raising, the uncaught throw killed
/// the run at the FOURTH of nineteen files. The twelve after it never ran, and
/// the output said only "test failed": a suite that does not run produces
/// nothing to compare, and empty looks exactly like green at the place anyone
/// looks.
///
/// `rts test` and `examples/suite_run.rs` are one process per file for this
/// exact reason, stated in their own comments. This is that rule, applied to
/// the one runner that had not taken it.
///
/// The child half is this same test binary re-executed with [`ONE`] set, which
/// is `exhaustion.rs`'s pattern in this crate and needs no second target.
#[test]
fn every_fixture_passes() {
    if let Ok(one) = std::env::var(ONE) {
        run_one(Path::new(&one));
        return;
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/suite");
    let mut fixtures = collect(&root);
    fixtures.sort();
    assert!(
        !fixtures.is_empty(),
        "no fixtures found under {}",
        root.display()
    );

    let exe = std::env::current_exe().expect("this test binary");
    let mut failures: Vec<String> = Vec::new();
    let mut checked = 0usize;

    for path in &fixtures {
        let name = path
            .strip_prefix(&root)
            .unwrap_or(path)
            .display()
            .to_string();
        let child = Command::new(&exe)
            .arg("every_fixture_passes")
            .arg("--exact")
            .arg("--nocapture")
            .env(ONE, path)
            .output()
            .expect("re-executing this test binary");
        let out = String::from_utf8_lossy(&child.stdout);
        let err = String::from_utf8_lossy(&child.stderr);

        checked += 1;
        if let Some(named) = line_after(&out, REPORT) {
            // The convention: a fixture answers the empty string when every
            // check in it held, and a comma-separated list of names when some
            // did not.
            if !named.is_empty() {
                failures.push(format!("{name}: {named}"));
            }
            continue;
        }
        // A refusal is a failure. The emitter refuses by name, so the message
        // says which construct — exactly the diagnostic a suite should surface
        // rather than swallow.
        if let Some(reason) = line_after(&out, REFUSED) {
            failures.push(format!("{name}: did not compile — {reason}"));
            continue;
        }
        // Answered something that is not a report — a `return` forgotten, or an
        // early one.
        if let Some(word) = line_after(&out, NOT_A_REPORT) {
            failures.push(format!(
                "{name}: answered a non-string ({word}); a fixture returns its failure list"
            ));
            continue;
        }
        // No line at all: the child DIED. This is the case the single-process
        // version could not report, because it was the same death. `stderr`
        // carries the engine's own last word, which is the whole diagnostic, so
        // it is quoted rather than summarised.
        let last = err.trim().lines().last().unwrap_or("").trim().to_owned();
        failures.push(format!(
            "{name}: the process did not survive the fixture ({}){}",
            child
                .status
                .code()
                .map(|code| code.to_string())
                .unwrap_or_else(|| "signal".to_owned()),
            match last.is_empty() {
                true => String::new(),
                false => format!(" — {last}"),
            }
        ));
    }

    assert!(
        failures.is_empty(),
        "{} of {} fixtures failed:\n  {}",
        failures.len(),
        checked,
        failures.join("\n  ")
    );
}

/// The child half: compile and run ONE fixture, and print what it answered.
///
/// Printed rather than asserted, because the parent is what decides whether an
/// answer is a failure — and because a panic here would reach the parent as a
/// dead child, which is the one outcome that has to stay reserved for a fixture
/// that genuinely killed the process.
fn run_one(path: &Path) {
    let source = std::fs::read_to_string(path).expect("a fixture is readable");
    let mut program = match compile(&source) {
        Ok(program) => program,
        Err(error) => {
            println!("{REFUSED}{error:?}");
            return;
        }
    };
    let word = program.run();
    match program.described() {
        Some(named) => println!("{REPORT}{named}"),
        None => println!("{NOT_A_REPORT}{word:#x}"),
    }
}

/// The rest of the first line that starts with `marker`.
///
/// A whole line and not a `contains`, so a fixture whose own report happens to
/// hold the marker text cannot be read as the runner's own output.
fn line_after(text: &str, marker: &str) -> Option<String> {
    text.lines()
        .find(|line| line.starts_with(marker))
        .map(|line| line[marker.len()..].trim_end().to_owned())
}

/// Every `.js` file under a directory, at any depth.
fn collect(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(root) else {
        return found;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            found.extend(collect(&path));
        } else if path.extension().is_some_and(|kind| kind == "js") {
            found.push(path);
        }
    }
    found
}
