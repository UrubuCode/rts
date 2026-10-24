//! WHERE the WPT corpus is, asked in ONE place.
//!
//! Dozens of tests in this crate compare a WPT test against its own reference
//! (`css/css-grid/subgrid/*`, `abspos/*`, …), and each of them named the corpus
//! by an ABSOLUTE path on the machine of whoever wrote it
//! (`C:\Users\…\wpt-corpus\…`). That is not a stale path, it is a test that
//! only one computer can run: on 2026-10-24 **50 of the 1 510 tests of
//! `cargo test -p rts-dom --lib` failed with `NotFound`** on every other
//! machine, the `dom-tests` job of CI among them — and that job is
//! `continue-on-error`, so the red was reported to nobody. A merge gate that is
//! permanently red for everyone is a gate that trains people to ignore it,
//! which is the honesty floor's "a suite that does not run produces nothing to
//! compare".
//!
//! So the corpus is now a LOCATION this module answers, and a test that cannot
//! find it SKIPS and says so instead of failing. The three answers, in order:
//!
//! 1. `$RTS_WPT_CORPUS` — a WPT checkout. How to make one is NOT written
//!    twice: `scripts/wpt_reftests.md` already has the sparse-checkout
//!    procedure for the reftest ruler, and the same checkout serves both. Read
//!    its warning first — the tree is SHARED, and narrowing an existing one
//!    with a shorter `sparse-checkout set` silently deletes folders and makes a
//!    measurement count fewer tests WITHOUT failing. These tests need
//!    `css/css-grid`, `css/support`, `css/reference` and `fonts`, which the
//!    list in that document already covers;
//! 2. `tests/wpt-corpus/` inside this repository, which `.gitignore` keeps out
//!    of it — nothing of the WPT is vendored here (`THIRD-PARTY-NOTICES.md`);
//! 3. the legacy absolute path, so the machine that has always run these keeps
//!    running them with nothing set.
//!
//! Why skipping is not the silent green this repository warns about: the skip
//! PRINTS the reason with the path it looked for, `corpus_is_present` is a test
//! of its own whose whole job is to report the state, and no measurement is
//! ever published from a run that skipped — the number in `tests/css/README.md`
//! comes from the fixture corpus, which is in the tree and always runs.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// The legacy location. Kept so the one machine that already has the corpus
/// needs no environment variable to go on working.
const LEGACY: &str = r"C:/Users/nexga/Documents/wpt-corpus";

/// The corpus root, or `None` when this machine has no corpus.
pub(crate) fn root() -> Option<&'static Path> {
    static ROOT: OnceLock<Option<PathBuf>> = OnceLock::new();
    ROOT.get_or_init(|| {
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/wpt-corpus");
        std::env::var_os("RTS_WPT_CORPUS")
            .map(PathBuf::from)
            .into_iter()
            .chain([repo, PathBuf::from(LEGACY)])
            .find(|p| p.is_dir())
    })
    .as_deref()
}

/// `path("css/css-grid/subgrid/abs-pos-001.html")` under the corpus root — or
/// that same relative path when there is no corpus, so the error a test still
/// gets (one that did not ask first) names the file and not an empty string.
///
/// A BACKSLASH in `rel` is a separator, not a character of the name: the tests
/// were written on Windows and several spell the path `css-grid\alignment\…`.
/// On Linux that is a file whose name contains backslashes, so two of them
/// failed with `NotFound` even with the corpus in place — the only two of the
/// 51 that a corpus did not fix. Normalising here is the one place that keeps
/// the answer the same on both systems.
pub(crate) fn path(rel: &str) -> PathBuf {
    let rel = rel.replace('\\', "/");
    root().map_or_else(|| PathBuf::from(&rel), |r| r.join(&rel))
}

/// Skip the test that calls it when there is no corpus, printing why.
///
/// First statement of every test that reads one, so the file-not-found never
/// happens: `cargo test` prints the note under the test's name with
/// `--nocapture`, and the test counts as passed because it asserted nothing —
/// which is what `corpus_is_present` exists to keep visible.
macro_rules! sem_corpus_wpt {
    () => {
        if $crate::wpt_corpus::root().is_none() {
            eprintln!(
                "SKIP: no WPT corpus. Set RTS_WPT_CORPUS or put a checkout in \
                 tests/wpt-corpus/ (see crates/rts-dom/src/wpt_corpus.rs)."
            );
            return;
        }
    };
}
pub(crate) use sem_corpus_wpt;

/// Does this machine have the corpus? REPORTS, never fails.
///
/// The one test whose job is to say what the others measured against, because
/// "skipped" and "passed" look the same in a count. Run it with `--nocapture`.
#[test]
fn corpus_is_present() {
    match root() {
        Some(r) => eprintln!("WPT corpus: {}", r.display()),
        None => eprintln!(
            "WPT corpus: ABSENT — the tests that read it SKIP. \
             Set RTS_WPT_CORPUS to a wpt checkout to run them."
        ),
    }
}
