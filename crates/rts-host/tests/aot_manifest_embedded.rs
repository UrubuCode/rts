//! The manifest a compiled program needs now travels inside the `.exe`
//! itself, not only in the `.rtsdata` file beside it.
//!
//! # Why this needs a real binary, and why it is not `#[ignore]`d
//!
//! `rts_host::object::embed_manifest`'s claim is only true of a LINKED
//! program: the object this crate emits carries the manifest as a data
//! symbol, but seeing it survive a real link against `rts-runtime-jit` and
//! answer correctly at run time needs the system linker and the runtime
//! archive `cargo test -p rts-host` builds neither of — the same gap
//! `aot_object.rs` and `aot_embed_compiler.rs` state for their own siblings.
//! `docs/engine/aot-page-scripts.md` and this crate's `README.md` rule 5 both
//! say a test here runs the program; this is that test, for what only a
//! release build has on disk.
//!
//! So this is not `#[ignore]`d: an ignored test needs an extra flag to ever
//! run, which is exactly the kind of thing CLAUDE.md's honesty floor warns
//! stays forgotten. Instead it looks for `target/{release,fast,debug}/rts`
//! and skips itself, loudly, when none exists — which is the ordinary case
//! while iterating (CLAUDE.md's ITERATION SPEED section: `cargo build
//! --release` is a merge-time activity, not something this test performs)
//! and a real check once one does, which is what the coordinator's own
//! `cargo build --release` produces before this runs at the merge gate.
//!
//! # The régua this pins
//!
//! `rts compile tests/aot/claude-pagina-eval.ts X`, delete `X.rtsdata`, run
//! `X.exe` — it must still print `3`. That file's own header has the rest of
//! the claim: `eval("1+2")` inside a page `<script>`, which only works at all
//! because `rts compile` embeds a compiler by default now.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The workspace root, from this crate's own manifest directory rather than
/// the test process's current directory.
///
/// `cargo test` runs a test binary with its OWN package directory as the
/// current directory, not the workspace root — and the fixture this test
/// compiles reads `tests/aot/claude-pagina-eval.html` with `node:fs` at
/// PROGRAM run time, relative to wherever it is invoked from. Both the
/// `rts compile` step and the compiled `.exe` itself are run with this as
/// their `current_dir`, matching the convention the AOT smoke in
/// `.github/workflows/build-artifacts.yml` already uses from a shell (repo
/// root, so `tests/aot/...` and `target/release/rts` are both relative to
/// it).
fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("crates/rts-host/../.. is the workspace root")
}

/// The `rts` CLI binary, searched the same three named profiles
/// `rts_cli::cli::runtime_archive` searches for the runtime archive it
/// links against — not called directly (that function is private to a crate
/// this one does not depend on, and a test shelling out to a real binary is
/// a different claim than one linking the crate that builds it), but the
/// same reasoning applies: `rts.exe` and the archives `rts compile` links
/// are produced by the very same `cargo build` invocation, in the same
/// `target/<profile>` directory, so there is nowhere else to look first.
fn rts_binary(workspace: &Path) -> Option<PathBuf> {
    let name = if cfg!(windows) { "rts.exe" } else { "rts" };
    ["release", "fast", "debug"]
        .into_iter()
        .map(|profile| workspace.join("target").join(profile).join(name))
        .find(|path| path.is_file())
}

#[test]
fn a_moved_exe_still_runs_once_its_sidecar_manifest_is_deleted() {
    let workspace = workspace_root();
    let Some(rts) = rts_binary(&workspace) else {
        eprintln!(
            "skipping a_moved_exe_still_runs_once_its_sidecar_manifest_is_deleted: no `rts` \
             binary under target/{{release,fast,debug}} — this test needs a release build \
             already on disk (`cargo build --release`), a merge-time activity here, not \
             something this test performs itself"
        );
        return;
    };

    let scratch = std::env::temp_dir().join("rts-aot-manifest-embedded");
    std::fs::create_dir_all(&scratch).expect("a scratch directory for this test's own output");
    let output_base = scratch.join("claude_pagina_eval_embedded");
    let exe_path = output_base.with_extension("exe");
    let obj_path = output_base.with_extension("obj");
    let manifest_path = output_base.with_extension("rtsdata");
    // A clean slate — a previous run's leftovers must not be what makes this
    // pass.
    for path in [&exe_path, &obj_path, &manifest_path] {
        let _ = std::fs::remove_file(path);
    }

    let compiled = Command::new(&rts)
        .arg("compile")
        .arg("tests/aot/claude-pagina-eval.ts")
        .arg(&output_base)
        .current_dir(&workspace)
        .output()
        .expect("running `rts compile`");
    assert!(
        compiled.status.success(),
        "`rts compile` failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&compiled.stdout),
        String::from_utf8_lossy(&compiled.stderr)
    );
    assert!(
        manifest_path.is_file(),
        "`rts compile` did not write the .rtsdata sidecar at {} — nothing for this \
         test's own claim to delete",
        manifest_path.display()
    );
    assert!(
        exe_path.is_file(),
        "`rts compile` did not produce {}",
        exe_path.display()
    );

    // The claim this test exists to pin: the sidecar is no longer NEEDED,
    // only still accepted when present. Delete it and run the `.exe` alone.
    std::fs::remove_file(&manifest_path)
        .unwrap_or_else(|error| panic!("delete the sidecar this test just wrote: {error}"));

    let ran = Command::new(&exe_path)
        .current_dir(&workspace)
        .output()
        .unwrap_or_else(|error| {
            panic!(
                "running {} without its .rtsdata sidecar: {error}",
                exe_path.display()
            )
        });
    let stdout = String::from_utf8_lossy(&ran.stdout);
    assert!(
        ran.status.success(),
        "the AOT binary did not run once its .rtsdata sidecar was deleted — the old \
         refusal this batch replaces prints exactly this failure mode:\nstdout: {stdout}\n\
         stderr: {}",
        String::from_utf8_lossy(&ran.stderr)
    );
    assert_eq!(
        stdout.trim(),
        "3",
        "eval(\"1+2\") inside the page's own <script>, run from an .exe moved without its \
         sidecar manifest, should still answer 3 — the manifest travels inside the image now"
    );
}
