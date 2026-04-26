use std::collections::VecDeque;
use std::env;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};

const TOOLCHAINS_ENV_VAR: &str = "RTS_TOOLCHAINS_PATH";
const TARGET_ENV_VAR: &str = "RTS_TARGET";
const LINKER_DOWNLOAD_URL_ENV_VAR: &str = "RTS_LINKER_DOWNLOAD_URL";
const LINKER_SHA256_ENV_VAR: &str = "RTS_LINKER_SHA256";
const WINDOWS_SYSROOT_ENV_VAR: &str = "RTS_WINDOWS_SYSROOT";
const XWIN_HTTP_RETRY_ENV_VAR: &str = "RTS_XWIN_HTTP_RETRY";
const XWIN_TIMEOUT_SECS: u64 = 120;
const XWIN_MANIFEST_VERSION: u8 = 17;
const XWIN_CHANNEL: &str = "release";
const RUST_DIST_MANIFEST_URL: &str = "https://static.rust-lang.org/dist/channel-rust-stable.toml";
const RUST_LLD_TOOL_NAME: &str = "rust-lld";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetFlavor {
    Coff,
    Elf,
    MachO,
}

impl TargetFlavor {
    pub fn format_name(self) -> &'static str {
        match self {
            Self::Coff => "coff",
            Self::Elf => "elf",
            Self::MachO => "mach-o",
        }
    }
}

#[derive(Debug, Clone)]
pub struct TargetTriple {
    pub triple: String,
    pub flavor: TargetFlavor,
}

impl TargetTriple {
    pub fn resolve(explicit_target: Option<&str>) -> Self {
        let triple = explicit_target
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .or_else(|| {
                env::var(TARGET_ENV_VAR)
                    .ok()
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty())
            })
            .unwrap_or_else(host_target_triple);

        let flavor = flavor_from_triple(&triple);
        Self { triple, flavor }
    }
}

#[derive(Debug, Clone)]
pub struct ToolchainLayout {
    pub target: TargetTriple,
    pub root: PathBuf,
    pub bin_dir: PathBuf,
}

impl ToolchainLayout {
    pub fn resolve(explicit_target: Option<&str>) -> Result<Self> {
        let target = TargetTriple::resolve(explicit_target);
        let root = resolve_toolchains_base_dir()?.join(&target.triple);
        let bin_dir = root.join("bin");
        std::fs::create_dir_all(&bin_dir)
            .with_context(|| format!("failed to create {}", bin_dir.display()))?;
        Ok(Self {
            target,
            root,
            bin_dir,
        })
    }
}

#[derive(Debug, Clone)]
pub struct ResolvedLinker {
    pub path: PathBuf,
}

impl ResolvedLinker {
    pub fn name(&self) -> String {
        self.path
            .file_name()
            .and_then(|value| value.to_str())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| self.path.display().to_string())
    }

    pub fn is_rust_lld(&self) -> bool {
        let lower = lowercase_stem(&self.path);
        lower == "rust-lld"
    }

    pub fn is_link_style(&self) -> bool {
        let lower = lowercase_stem(&self.path);
        lower == "lld-link" || lower == "link"
    }

    pub fn is_compiler_driver(&self) -> bool {
        let lower = lowercase_stem(&self.path);
        lower == "cc"
            || lower == "gcc"
            || lower == "clang"
            || lower.starts_with("clang-")
            || lower.starts_with("gcc-")
    }

    /// Raw linkers (ld.lld, rust-lld, ld64.lld) don't add CRT objects or system libs
    /// automatically — unlike compiler drivers (cc, clang) that wrap the linker with those.
    pub fn is_raw_linker(&self) -> bool {
        !self.is_compiler_driver() && !self.is_link_style()
    }
}

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

fn resolve_toolchains_base_dir() -> Result<PathBuf> {
    if let Ok(configured) = env::var(TOOLCHAINS_ENV_VAR) {
        let configured = configured.trim();
        if configured.is_empty() || configured == "~" {
            return default_toolchains_base_dir();
        }
        return expand_tilde_path(configured);
    }

    default_toolchains_base_dir()
}

fn default_toolchains_base_dir() -> Result<PathBuf> {
    Ok(home_dir()?.join(".rts").join("toolchains"))
}

fn home_dir() -> Result<PathBuf> {
    if let Ok(home) = env::var("HOME") {
        if !home.trim().is_empty() {
            return Ok(PathBuf::from(home));
        }
    }

    if let Ok(profile) = env::var("USERPROFILE") {
        if !profile.trim().is_empty() {
            return Ok(PathBuf::from(profile));
        }
    }

    bail!("unable to resolve user home directory for RTS toolchain cache")
}

