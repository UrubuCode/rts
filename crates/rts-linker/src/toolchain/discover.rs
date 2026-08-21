//! System-linker discovery: cache dirs, PATH, rustup/sysroot, then download.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Result, bail};

use super::download::{
    LINKER_DOWNLOAD_URL_ENV_VAR, RUST_LLD_TOOL_NAME, maybe_download_linker,
    maybe_download_rust_dist_linker,
};
use super::paths::{
    expected_binary_name, find_binary_in_dir, find_binary_in_path, resolve_toolchains_base_dir,
    sanitize_tool_dir_name,
};
use super::target::{ResolvedLinker, TargetFlavor, ToolchainLayout};

pub fn resolve_linker(layout: &ToolchainLayout) -> Result<ResolvedLinker> {
    let candidates = preferred_linker_names(layout.target.flavor);
    let toolchains_base = resolve_toolchains_base_dir()?;

    for candidate in candidates {
        if let Some(path) = find_binary_in_dir(&layout.bin_dir, candidate) {
            return Ok(ResolvedLinker { path });
        }
    }

    for candidate in candidates {
        for dir in
            tool_cache_search_dirs(&toolchains_base, RUST_LLD_TOOL_NAME, &layout.target.triple)
        {
            if let Some(path) = find_binary_in_dir(&dir, candidate) {
                return Ok(ResolvedLinker { path });
            }
        }
    }

    for candidate in candidates {
        for dir in tool_cache_search_dirs(
            &toolchains_base,
            sanitize_tool_dir_name(candidate).as_str(),
            &layout.target.triple,
        ) {
            if let Some(path) = find_binary_in_dir(&dir, candidate) {
                return Ok(ResolvedLinker { path });
            }
        }
    }

    if let Some(path) = find_linker_near_current_exe(candidates) {
        return Ok(ResolvedLinker { path });
    }

    // For COFF/MachO, check rust-lld from the local Rust toolchain BEFORE PATH.
    // Apple ld ships LLVM 17 and VS lld-link ships LLVM 19; both reject LLVM 22
    // bitcode embedded in pre-compiled dependency rlibs (regex, memchr, …).
    // rust-lld from rustup shares the same LLVM version as the compiler, so it
    // handles the bitcode cleanly.
    if matches!(layout.target.flavor, TargetFlavor::Coff | TargetFlavor::MachO) {
        if let Some(path) = rustup_rust_lld() {
            return Ok(ResolvedLinker { path });
        }
        if let Some(path) = rustc_sysroot_rust_lld(layout) {
            return Ok(ResolvedLinker { path });
        }
    }

    for candidate in candidates {
        if let Some(path) = find_binary_in_path(candidate) {
            return Ok(ResolvedLinker { path });
        }
    }

    if let Some(path) = rustup_rust_lld() {
        return Ok(ResolvedLinker { path });
    }

    if let Some(path) = rustc_sysroot_rust_lld(layout) {
        return Ok(ResolvedLinker { path });
    }

    if let Some(primary) = candidates.first().copied() {
        if let Some(path) = maybe_download_linker(layout, primary, &toolchains_base)? {
            eprintln!(
                "RTS toolchain: cached target '{}' linker at {}",
                layout.target.triple,
                path.display()
            );
            return Ok(ResolvedLinker { path });
        }
    }

    if let Some(path) = maybe_download_rust_dist_linker(layout, &toolchains_base)? {
        eprintln!(
            "RTS toolchain: cached target '{}' linker at {}",
            layout.target.triple,
            path.display()
        );
        return Ok(ResolvedLinker { path });
    }

    bail!(
        "no system linker found for target '{}' (searched in {}, ~/.rts/toolchains/rust-lld, ~/.rts/toolchains/<tool>, PATH, rustup/sysroot, optional download via {}, and Rust dist)",
        layout.target.triple,
        layout.bin_dir.display(),
        LINKER_DOWNLOAD_URL_ENV_VAR
    )
}

fn find_linker_near_current_exe(candidates: &[&str]) -> Option<PathBuf> {
    let current_exe = std::env::current_exe().ok()?;
    let bin_dir = current_exe.parent()?;

    for candidate in candidates {
        if let Some(path) = find_binary_in_dir(bin_dir, candidate) {
            return Some(path);
        }
    }

    None
}

fn preferred_linker_names(flavor: TargetFlavor) -> &'static [&'static str] {
    match flavor {
        TargetFlavor::Coff => &["lld-link", "rust-lld", "link"],
        // Prefer system linker drivers over rust-lld: rust-lld is a raw linker
        // that doesn't add implicit libc/libstdc++ and can crash on ObjC stubs.
        TargetFlavor::Elf => &["ld.lld", "clang", "cc", "lld", "rust-lld"],
        TargetFlavor::MachO => &["ld64.lld", "ld", "clang", "cc", "rust-lld"],
    }
}

fn tool_cache_search_dirs(base: &Path, tool_name: &str, target: &str) -> Vec<PathBuf> {
    let normalized_tool = sanitize_tool_dir_name(tool_name);
    let tool_root = base.join(&normalized_tool);
    vec![
        tool_root.clone(),
        tool_root.join(target),
        tool_root.join(target).join("bin"),
    ]
}

