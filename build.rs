use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let output = out.join("runtime_support.a");

    // (#runtime-dedup) The AOT runtime archive IS the staticlib that cargo builds
    // for `rts-runtime` (crate-type = ["lib", "staticlib"]). We locate it in the
    // profile output dir and copy it to OUT_DIR so it can be embedded via
    // `include_bytes!` (see src/runtime_objects.rs).
    //
    // This replaces the previous approach where build.rs hand-invoked `rustc
    // --crate-type staticlib` over a SEPARATE `src/namespaces/` copy of the
    // runtime, manually picking every dependency rlib. That copy drifted ~360
    // symbols behind the canonical `crates/rts-runtime` during the cross-runtime
    // push (AOT linking then failed on any newer symbol, e.g.
    // `__RTS_FN_RT_TO_PRIMITIVE`), and the manual rlib selection could not unify
    // the serde / serde_json / json5 ecosystem (many duplicate rlib variants in
    // deps/). Letting cargo build the staticlib gives a single, consistent
    // dependency resolution and one source of truth.
    let profile_dir = profile_dir_from_out_dir(&out).unwrap_or_else(|| {
        panic!(
            "failed to derive the cargo profile dir from OUT_DIR: {}",
            out.display()
        )
    });

    // Staticlib filename is platform-specific: `rts_runtime.lib` (MSVC) or
    // `librts_runtime.a` (gnu / unix).
    let candidates = [
        profile_dir.join("rts_runtime.lib"),
        profile_dir.join("librts_runtime.a"),
    ];
    let staticlib = candidates.iter().find(|p| p.is_file()).unwrap_or_else(|| {
        panic!(
            "rts-runtime staticlib not found in {} (looked for rts_runtime.lib / \
             librts_runtime.a).\nBuild the whole workspace (`cargo build`) so the \
             rts-runtime staticlib crate-type is produced.",
            profile_dir.display()
        )
    });

    std::fs::copy(staticlib, &output).unwrap_or_else(|e| {
        panic!(
            "failed to copy runtime staticlib {} -> {}: {e}",
            staticlib.display(),
            output.display()
        )
    });

    // Strip LLVM bitcode sections so platform linkers don't trip on bitcode
    // embedded in pre-compiled dependency rlibs (no-op on Windows COFF).
    strip_bitcode_from_archive(&output);

    println!("cargo:rerun-if-changed=crates/rts-runtime/src/");
    println!("cargo:rerun-if-changed=crates/rts-abi/src/");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={}", staticlib.display());
}

/// `OUT_DIR` is `<target>/<profile>/build/<pkg>-<hash>/out`; the staticlib lives
/// in `<target>/<profile>/`. Walk up to the `build` component and take its parent.
fn profile_dir_from_out_dir(out_dir: &Path) -> Option<PathBuf> {
    for ancestor in out_dir.ancestors() {
        if ancestor
            .file_name()
            .map(|n| n.to_string_lossy().eq_ignore_ascii_case("build"))
            .unwrap_or(false)
        {
            return ancestor.parent().map(|p| p.to_path_buf());
        }
    }
    None
}

fn strip_bitcode_from_archive(archive: &Path) {
    // macOS: xcrun bitcode_strip handles Mach-O archives natively, no LLVM tools needed.
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

    // Linux: GNU objcopy (binutils) strips ELF sections without any LLVM dependency.
    #[cfg(target_os = "linux")]
    {
        let _ = Command::new("objcopy")
            .args(["--remove-section=.llvmbc", "--remove-section=.llvmcmd"])
            .arg(archive)
            .status();
        return;
    }

    // Windows (COFF): lld-link ignores .llvmbc/.llvmcmd in COFF archives — no stripping needed.
    #[allow(unreachable_code)]
    let _ = archive;
}