fn expand_tilde_path(raw: &str) -> Result<PathBuf> {
    if raw == "~" {
        return default_toolchains_base_dir();
    }

    if let Some(rest) = raw.strip_prefix("~/") {
        return home_dir().map(|home| home.join(rest));
    }

    Ok(PathBuf::from(raw))
}

fn maybe_download_linker(
    layout: &ToolchainLayout,
    binary_name: &str,
    toolchains_base: &Path,
) -> Result<Option<PathBuf>> {
    let Some(template) = env::var(LINKER_DOWNLOAD_URL_ENV_VAR)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };

    let binary_file = expected_binary_name(binary_name);
    let url = template
        .replace("{target}", &layout.target.triple)
        .replace("{binary}", &binary_file);

    let destination = cache_destination_for_tool(
        toolchains_base,
        sanitize_tool_dir_name(binary_name).as_str(),
        &layout.target.triple,
        &binary_file,
    )?;
    mirror_to_legacy_layout(&layout.bin_dir, &binary_file, &destination)?;
    if destination.is_file() {
        eprintln!(
            "RTS toolchain: using cached target '{}' from {}",
            layout.target.triple,
            destination.display()
        );
        return Ok(Some(destination));
    }

    eprintln!(
        "RTS toolchain: getting target '{}' linker from web...",
        layout.target.triple
    );
    let bytes = download_url_bytes(&url)?;

    if let Some(expected) = env::var(LINKER_SHA256_ENV_VAR)
        .ok()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
    {
        verify_sha256(&bytes, &expected, &url)?;
    }

    std::fs::write(&destination, &bytes).with_context(|| {
        format!(
            "failed to write downloaded linker {}",
            destination.display()
        )
    })?;
    mirror_to_legacy_layout(&layout.bin_dir, &binary_file, &destination)?;
    set_executable_if_supported(&destination)?;

    eprintln!(
        "RTS toolchain: target '{}' linker downloaded and cached.",
        layout.target.triple
    );

    Ok(Some(destination))
}

fn maybe_download_rust_dist_linker(
    layout: &ToolchainLayout,
    toolchains_base: &Path,
) -> Result<Option<PathBuf>> {
    let binary_name = expected_binary_name("rust-lld");
    let destination = cache_destination_for_tool(
        toolchains_base,
        RUST_LLD_TOOL_NAME,
        &layout.target.triple,
        &binary_name,
    )?;
    mirror_to_legacy_layout(&layout.bin_dir, &binary_name, &destination)?;
    if destination.is_file() {
        eprintln!(
            "RTS toolchain: using cached target '{}' from {}",
            layout.target.triple,
            destination.display()
        );
        return Ok(Some(destination));
    }

    let Some(artifact) = rust_dist_rustc_artifact_for_target(&layout.target.triple)? else {
        return Ok(None);
    };

    eprintln!(
        "RTS toolchain: getting target '{}' from Rust dist...",
        layout.target.triple
    );
    let archive_bytes = download_url_bytes(&artifact.url)?;
    verify_sha256(&archive_bytes, &artifact.hash, &artifact.url)?;

    if !extract_rust_lld_from_rustc_archive(&archive_bytes, &destination)? {
        bail!(
            "downloaded Rust dist archive did not contain rust-lld for target {} ({})",
            layout.target.triple,
            artifact.url
        );
    }
    mirror_to_legacy_layout(&layout.bin_dir, &binary_name, &destination)?;

    eprintln!(
        "RTS toolchain: target '{}' downloaded and cached.",
        layout.target.triple
    );

    Ok(Some(destination))
}

fn cache_destination_for_tool(
    toolchains_base: &Path,
    tool_name: &str,
    target: &str,
    binary_file: &str,
) -> Result<PathBuf> {
    let dir = toolchains_base
        .join(sanitize_tool_dir_name(tool_name))
        .join(target);
    std::fs::create_dir_all(&dir).with_context(|| format!("failed to create {}", dir.display()))?;
    Ok(dir.join(binary_file))
}

fn mirror_to_legacy_layout(legacy_bin_dir: &Path, binary_file: &str, source: &Path) -> Result<()> {
    if !source.is_file() {
        return Ok(());
    }

    std::fs::create_dir_all(legacy_bin_dir)
        .with_context(|| format!("failed to create {}", legacy_bin_dir.display()))?;
    let destination = legacy_bin_dir.join(binary_file);
    if destination == source {
        return Ok(());
    }
    std::fs::copy(source, &destination).with_context(|| {
        format!(
            "failed to mirror tool '{}' to legacy layout at {}",
            source.display(),
            destination.display()
        )
    })?;
    set_executable_if_supported(&destination)?;
    Ok(())
}

