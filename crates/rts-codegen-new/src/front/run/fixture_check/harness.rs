//! Mini cross-runtime harness — the FIRST real measurement of the new engine on
//! actual fixtures.
//!
//! Honest and subset-only: it walks `tests/cross-runtime/**/*.ts`, runs each
//! through [`super::super::render_source`] (console.log captured to a `String` via
//! the adapter's real-pool-backed sink), and — for the ones the new engine
//! actually runs — compares the captured stdout to `bun <file>`. Most fixtures are
//! Unsupported today (objects, regex, classes, …); the harness reports the REAL
//! count it runs and matches, never inflates it.
//!
//! It is `#[ignore]`d by default so the normal `cargo test` run does NOT depend on
//! `bun` being installed. Run it explicitly with:
//!
//! ```text
//! cargo test -p rts-codegen-new -- --ignored fixture_harness --nocapture
//! ```

use std::path::Path;
use std::process::Command;

use super::super::render_source;
use super::{collect_fixtures, fixtures_root, normalize, rel};

/// Run a fixture through `bun` and return its stdout (None if bun fails/missing).
fn bun_stdout(path: &Path) -> Option<String> {
    let output = Command::new("bun").arg("run").arg(path).output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(normalize(&String::from_utf8_lossy(&output.stdout)))
}

#[derive(Default)]
struct Tally {
    total: usize,
    ran: usize,         // run_source returned Ok
    matched: usize,     // ran AND equals bun
    diverged: usize,    // ran but != bun
    unsupported: usize, // run_source bailed
    bun_unavailable: usize,
}

#[test]
#[ignore = "shells out to `bun`; run explicitly with --ignored"]
fn fixture_harness() {
    let root = fixtures_root();
    if !root.is_dir() {
        eprintln!("cross-runtime fixtures not found at {}", root.display());
        return;
    }
    // Probe bun once; if unavailable, report and stop (the test is ignored by
    // default, so this only fires on an explicit, bun-less run).
    if Command::new("bun").arg("--version").output().is_err() {
        eprintln!("`bun` not on PATH — skipping the cross-runtime comparison.");
        return;
    }

    let fixtures = collect_fixtures(&root);
    let mut t = Tally::default();
    let mut matched_names: Vec<String> = Vec::new();
    let mut diverged_names: Vec<(String, String, String)> = Vec::new();

    for path in &fixtures {
        t.total += 1;
        let src = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(_) => continue,
        };
        // Guard against runaway loops in a fixture: each run is in-process, so a
        // genuinely infinite program would hang. We accept that risk for the
        // ignored harness (the subset the engine runs is loop-bounded in
        // practice); a timeout wrapper is a follow-up.
        match render_source(&src) {
            Ok(out) => {
                t.ran += 1;
                match bun_stdout(path) {
                    Some(expected) => {
                        if normalize(&out) == expected {
                            t.matched += 1;
                            matched_names.push(rel(&root, path));
                        } else {
                            t.diverged += 1;
                            diverged_names.push((rel(&root, path), normalize(&out), expected));
                        }
                    }
                    None => t.bun_unavailable += 1,
                }
            }
            Err(_) => t.unsupported += 1,
        }
    }

    eprintln!("\n=== new-engine cross-runtime harness (honest, subset-only) ===");
    eprintln!("fixtures scanned : {}", t.total);
    eprintln!("ran (Ok)         : {}", t.ran);
    eprintln!("  matched bun    : {}", t.matched);
    eprintln!("  diverged       : {}", t.diverged);
    eprintln!("  bun unavailable: {}", t.bun_unavailable);
    eprintln!("unsupported (bail): {}", t.unsupported);
    eprintln!("\nmatched fixtures:");
    for n in &matched_names {
        eprintln!("  ✓ {n}");
    }
    eprintln!("\ndiverged fixtures (new-engine vs bun):");
    for (n, got, want) in &diverged_names {
        eprintln!("  ✗ {n}\n     got : {got:?}\n     bun : {want:?}");
    }
}
