//! `rts i` / `rts install` command.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use colored::Colorize;

use crate::module::manifest::{RawPackageManifest, strip_json_comments};
use crate::registers::install::{InstallRequest, install_packages};
use crate::registers::lock::LockFile;

pub fn command(extra_pkgs: Vec<String>) -> Result<()> {
    let root = find_project_root()?;

    println!(
        "{} {}",
        "rts install".bold(),
        root.display()
    );

    let mut lock = LockFile::load(&root)?;

    let requests: Vec<InstallRequest> = if extra_pkgs.is_empty() {
        deps_from_package_json(&root)?
    } else {
        extra_pkgs.iter().map(|s| parse_pkg_arg(s)).collect()
    };

    if requests.is_empty() {
        println!("No packages to install (empty dependencies in package.json).");
        return Ok(());
    }

    println!("  {} packages to install", requests.len());

    let installed = install_packages(&root, &requests, &mut lock)
        .context("install packages")?;

    lock.save(&root)?;

    for pkg in &installed {
        println!(
            "  {} {}@{}",
            "+".green(),
            pkg.name.bold(),
            pkg.version
        );
    }

    println!(
        "\n{} {} package{} installed. Lock saved to rts.lock",
        "✓".green().bold(),
        installed.len(),
        if installed.len() == 1 { "" } else { "s" }
    );

    Ok(())
}

fn find_project_root() -> Result<PathBuf> {
    let cwd = std::env::current_dir().context("get cwd")?;
    let mut dir = cwd.as_path();
    loop {
        if dir.join("package.json").exists() {
            return Ok(dir.to_path_buf());
        }
        match dir.parent() {
            Some(p) => dir = p,
            None => break,
        }
    }
    Ok(cwd)
}

fn deps_from_package_json(root: &Path) -> Result<Vec<InstallRequest>> {
    let pkg_path = root.join("package.json");
    if !pkg_path.exists() {
        return Ok(vec![]);
    }
    let raw = std::fs::read_to_string(&pkg_path)?;
    let clean = strip_json_comments(&raw);
    let parsed: RawPackageManifest = serde_json::from_str(&clean)
        .with_context(|| format!("parse {}", pkg_path.display()))?;

    Ok(parsed
        .dependencies
        .into_iter()
        .map(|(name, spec)| InstallRequest {
            name,
            version_spec: spec,
        })
        .collect())
}

fn parse_pkg_arg(arg: &str) -> InstallRequest {
    // Scoped: @org/pkg@1.2.3
    if arg.starts_with('@') {
        if let Some(at) = arg[1..].rfind('@') {
            let split = at + 1;
            return InstallRequest {
                name: arg[..split].to_string(),
                version_spec: arg[split + 1..].to_string(),
            };
        }
        return InstallRequest {
            name: arg.to_string(),
            version_spec: "latest".to_string(),
        };
    }

    // Unscoped: pkg@1.2.3
    if let Some((name, ver)) = arg.split_once('@') {
        InstallRequest {
            name: name.to_string(),
            version_spec: ver.to_string(),
        }
    } else {
        InstallRequest {
            name: arg.to_string(),
            version_spec: "latest".to_string(),
        }
    }
}