fn rustup_rust_lld() -> Option<PathBuf> {
    let output = Command::new("rustup")
        .args(["which", "rust-lld"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path.is_empty() {
        return None;
    }

    let candidate = PathBuf::from(path);
    (candidate.is_file() && can_launch(&candidate)).then_some(candidate)
}

/// Whether a discovered linker can actually be started.
///
/// # Why existing on disk is not enough
///
/// `rust-lld` in a rustup toolchain is dynamically linked against
/// `@rpath/libLLVM.dylib`, and on 2026-08-20 the `macos-latest` runner shipped
/// a toolchain where that dylib is not present. The binary is there, it is
/// executable, and it dies in `dyld` before `main`. Everything above answered
/// `is_file()` and handed it back, so `rts compile` chose a linker that cannot
/// run and the AOT smoke test failed on every macOS build for three days —
/// with `release`, `cross-runtime`, `ts-suite` and the whole Benchmarks
/// workflow skipped behind it.
///
/// # What counts as "can be started", and why it is the exit STATUS
///
/// Not the exit code's value: a linker invoked with `--version` and nothing to
/// link may answer non-zero for reasons that have nothing to do with whether it
/// works. What separates the two cases is whether the process exited AT ALL.
/// A `dyld` failure kills it with a signal, so `status.code()` is `None` —
/// which is exactly what the failing runner reported: *"failed for target
/// aarch64-apple-darwin (status=None, stdout='', stderr='dyld[45755]: Library
/// not loaded'"*.
///
/// # Why this does not weaken the rust-lld preference
///
/// The preference above stays, and its reason with it: rust-lld shares the
/// compiler's LLVM version and reads bitcode the platform linker rejects. This
/// only stops an rust-lld that cannot start from WINNING that preference, which
/// lets discovery fall through to PATH — where the platform linker is, and
/// where the archive it will be handed has already had its bitcode stripped by
/// the root `build.rs`.
fn can_launch(path: &Path) -> bool {
    match Command::new(path).arg("--version").output() {
        Ok(output) => output.status.code().is_some(),
        // Could not be spawned at all: missing, not executable, wrong
        // architecture. Indistinguishable from absent, and treated as absent.
        Err(_) => false,
    }
}

fn rustc_sysroot_rust_lld(layout: &ToolchainLayout) -> Option<PathBuf> {
    let sysroot = rustc_sysroot()?;
    let target_candidate = sysroot
        .join("lib")
        .join("rustlib")
        .join(&layout.target.triple)
        .join("bin")
        .join(expected_binary_name("rust-lld"));
    // Same `can_launch` gate as the rustup path, and for the same reason: this
    // is the SAME binary reached by a second route, so a check on one route and
    // not the other would let the broken one back in through the other door.
    if target_candidate.is_file() && can_launch(&target_candidate) {
        return Some(target_candidate);
    }

    let host = rustc_host_triple()?;
    let host_candidate = sysroot
        .join("lib")
        .join("rustlib")
        .join(host)
        .join("bin")
        .join(expected_binary_name("rust-lld"));
    (host_candidate.is_file() && can_launch(&host_candidate)).then_some(host_candidate)
}

fn rustc_sysroot() -> Option<PathBuf> {
    let output = Command::new("rustc")
        .args(["--print", "sysroot"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path.is_empty() {
        return None;
    }
    Some(PathBuf::from(path))
}

fn rustc_host_triple() -> Option<String> {
    let output = Command::new("rustc").arg("-vV").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    text.lines()
        .find_map(|line| line.strip_prefix("host: "))
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_linker_that_is_not_there_cannot_be_launched() {
        // The spawn itself fails. Indistinguishable from absent, and the point
        // of the test is that it answers `false` rather than panicking — a
        // probe that propagated the error would turn a missing linker into a
        // failed compile instead of a fall-through to the next candidate.
        assert!(!can_launch(Path::new(
            "definitely-not-a-linker-on-this-machine"
        )));
    }

    #[test]
    fn a_file_that_is_not_a_program_cannot_be_launched() {
        // The case the macOS runner produced, as close as it can be reproduced
        // portably: a real, readable file at a real path that is not something
        // the operating system will start. `is_file()` answers TRUE for it,
        // which is exactly why `is_file()` was the wrong question.
        let path = std::env::temp_dir().join("rts_linker_probe_not_a_program");
        std::fs::write(&path, b"this is not an executable").expect("write the fixture");
        let answer = can_launch(&path);
        let _ = std::fs::remove_file(&path);
        assert!(
            !answer,
            "a file that exists and cannot be started must not be chosen as the linker"
        );
    }

    #[test]
    fn a_real_program_can_be_launched_even_when_it_answers_non_zero() {
        // The other half, and the one that says the probe is not merely
        // rejecting everything: what is asked is whether the process EXITED,
        // not whether it liked its arguments. `rustc --version` is a program
        // every machine building this repository has.
        //
        // If this fails, the probe has become strict enough to reject a working
        // linker, which would send macOS to the platform linker forever and
        // lose the bitcode handling the preference exists for.
        assert!(can_launch(Path::new("rustc")));
    }
}
