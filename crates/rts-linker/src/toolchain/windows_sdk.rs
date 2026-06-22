//! Windows SDK / MSVC CRT import-lib discovery + xwin auto-provisioning.

use std::collections::VecDeque;
use std::env;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail};

use super::paths::resolve_toolchains_base_dir;

const WINDOWS_SYSROOT_ENV_VAR: &str = "RTS_WINDOWS_SYSROOT";
const XWIN_HTTP_RETRY_ENV_VAR: &str = "RTS_XWIN_HTTP_RETRY";
const XWIN_TIMEOUT_SECS: u64 = 120;
const XWIN_MANIFEST_VERSION: u8 = 17;
const XWIN_CHANNEL: &str = "release";

pub fn ensure_windows_msvc_runtime_lib_paths(target_triple: &str) -> Result<Vec<PathBuf>> {
    let lower = target_triple.to_ascii_lowercase();
    if !(lower.contains("windows") && lower.contains("msvc")) {
        return Ok(Vec::new());
    }

    if let Ok(configured_root) = env::var(WINDOWS_SYSROOT_ENV_VAR) {
        let configured_root = configured_root.trim();
        if !configured_root.is_empty() {
            let root = PathBuf::from(configured_root);
            if !root.is_dir() {
                bail!(
                    "{WINDOWS_SYSROOT_ENV_VAR} points to a missing directory: {}",
                    root.display()
                );
            }
            let paths = discover_windows_msvc_lib_paths(&root, target_triple);
            if windows_runtime_libs_available(&paths) {
                return Ok(paths);
            }
            bail!(
                "{WINDOWS_SYSROOT_ENV_VAR}={} does not contain required Windows import libs (kernel32.lib, ucrt.lib, vcruntime.lib/msvcrt.lib)",
                root.display()
            );
        }
    }

    let toolchains_base = resolve_toolchains_base_dir()?;
    let sysroot_root = toolchains_base.join("windows-msvc").join(target_triple);
    if let Some(paths) = discover_complete_windows_msvc_lib_paths(&sysroot_root, target_triple) {
        eprintln!(
            "RTS toolchain: using cached Windows SDK/CRT for target '{}' from {}",
            target_triple,
            sysroot_root.display()
        );
        return Ok(paths);
    }

    let xwin_cache = toolchains_base.join("xwin").join("cache");
    eprintln!(
        "RTS toolchain: downloading Windows SDK/CRT for target '{}'...",
        target_triple
    );
    run_xwin_splat(target_triple, &sysroot_root, &xwin_cache)?;

    if let Some(paths) = discover_complete_windows_msvc_lib_paths(&sysroot_root, target_triple) {
        eprintln!(
            "RTS toolchain: Windows SDK/CRT downloaded and cached at {}",
            sysroot_root.display()
        );
        return Ok(paths);
    }

    bail!(
        "automatic Windows SDK/CRT provisioning finished but required import libs were not found under {}",
        sysroot_root.display()
    )
}

fn run_xwin_splat(target_triple: &str, output_root: &Path, cache_dir: &Path) -> Result<()> {
    std::fs::create_dir_all(cache_dir)
        .with_context(|| format!("failed to create {}", cache_dir.display()))?;
    std::fs::create_dir_all(output_root)
        .with_context(|| format!("failed to create {}", output_root.display()))?;

    let xwin_arch = xwin_arch_for_target(target_triple);
    let arches = xwin_arch as u32;
    let variants = xwin::Variant::Desktop as u32;

    let cache_dir = to_utf8_pathbuf(cache_dir)?;
    let output_root = to_utf8_pathbuf(output_root)?;
    let client = xwin_http_agent()?;
    let ctx = xwin::Ctx::with_dir(
        cache_dir,
        xwin::util::ProgressTarget::Hidden,
        client,
        xwin_http_retry(),
    )
    .context("failed to initialize xwin context")?;
    let ctx = std::sync::Arc::new(ctx);

    let manifest_pb = indicatif::ProgressBar::hidden();
    let manifest = xwin::manifest::get_manifest(
        &ctx,
        XWIN_MANIFEST_VERSION,
        XWIN_CHANNEL,
        manifest_pb.clone(),
    )
    .context("failed to fetch xwin manifest")?;
    let pkg_manifest = xwin::manifest::get_package_manifest(&ctx, &manifest, manifest_pb)
        .context("failed to fetch xwin package manifest")?;

    let pruned = xwin::prune_pkg_list(&pkg_manifest, arches, variants, false, false, None, None)
        .context("failed to prepare xwin package list")?;
    let op = xwin::Ops::Splat(xwin::SplatConfig {
        include_debug_libs: false,
        include_debug_symbols: false,
        enable_symlinks: false,
        preserve_ms_arch_notation: false,
        use_winsysroot_style: false,
        copy: false,
        map: None,
        output: output_root,
    });
    let work_items = pruned
        .payloads
        .into_iter()
        .map(|payload| xwin::WorkItem {
            payload: std::sync::Arc::new(payload),
            progress: indicatif::ProgressBar::hidden(),
        })
        .collect::<Vec<_>>();

    ctx.execute(
        pkg_manifest.packages,
        work_items,
        pruned.crt_version,
        pruned.sdk_version,
        pruned.vcr_version,
        arches,
        variants,
        op,
    )
    .context("xwin execution failed")
}

fn to_utf8_pathbuf(path: &Path) -> Result<xwin::PathBuf> {
    xwin::PathBuf::from_path_buf(path.to_path_buf()).map_err(|invalid| {
        anyhow::anyhow!(
            "path '{}' is not valid UTF-8; automatic Windows SDK provisioning requires UTF-8 paths",
            invalid.display()
        )
    })
}

