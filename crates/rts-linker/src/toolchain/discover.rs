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
    candidate.is_file().then_some(candidate)
}

fn rustc_sysroot_rust_lld(layout: &ToolchainLayout) -> Option<PathBuf> {
    let sysroot = rustc_sysroot()?;
    let target_candidate = sysroot
        .join("lib")
        .join("rustlib")
        .join(&layout.target.triple)
        .join("bin")
        .join(expected_binary_name("rust-lld"));
    if target_candidate.is_file() {
        return Some(target_candidate);
    }

    let host = rustc_host_triple()?;
    let host_candidate = sysroot
        .join("lib")
        .join("rustlib")
        .join(host)
        .join("bin")
        .join(expected_binary_name("rust-lld"));
    host_candidate.is_file().then_some(host_candidate)
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
