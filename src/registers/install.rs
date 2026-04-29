use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use super::lock::LockFile;
use super::npm;
use super::use_symlinks;

pub struct InstallRequest {
    pub name: String,
    pub version_spec: String,
}

#[derive(Debug)]
pub struct InstalledPackage {
    pub name: String,
    pub version: String,
    pub path: PathBuf,
}

pub fn install_packages(
    project_root: &Path,
    requests: &[InstallRequest],
    lock: &mut LockFile,
) -> Result<Vec<InstalledPackage>> {
    let node_modules = project_root.join("node_modules");
    std::fs::create_dir_all(&node_modules)
        .with_context(|| format!("create node_modules at {}", node_modules.display()))?;

    let bin_dir = node_modules.join(".bin");
    std::fs::create_dir_all(&bin_dir)?;

    let symlink_mode = use_symlinks();
    let mut installed = Vec::new();

    for req in requests {
        let pkg = install_one(req, &node_modules, &bin_dir, lock, symlink_mode)
            .with_context(|| format!("install {}@{}", req.name, req.version_spec))?;
        installed.push(pkg);
    }

    Ok(installed)
}

fn install_one(
    req: &InstallRequest,
    node_modules: &Path,
    bin_dir: &Path,
    lock: &mut LockFile,
    symlink_mode: bool,
) -> Result<InstalledPackage> {
    let (pkg, register_path) = npm::resolve_and_fetch(&req.name, &req.version_spec)?;

    lock.insert_npm(
        &pkg.name,
        &pkg.version,
        &pkg.tarball_url,
        pkg.integrity.as_deref(),
        pkg.dependencies.clone(),
    );

    // For scoped packages like @org/pkg, ensure @org/ exists
    let dest = pkg_dest(node_modules, &pkg.name);
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Remove stale entry
    if dest.exists() || dest.is_symlink() {
        if dest.is_dir() && !dest.is_symlink() {
            std::fs::remove_dir_all(&dest).ok();
        } else {
            std::fs::remove_file(&dest).ok();
        }
    }

    if symlink_mode {
        symlink_dir(&register_path, &dest).with_context(|| {
            format!(
                "symlink {} -> {}",
                register_path.display(),
                dest.display()
            )
        })?;
    } else {
        copy_dir_all(&register_path, &dest)?;
    }

    // Install .bin entries
    for (bin_name, bin_rel) in &pkg.bin {
        let target = dest.join(bin_rel);
        install_bin(bin_dir, bin_name, &target, symlink_mode).ok();
    }

    Ok(InstalledPackage {
        name: pkg.name,
        version: pkg.version,
        path: dest,
    })
}

fn pkg_dest(node_modules: &Path, name: &str) -> PathBuf {
    // Scoped: @org/pkg → node_modules/@org/pkg
    node_modules.join(name)
}

fn copy_dir_all(src: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_dir() {
            copy_dir_all(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

fn symlink_dir(src: &Path, dst: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(src, dst)?;
    }
    #[cfg(windows)]
    {
        std::os::windows::fs::symlink_dir(src, dst)?;
    }
    Ok(())
}

fn install_bin(bin_dir: &Path, name: &str, target: &Path, _symlink_mode: bool) -> Result<()> {
    #[cfg(unix)]
    {
        let bin_path = bin_dir.join(name);
        if symlink_mode {
            if bin_path.exists() || bin_path.is_symlink() {
                std::fs::remove_file(&bin_path).ok();
            }
            std::os::unix::fs::symlink(target, &bin_path)?;
        } else {
            let script = format!("#!/bin/sh\nexec \"{}\" \"$@\"\n", target.display());
            std::fs::write(&bin_path, &script)?;
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&bin_path, std::fs::Permissions::from_mode(0o755))?;
        }
    }
    #[cfg(windows)]
    {
        // .cmd wrapper for cmd.exe
        let cmd_path = bin_dir.join(format!("{name}.cmd"));
        let cmd_script = format!("@echo off\r\n\"{target}\" %*\r\n", target = target.display());
        std::fs::write(&cmd_path, cmd_script)?;

        // shell script for Git Bash / WSL
        let sh_path = bin_dir.join(name);
        let sh_script = format!("#!/bin/sh\nexec \"{}\" \"$@\"\n", target.display());
        std::fs::write(&sh_path, sh_script)?;
    }
    Ok(())
}