fn xwin_http_retry() -> u8 {
    env::var(XWIN_HTTP_RETRY_ENV_VAR)
        .ok()
        .and_then(|raw| raw.trim().parse::<u8>().ok())
        .unwrap_or(1)
}

fn xwin_http_agent() -> Result<xwin::ureq::Agent> {
    let mut config = xwin::ureq::config::Config::builder();
    config = config.timeout_recv_body(Some(Duration::from_secs(XWIN_TIMEOUT_SECS)));

    if let Some(proxy_raw) = env::var("HTTPS_PROXY")
        .ok()
        .or_else(|| env::var("https_proxy").ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        let proxy = xwin::ureq::Proxy::new(&proxy_raw)
            .with_context(|| format!("failed to parse HTTPS proxy '{proxy_raw}'"))?;
        config = config.proxy(Some(proxy));
    }

    let tls_config = xwin::ureq::tls::TlsConfig::builder()
        .root_certs(xwin::ureq::tls::RootCerts::PlatformVerifier)
        .build();
    config = config.tls_config(tls_config);
    Ok(config.build().new_agent())
}

pub(crate) fn discover_complete_windows_msvc_lib_paths(
    root: &Path,
    target_triple: &str,
) -> Option<Vec<PathBuf>> {
    let paths = discover_windows_msvc_lib_paths(root, target_triple);
    windows_runtime_libs_available(&paths).then_some(paths)
}

fn discover_windows_msvc_lib_paths(root: &Path, target_triple: &str) -> Vec<PathBuf> {
    if !root.is_dir() {
        return Vec::new();
    }

    let arch_tokens = windows_arch_tokens(target_triple);
    let mut paths = Vec::<PathBuf>::new();

    if let Some(path) =
        select_best_arch_dir(find_dirs_with_file(root, "kernel32.lib", 8), arch_tokens)
    {
        paths.push(path);
    }
    if let Some(path) = select_best_arch_dir(find_dirs_with_file(root, "ucrt.lib", 8), arch_tokens)
    {
        paths.push(path);
    }

    let mut crt_candidates = find_dirs_with_file(root, "vcruntime.lib", 8);
    crt_candidates.extend(find_dirs_with_file(root, "msvcrt.lib", 8));
    if let Some(path) = select_best_arch_dir(crt_candidates, arch_tokens) {
        paths.push(path);
    }

    paths.sort();
    paths.dedup();
    paths
}

fn find_dirs_with_file(root: &Path, file_name: &str, max_depth: usize) -> Vec<PathBuf> {
    let mut out = Vec::<PathBuf>::new();
    let mut queue = VecDeque::<(PathBuf, usize)>::new();
    queue.push_back((root.to_path_buf(), 0));
    while let Some((dir, depth)) = queue.pop_front() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };

        let mut has_file = false;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if depth < max_depth {
                    queue.push_back((path, depth + 1));
                }
                continue;
            }

            if path.is_file()
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .map(|name| name.eq_ignore_ascii_case(file_name))
                    .unwrap_or(false)
            {
                has_file = true;
            }
        }

        if has_file {
            out.push(dir);
        }
    }
    out
}

fn select_best_arch_dir(mut candidates: Vec<PathBuf>, arch_tokens: &[&str]) -> Option<PathBuf> {
    candidates.sort_by(|left, right| {
        let left_key = (
            !path_matches_target_arch(left, arch_tokens),
            path_depth(left),
            left.to_string_lossy().len(),
        );
        let right_key = (
            !path_matches_target_arch(right, arch_tokens),
            path_depth(right),
            right.to_string_lossy().len(),
        );
        left_key.cmp(&right_key)
    });
    candidates.into_iter().next()
}

fn windows_runtime_libs_available(paths: &[PathBuf]) -> bool {
    let has_um = paths.iter().any(|path| path.join("kernel32.lib").is_file());
    let has_ucrt = paths.iter().any(|path| path.join("ucrt.lib").is_file());
    let has_crt = paths
        .iter()
        .any(|path| path.join("vcruntime.lib").is_file() || path.join("msvcrt.lib").is_file());
    has_um && has_ucrt && has_crt
}

fn path_matches_target_arch(path: &Path, tokens: &[&str]) -> bool {
    path.components().any(|component| {
        let component = component.as_os_str().to_string_lossy().to_ascii_lowercase();
        tokens.iter().any(|token| {
            if *token == "x86" {
                return component == "x86" || component == "i686";
            }
            component == *token || component.contains(token)
        })
    })
}

fn path_depth(path: &Path) -> usize {
    path.components().count()
}

pub(crate) fn xwin_arch_for_target(target_triple: &str) -> xwin::Arch {
    let lower = target_triple.to_ascii_lowercase();
    if lower.starts_with("x86_64-") {
        xwin::Arch::X86_64
    } else if lower.starts_with("i686-") || lower.starts_with("x86-") {
        xwin::Arch::X86
    } else if lower.starts_with("aarch64-") {
        xwin::Arch::Aarch64
    } else {
        xwin::Arch::X86_64
    }
}

fn windows_arch_tokens(target_triple: &str) -> &'static [&'static str] {
    let lower = target_triple.to_ascii_lowercase();
    if lower.starts_with("x86_64-") {
        &["x64", "x86_64", "amd64"]
    } else if lower.starts_with("i686-") || lower.starts_with("x86-") {
        &["x86", "i686"]
    } else if lower.starts_with("aarch64-") {
        &["arm64", "aarch64"]
    } else {
        &["x64", "x86_64", "amd64"]
    }
}
