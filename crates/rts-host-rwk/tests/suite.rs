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

use rts_host_rwk::compile;

/// Every fixture, run, with the failures named.
///
/// One test rather than one per file, and deliberately: a fixture is a unit of
/// *topic*, not of assertion, and the report below already names both the file
/// and the checks inside it. The alternative — generating a Rust test per file —
/// needs a build script to enumerate them, which is a second place the list of
/// fixtures would live.
#[test]
fn every_fixture_passes() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/suite");
    let mut fixtures = collect(&root);
    fixtures.sort();
    assert!(
        !fixtures.is_empty(),
        "no fixtures found under {}",
        root.display()
    );

    let mut failures: Vec<String> = Vec::new();
    let mut checked = 0usize;

    for path in &fixtures {
        let name = path
            .strip_prefix(&root)
            .unwrap_or(path)
            .display()
            .to_string();
        let source = std::fs::read_to_string(path).expect("a fixture is readable");

        let mut program = match compile(&source) {
            Ok(program) => program,
            // A refusal is a failure. The emitter refuses by name, so the
            // message says which construct — which is exactly the diagnostic a
            // suite should surface rather than swallow.
            Err(error) => {
                failures.push(format!("{name}: did not compile — {error:?}"));
                continue;
            }
        };
        let word = program.run();
        checked += 1;

        match program.described() {
            // The convention: a fixture answers the empty string when every
            // check in it held, and a comma-separated list of names when some
            // did not.
            Some("") => {}
            Some(named) => failures.push(format!("{name}: {named}")),
            // Not a string at all, which means the fixture returned something
            // other than its report — a `return` forgotten, or an early one.
            None => failures.push(format!(
                "{name}: answered a non-string ({word:#x}); a fixture returns its failure list"
            )),
        }
    }

    assert!(
        failures.is_empty(),
        "{} of {} fixtures failed:\n  {}",
        failures.len(),
        checked,
        failures.join("\n  ")
    );
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
