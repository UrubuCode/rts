//! Cross-runtime fixture MEASUREMENT (test-only) — split into two `#[ignore]`d
//! harnesses sharing the fixture-collection helpers here:
//!
//! - [`harness`] — the bun cross-runtime comparison (`fixture_harness`): runs each
//!   fixture the new engine accepts and diffs its stdout against `bun`.
//! - [`histogram`] — the bail histogram (`bail_histogram`): categorizes WHY each
//!   bailing fixture bailed, to point the build at the highest-leverage feature.
//!
//! Both are `#[ignore]`d so the normal `cargo test` run does not depend on `bun`
//! or walk the whole corpus. Run explicitly:
//!
//! ```text
//! cargo test -p rts-codegen-new -- --ignored fixture_harness --nocapture
//! cargo test -p rts-codegen-new -- --ignored bail_histogram  --nocapture
//! ```
//!
//! This file holds ONLY the shared fixture-collection helpers so each harness
//! stays well under the 500-line module rule.

use std::path::{Path, PathBuf};

mod harness;
mod histogram;

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

/// Normalize line endings (CRLF→LF) for a fair host-vs-bun comparison.
fn normalize(s: &str) -> String {
    s.replace("\r\n", "\n")
}

/// The fixture path relative to `root`, with forward slashes (for stable output).
fn rel(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}
