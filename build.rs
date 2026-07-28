use std::path::{Path, PathBuf};
use std::process::Command;

use sha2::{Digest, Sha256};

fn main() {
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let output = out.join("runtime_support.a");
    // Embedded artifacts: zstd-compressed archive + sha256 of the *decompressed*
    // bytes. The compressed blob keeps the shipped `rts` binary small
    // (~99MB -> ~18MB); the sha lets `ensure_artifacts` skip decompression when
    // `~/.rts/artifacts/<host-triple>.a` is already up to date.
    let output_zst = out.join("runtime_support.a.zst");
    let sha_file = out.join("runtime_support.sha256");

    // The embedded archive is the HOST target's runtime; record which triple it
    // was built for so `runtime_objects.rs` can name `artifacts/<triple>.a` and
    // tell host vs. cross targets apart. `TARGET` is the triple `rts` is being
    // built for (== host for a normal native build).
    let host_target = std::env::var("TARGET").unwrap_or_else(|_| "unknown".to_string());
    println!("cargo:rustc-env=RTS_HOST_TARGET={host_target}");

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
    // We embed the `rts-runtime` staticlib. `rts-runtime` depends on
    // `rts-engine`/`rts-primitives`/`rts-shared`/`rts-std`/… and, since the
    // `rts-adapters` crate was folded into it (`src/adapters/`), it now hosts the
    // `__rtsadp_*` value-model trampolines itself too. Cargo bundles the whole
    // dependency rlib closure (every `__RTS_*` extern "C" symbol) INTO this one
    // staticlib — one archive, no merge, no duplicate symbols.
    //
    // `rts-runtime` is a *direct* dependency of the `rts` bin (root Cargo.toml),
    // so Cargo emits its staticlib as part of an ordinary `cargo build` — no
    // separate `-p rts-runtime` pre-step is needed (unlike the old `rts-adapters`
    // crate, which was never a direct target of the bin and required one).
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
            // Compress for embedding + record the decompressed sha256.
            let raw =
                std::fs::read(&output).unwrap_or_else(|e| panic!("read {}: {e}", output.display()));
            std::fs::write(&sha_file, format!("{:x}", Sha256::digest(&raw)))
                .unwrap_or_else(|e| panic!("write {}: {e}", sha_file.display()));
            zstd_to(&raw, &output_zst);
            println!("cargo:rerun-if-changed={}", lib.display());
        }
        None => {
            // Placeholder: no staticlib at compile time. The .zst is never
            // decompressed because the sha sentinel makes ensure_artifacts bail.
            std::fs::write(&sha_file, "PLACEHOLDER")
                .unwrap_or_else(|e| panic!("write placeholder sha {}: {e}", sha_file.display()));
            zstd_to(b"", &output_zst);
            println!(
                "cargo:warning=rts-runtime staticlib not found in {} — embedding a \
                 placeholder. JIT (`rts run`) works; for AOT (`rts compile`) run \
                 `cargo build -p rts-runtime` first, then rebuild.",
                profile_dir.display()
            );
        }
    }

    wire_resident_prelude(&out);

    println!("cargo:rerun-if-changed=crates/rts-runtime/src/");
    println!("cargo:rerun-if-changed=build.rs");
}

/// Step 10, slice 2: OPT-IN embedding of the baked resident-prelude MANIFEST.
///
/// The `rts-prelude-baker` bin (run as a prior build step) writes
/// `prelude_manifest.bin` into a directory named by
/// `RTS_PRELUDE_DIR`. The manifest holds the prelude's PRE-COMPILED machine code
/// (bytes + symbolic relocs) + string blobs; the run path replays it into each
/// run's JIT arena via `define_function_bytes` (no linking, no far calls). When
/// `RTS_PRELUDE_DIR` points at a dir with the manifest, embed it (`src/lib.rs`
/// `include_bytes!`s it); the bin installs it at startup.
///
/// When the var is unset or the manifest is missing (the DEFAULT — every ordinary
/// `cargo build`), embed an EMPTY manifest; the bin installs nothing and the run
/// path keeps the fallback (lower + merge + compile the prelude). Default build
/// unchanged; no object is linked either way.
fn wire_resident_prelude(out: &Path) {
    let manifest_out = out.join("prelude_manifest.bin");

    let dir = std::env::var_os("RTS_PRELUDE_DIR").map(PathBuf::from);
    println!("cargo:rerun-if-env-changed=RTS_PRELUDE_DIR");

    if let Some(dir) = dir {
        let manifest = dir.join("prelude_manifest.bin");
        if manifest.is_file() {
            std::fs::copy(&manifest, &manifest_out)
                .unwrap_or_else(|e| panic!("copy {}: {e}", manifest.display()));
            println!("cargo:rerun-if-changed={}", manifest.display());
            println!(
                "cargo:warning=rts: embedding baked resident prelude manifest from {}",
                dir.display()
            );
            return;
        }
        println!(
            "cargo:warning=RTS_PRELUDE_DIR={} set but prelude_manifest.bin missing — \
             building WITHOUT a resident prelude (run \
             `cargo run -p rts-prelude-baker -- {}` first).",
            dir.display(),
            dir.display()
        );
    }

    // Inert: empty manifest → the bin installs nothing → the run path uses the fallback.
    std::fs::write(&manifest_out, b"")
        .unwrap_or_else(|e| panic!("write empty manifest {}: {e}", manifest_out.display()));
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

fn zstd_to(raw: &[u8], dest: &Path) {
    // Level 19: near-max ratio, still reasonable build-time cost. Decompression
    // speed is level-independent and only runs once (first AOT extraction).
    let zst =
        zstd::encode_all(raw, 19).unwrap_or_else(|e| panic!("zstd-compress runtime archive: {e}"));
    std::fs::write(dest, zst).unwrap_or_else(|e| panic!("write {}: {e}", dest.display()));
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
