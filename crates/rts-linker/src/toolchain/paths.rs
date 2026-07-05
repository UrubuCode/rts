//! Toolchain cache directories + binary path helpers.

use std::env;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

const TOOLCHAINS_ENV_VAR: &str = "RTS_TOOLCHAINS_PATH";

pub(crate) fn resolve_toolchains_base_dir() -> Result<PathBuf> {
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

pub(crate) fn cache_destination_for_tool(
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

pub(crate) fn set_executable_if_supported(_path: &Path) -> Result<()> {
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

pub(crate) fn find_binary_in_path(binary_name: &str) -> Option<PathBuf> {
    let path_env = env::var_os("PATH")?;
    env::split_paths(&path_env).find_map(|directory| find_binary_in_dir(&directory, binary_name))
}

pub(crate) fn find_binary_in_dir(directory: &Path, binary_name: &str) -> Option<PathBuf> {
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

pub(crate) fn expected_binary_name(binary_name: &str) -> String {
    if cfg!(windows) && Path::new(binary_name).extension().is_none() {
        format!("{binary_name}.exe")
    } else {
        binary_name.to_string()
    }
}

pub(crate) fn sanitize_tool_dir_name(raw: &str) -> String {
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
