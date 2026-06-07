use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let output = out.join("runtime_support.a");

    // The AOT runtime-support archive is now produced by Cargo itself:
    // `rts-runtime` is built with `crate-type = ["rlib", "staticlib"]`, so Cargo
    // bundles every dependency (with correct feature unification) together with
    // the `__RTS_*` extern "C" symbols into one static archive. We just locate
    // that archive and copy it to OUT_DIR for `runtime_objects.rs` to embed.
    //
    // This replaces a hand-rolled `rustc` invocation that re-compiled the
    // namespace tree from a synthetic crate root and picked each dependency rlib
    // by filename-hash heuristics. That heuristic could not disambiguate
    // duplicate dependency variants (e.g. host proc-macro vs. target
    // `serde_core`, or `time` with/without `local-offset`) — it happened to work
    // locally with a single variant but broke on the Windows CI runner with
    // E0277/E0599. Letting Cargo resolve the graph removes the whole class of bug.
    let profile_dir = profile_dir_from_out_dir(&out).unwrap_or_else(|| {
        panic!(
            "failed to derive Cargo profile dir from OUT_DIR: {}",
            out.display()
        )
    });

    // Rust staticlib naming is target-dependent: `librts_runtime.a` on GNU/Unix,
    // `rts_runtime.lib` on MSVC.
    //
    // Cargo only emits a dependency's `staticlib` output when that package is a
    // *direct* build target, not when it is pulled in as an rlib dependency — and
    // even `--workspace` does not order the staticlib before this build script
    // (we only depend on rts-runtime's rlib). So the staticlib must be produced
    // by a prior `cargo build -p rts-runtime`. The AOT archive build is therefore
    // a two-step build (see CLAUDE.md / CI).
    //
    // When the staticlib is missing we DON'T fail the build: the JIT path (`rts
    // run`) never touches the archive, so dev/JIT iteration must keep working.
    // We embed a tiny placeholder instead; the AOT path detects it at runtime and
    // emits a clear "rebuild the runtime archive" error (see runtime_objects.rs).
    let candidates = ["librts_runtime.a", "rts_runtime.lib"];
    let staticlib = candidates
        .iter()
        .map(|n| profile_dir.join(n))
        .find(|p| p.is_file());

    match staticlib {
        Some(lib) => {
            std::fs::copy(&lib, &output).unwrap_or_else(|e| {
                panic!(
                    "failed to copy runtime staticlib {} -> {}: {e}",
                    lib.display(),
                    output.display()
                )
            });
            // Strip LLVM bitcode sections so platform linkers (Apple ld) don't
            // trip on bitcode embedded in pre-compiled dependency rlibs.
            strip_bitcode_from_archive(&output);
            println!("cargo:rerun-if-changed={}", lib.display());
        }
        None => {
            // PLACEHOLDER_MAGIC must match runtime_objects.rs.
            std::fs::write(&output, b"!<arch>\nRTS_PLACEHOLDER").unwrap_or_else(|e| {
                panic!("failed to write placeholder archive {}: {e}", output.display())
            });
            println!(
                "cargo:warning=rts-runtime staticlib not found in {} — embedding a \
                 placeholder. JIT (`rts run`) works; for AOT (`rts compile`) run \
                 `cargo build -p rts-runtime` first, then rebuild.",
                profile_dir.display()
            );
        }
    }

    println!("cargo:rerun-if-changed=crates/rts-runtime/src/");
    println!("cargo:rerun-if-changed=build.rs");
}

/// `OUT_DIR` is `target/<profile>/build/<pkg>-<hash>/out`; the profile dir
/// (`target/<profile>/`, the one holding `build/` and `deps/`, and where Cargo
/// drops staticlib outputs) is the parent of the `build` component.
fn profile_dir_from_out_dir(out_dir: &Path) -> Option<PathBuf> {
    for ancestor in out_dir.ancestors() {
        let name = ancestor.file_name()?.to_string_lossy().to_string();
        if name == "build" {
            return ancestor.parent().map(|p| p.to_path_buf());
        }
    }
    None
}

fn strip_bitcode_from_archive(archive: &Path) {
    // macOS: xcrun bitcode_strip handles Mach-O archives natively.
    #[cfg(target_os = "macos")]
    {
        let tmp = archive.with_extension("tmp");
        let ok = Command::new("xcrun")
            .args(["bitcode_strip", "-r"])
            .arg(archive)
            .arg("-o")
            .arg(&tmp)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if ok {
            let _ = std::fs::rename(&tmp, archive);
        } else {
            let _ = std::fs::remove_file(&tmp);
        }
        return;
    }

    // Linux: GNU objcopy strips ELF sections without any LLVM dependency.
    #[cfg(target_os = "linux")]
    {
        let _ = Command::new("objcopy")
            .args(["--remove-section=.llvmbc", "--remove-section=.llvmcmd"])
            .arg(archive)
            .status();
        return;
    }

    // Windows (COFF): lld-link ignores .llvmbc/.llvmcmd in COFF archives.
    #[allow(unreachable_code)]
    let _ = (archive, Command::new("true"));
}
