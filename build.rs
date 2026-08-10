use std::path::{Path, PathBuf};
use std::process::Command;

use sha2::{Digest, Sha256};

fn main() {
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());

    // The embedded archive is the HOST target's runtime; record which triple it
    // was built for so `runtime_objects.rs` can name `artifacts/<triple>.a`
    // and tell host vs. cross targets apart. `TARGET` is the triple `rts` is
    // being built for (== host for a normal native build).
    let host_target = std::env::var("TARGET").unwrap_or_else(|_| "unknown".to_string());
    println!("cargo:rustc-env=RTS_HOST_TARGET={host_target}");

    let profile_dir = profile_dir_from_out_dir(&out).unwrap_or_else(|| {
        panic!(
            "failed to derive Cargo profile dir from OUT_DIR: {}",
            out.display()
        )
    });

    // The OLD engine's archive was embedded here too, from `rts-runtime`'s
    // staticlib: ~18 MB in every shipped binary and a level-19 compression of
    // ~99 MB on every build that touched the runtime. Nothing read it — `rts
    // compile` links the new engine's archive — so it went with the engine.
    embed_runtime_archive(&out, &profile_dir);

    println!("cargo:rerun-if-changed=build.rs");
}
/// Embeds the AOT runtime archive (`rts-runtime`, over `rts-core` +
/// `rts-std` + `rts-node`) into the binary, compressed.
///
/// It never fails the build: the JIT paths (`rts run`, `rts test`) never touch
/// this archive, so a missing `rts-runtime`
/// staticlib (e.g. mid-refactor, or a partial checkout) embeds a placeholder
/// instead of blocking every `cargo build`.
fn embed_runtime_archive(out: &Path, profile_dir: &Path) {
    let output = out.join("runtime_support.a");
    let output_zst = out.join("runtime_support.a.zst");
    let sha_file = out.join("runtime_support.sha256");

    // Same MSVC-vs-GNU staticlib naming split as the old engine's archive.
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
            strip_bitcode_from_archive(&output);
            let raw =
                std::fs::read(&output).unwrap_or_else(|e| panic!("read {}: {e}", output.display()));
            std::fs::write(&sha_file, format!("{:x}", Sha256::digest(&raw)))
                .unwrap_or_else(|e| panic!("write {}: {e}", sha_file.display()));
            zstd_to(&raw, &output_zst);
            println!("cargo:rerun-if-changed={}", lib.display());
        }
        None => {
            std::fs::write(&sha_file, "PLACEHOLDER").unwrap_or_else(|e| {
                panic!("write placeholder sha {}: {e}", sha_file.display())
            });
            zstd_to(b"", &output_zst);
            println!(
                "cargo:warning=rts-runtime staticlib not found in {} — embedding a \
                 placeholder. `rts run`/`rts test` (new engine JIT) work; for `rts compile` \
                 (new engine AOT) this should not happen from a normal build, since \
                 rts-runtime is now a direct dependency of the `rts` bin.",
                profile_dir.display()
            );
        }
    }

    println!("cargo:rerun-if-changed=crates/rts-runtime/src/");
    println!("cargo:rerun-if-changed=crates/rts-core/src/");
    println!("cargo:rerun-if-changed=crates/rts-std/src/");
    println!("cargo:rerun-if-changed=crates/rts-node/src/");
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