#[derive(Debug, Clone)]
struct RustDistArtifact {
    url: String,
    hash: String,
}

fn rust_dist_rustc_artifact_for_target(target: &str) -> Result<Option<RustDistArtifact>> {
    let manifest_bytes = download_url_bytes(RUST_DIST_MANIFEST_URL)?;
    let manifest = String::from_utf8(manifest_bytes)
        .with_context(|| format!("failed to decode {}", RUST_DIST_MANIFEST_URL))?;

    let header = format!("[pkg.rustc.target.{target}]");
    let mut in_section = false;
    let mut available = None::<bool>;
    let mut url = None::<String>;
    let mut hash = None::<String>;

    for raw_line in manifest.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if line.starts_with('[') {
            if in_section {
                break;
            }
            in_section = line == header;
            continue;
        }

        if !in_section {
            continue;
        }

        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim();

        match key {
            "available" => {
                available = Some(value.eq_ignore_ascii_case("true"));
            }
            "url" => {
                if let Some(parsed) = parse_toml_string(value) {
                    url = Some(parsed);
                }
            }
            "hash" => {
                if let Some(parsed) = parse_toml_string(value) {
                    hash = Some(parsed.to_ascii_lowercase());
                }
            }
            _ => {}
        }
    }

    if !in_section {
        return Ok(None);
    }

    if !available.unwrap_or(false) {
        return Ok(None);
    }

    match (url, hash) {
        (Some(url), Some(hash)) => Ok(Some(RustDistArtifact { url, hash })),
        _ => Ok(None),
    }
}

fn parse_toml_string(raw: &str) -> Option<String> {
    let raw = raw.trim();
    raw.strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .map(|value| value.to_string())
}

fn extract_rust_lld_from_rustc_archive(archive_bytes: &[u8], destination: &Path) -> Result<bool> {
    let decoder = flate2::read::GzDecoder::new(Cursor::new(archive_bytes));
    let mut archive = tar::Archive::new(decoder);

    for entry in archive
        .entries()
        .context("failed to read Rust dist archive")?
    {
        let mut entry = entry.context("failed to read Rust dist archive entry")?;
        let path = entry
            .path()
            .context("failed to read Rust dist archive entry path")?;
        let normalized = path.to_string_lossy().replace('\\', "/");

        if normalized.ends_with("/bin/rust-lld") || normalized.ends_with("/bin/rust-lld.exe") {
            if let Some(parent) = destination.parent() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("failed to create {}", parent.display()))?;
            }

            let mut file = std::fs::File::create(destination)
                .with_context(|| format!("failed to create {}", destination.display()))?;
            std::io::copy(&mut entry, &mut file)
                .with_context(|| format!("failed to extract {}", destination.display()))?;
            set_executable_if_supported(destination)?;
            return Ok(true);
        }
    }

    Ok(false)
}

fn download_url_bytes(url: &str) -> Result<Vec<u8>> {
    let response = match ureq::get(url).timeout(Duration::from_secs(90)).call() {
        Ok(value) => value,
        Err(ureq::Error::Status(code, response)) => {
            bail!(
                "failed to download {} (HTTP {} {})",
                url,
                code,
                response.status_text()
            )
        }
        Err(ureq::Error::Transport(error)) => {
            bail!("failed to download {} ({})", url, error)
        }
    };

    let mut bytes = Vec::new();
    response
        .into_reader()
        .read_to_end(&mut bytes)
        .with_context(|| format!("failed to read downloaded body from {}", url))?;
    Ok(bytes)
}

fn verify_sha256(bytes: &[u8], expected: &str, label: &str) -> Result<()> {
    let digest = Sha256::digest(bytes);
    let actual = format!("{digest:x}");
    if actual != expected.to_ascii_lowercase() {
        bail!(
            "SHA-256 mismatch for {} (expected {}, got {})",
            label,
            expected,
            actual
        );
    }
    Ok(())
}

fn set_executable_if_supported(_path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = std::fs::metadata(_path)
            .with_context(|| format!("failed to stat {}", _path.display()))?
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(_path, permissions)
            .with_context(|| format!("failed to update permissions for {}", _path.display()))?;
    }

    Ok(())
}

fn find_binary_in_path(binary_name: &str) -> Option<PathBuf> {
    let path_env = env::var_os("PATH")?;
    env::split_paths(&path_env).find_map(|directory| find_binary_in_dir(&directory, binary_name))
}

