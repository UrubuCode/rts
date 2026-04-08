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

    if let Some(path) = rustup_rust_lld() {
        return Ok(ResolvedLinker { path });
    }

    if let Some(path) = rustc_sysroot_rust_lld(layout) {
        return Ok(ResolvedLinker { path });
    }

    for candidate in candidates {
        if let Some(path) = find_binary_in_path(candidate) {
            return Ok(ResolvedLinker { path });
        }
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
        TargetFlavor::Elf => &["ld.lld", "rust-lld", "lld", "clang", "cc"],
        TargetFlavor::MachO => &["ld64.lld", "rust-lld", "ld", "clang", "cc"],
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
    use super::{TargetFlavor, flavor_from_triple};

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
}
