//! Target triple / flavor resolution and linker-handle types.

use std::env;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

const TARGET_ENV_VAR: &str = "RTS_TARGET";

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
        let root = super::paths::resolve_toolchains_base_dir()?.join(&target.triple);
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

pub(crate) fn flavor_from_triple(triple: &str) -> TargetFlavor {
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
