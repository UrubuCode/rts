//! Mini cross-runtime harness — the FIRST real measurement of the new engine on
//! actual fixtures.
//!
//! Honest and subset-only: it walks `tests/cross-runtime/**/*.ts`, runs each
//! through [`super::run_source`], and — for the ones the new engine actually
//! runs — compares the captured stdout to `bun <file>`. Most fixtures are
//! Unsupported today (objects, strings methods, classes, regex, …); the harness
//! reports the REAL count it runs and matches, never inflates it.
//!
//! It is `#[ignore]`d by default so the normal `cargo test` run does NOT depend
//! on `bun` being installed. Run it explicitly with:
//!
//! ```text
//! cargo test -p rts-codegen-new -- --ignored fixture_harness --nocapture
//! ```

use std::path::{Path, PathBuf};
use std::process::Command;

use super::run_source;

/// Root of the committed cross-runtime fixtures, resolved from this crate.
fn fixtures_root() -> PathBuf {
    // CARGO_MANIFEST_DIR = .../crates/rts-codegen-new
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    crate_dir.join("../../tests/cross-runtime")
}

/// Collect every `*.ts` fixture under `root`, skipping `node_modules` and the
/// shared `support/` helpers (which are imported, not run standalone).
fn collect_fixtures(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if name == "node_modules" || name == "support" {
                    continue;
                }
                stack.push(path);
            } else if path.extension().and_then(|e| e.to_str()) == Some("ts") {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

/// Run a fixture through `bun` and return its stdout (None if bun fails/missing).
fn bun_stdout(path: &Path) -> Option<String> {
    let output = Command::new("bun").arg("run").arg(path).output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(normalize(&String::from_utf8_lossy(&output.stdout)))
}

/// Normalize line endings (CRLF→LF) for a fair host-vs-bun comparison.
fn normalize(s: &str) -> String {
    s.replace("\r\n", "\n")
}

#[derive(Default)]
struct Tally {
    total: usize,
    ran: usize,        // run_source returned Ok
    matched: usize,    // ran AND equals bun
    diverged: usize,   // ran but != bun
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
        match run_source(&src) {
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

fn rel(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}
