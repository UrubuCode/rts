pub mod install;
pub mod lock;
pub mod npm;

use std::path::PathBuf;

use anyhow::{Result, bail};

pub const ENV_SYMLINK: &str = "RTS_SYMBOL_NODE_MODULES";

pub fn use_symlinks() -> bool {
    matches!(
        std::env::var(ENV_SYMLINK).as_deref(),
        Ok("1") | Ok("true") | Ok("yes")
    )
}

pub fn home_dir() -> Result<PathBuf> {
    if let Ok(home) = std::env::var("HOME") {
        if !home.trim().is_empty() {
            return Ok(PathBuf::from(home));
        }
    }
    if let Ok(profile) = std::env::var("USERPROFILE") {
        if !profile.trim().is_empty() {
            return Ok(PathBuf::from(profile));
        }
    }
    bail!("unable to resolve home directory")
}

pub fn rts_home() -> Result<PathBuf> {
    Ok(home_dir()?.join(".rts"))
}

pub fn register_dir() -> Result<PathBuf> {
    Ok(rts_home()?.join("register"))
}

pub fn globals_dir() -> Result<PathBuf> {
    Ok(rts_home()?.join("globals"))
}