fn find_binary_in_dir(directory: &Path, binary_name: &str) -> Option<PathBuf> {
    let with_name = directory.join(binary_name);
    if with_name.is_file() {
        return Some(with_name);
    }

    if cfg!(windows) {
        if Path::new(binary_name).extension().is_none() {
            let with_exe = directory.join(format!("{binary_name}.exe"));
            if with_exe.is_file() {
                return Some(with_exe);
            }
        }
    }

    None
}

fn expected_binary_name(binary_name: &str) -> String {
    if cfg!(windows) && Path::new(binary_name).extension().is_none() {
        format!("{binary_name}.exe")
    } else {
        binary_name.to_string()
    }
}

fn sanitize_tool_dir_name(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-') {
            out.push(ch);
        } else {
            out.push('_');
        }
    }

    let trimmed = out.trim_matches('_').to_string();
    if trimmed.is_empty() {
        "tool".to_string()
    } else {
        trimmed
    }
}

pub fn toolchains_base_dir() -> Result<PathBuf> {
    resolve_toolchains_base_dir()
}

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

fn discover_complete_windows_msvc_lib_paths(
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

fn xwin_arch_for_target(target_triple: &str) -> xwin::Arch {
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

fn host_target_triple() -> String {
    let arch = match env::consts::ARCH {
        "x86_64" => "x86_64",
        "aarch64" => "aarch64",
        "x86" => "i686",
        other => other,
    };

    let os = match env::consts::OS {
        "windows" => {
            if cfg!(target_env = "gnu") {
                "pc-windows-gnu"
            } else {
                "pc-windows-msvc"
            }
        }
        "macos" => "apple-darwin",
        "linux" => {
            if cfg!(target_env = "musl") {
                "unknown-linux-musl"
            } else {
                "unknown-linux-gnu"
            }
        }
        "freebsd" => "unknown-freebsd",
        other => return format!("{arch}-unknown-{other}"),
    };

    format!("{arch}-{os}")
}

fn flavor_from_triple(triple: &str) -> TargetFlavor {
    let lower = triple.to_ascii_lowercase();
    if lower.contains("windows") {
        TargetFlavor::Coff
    } else if lower.contains("darwin") || lower.contains("-apple-") {
        TargetFlavor::MachO
    } else {
        TargetFlavor::Elf
    }
}

fn lowercase_stem(path: &Path) -> String {
    path.file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{
        TargetFlavor, discover_complete_windows_msvc_lib_paths, flavor_from_triple,
        xwin_arch_for_target,
    };

    #[test]
    fn flavor_detection_works_for_common_triples() {
        assert_eq!(
            flavor_from_triple("x86_64-pc-windows-msvc"),
            TargetFlavor::Coff
        );
        assert_eq!(
            flavor_from_triple("x86_64-unknown-linux-gnu"),
            TargetFlavor::Elf
        );
        assert_eq!(
            flavor_from_triple("aarch64-apple-darwin"),
            TargetFlavor::MachO
        );
    }

    #[test]
    fn xwin_arch_mapping_matches_common_targets() {
        assert_eq!(
            xwin_arch_for_target("x86_64-pc-windows-msvc"),
            xwin::Arch::X86_64
        );
        assert_eq!(
            xwin_arch_for_target("i686-pc-windows-msvc"),
            xwin::Arch::X86
        );
        assert_eq!(
            xwin_arch_for_target("aarch64-pc-windows-msvc"),
            xwin::Arch::Aarch64
        );
    }

    #[test]
    fn discover_windows_lib_paths_from_splat_layout() {
        let root = temp_test_dir("windows_msvc_splat");
        std::fs::create_dir_all(root.join("sdk/lib/um/x64")).expect("create um");
        std::fs::create_dir_all(root.join("sdk/lib/ucrt/x64")).expect("create ucrt");
        std::fs::create_dir_all(root.join("crt/lib/x64")).expect("create crt");

        std::fs::write(root.join("sdk/lib/um/x64/kernel32.lib"), b"").expect("write kernel32");
        std::fs::write(root.join("sdk/lib/ucrt/x64/ucrt.lib"), b"").expect("write ucrt");
        std::fs::write(root.join("crt/lib/x64/vcruntime.lib"), b"").expect("write vcruntime");

        let paths = discover_complete_windows_msvc_lib_paths(&root, "x86_64-pc-windows-msvc")
            .expect("paths should be discovered");
        assert_eq!(paths.len(), 3);

        let _ = std::fs::remove_dir_all(&root);
    }

    fn temp_test_dir(tag: &str) -> PathBuf {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock drift");
        std::env::temp_dir().join(format!(
            "rts_toolchain_{tag}_{}_{}",
            std::process::id(),
            now.as_nanos()
        ))
    }
}
