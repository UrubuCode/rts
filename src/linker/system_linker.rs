use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

use super::toolchain::{ResolvedLinker, TargetFlavor, ToolchainLayout, resolve_linker};

#[derive(Debug, Clone)]
pub struct LinkedArtifact {
    pub path: PathBuf,
    pub format: String,
    pub linker: String,
}

pub fn link(
    object_paths: &[PathBuf],
    output_path: &Path,
    explicit_target: Option<&str>,
) -> Result<LinkedArtifact> {
    if object_paths.is_empty() {
        bail!("system linker received no object files to link");
    }

    let layout = ToolchainLayout::resolve(explicit_target)?;
    let linker = resolve_linker(&layout)?;
    let final_path = normalize_output_path(output_path, layout.target.flavor);

    if let Some(parent) = final_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create output directory {}", parent.display()))?;
    }

    let args = build_linker_args(&layout.target.flavor, object_paths, &final_path, &linker)?;
    let output = Command::new(&linker.path)
        .args(&args)
        .output()
        .with_context(|| {
            format!(
                "failed to invoke system linker '{}' for target {}",
                linker.path.display(),
                layout.target.triple
            )
        })?;

    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        bail!(
            "system linker '{}' failed for target {} (status={:?}, stdout='{}', stderr='{}')",
            linker.path.display(),
            layout.target.triple,
            output.status.code(),
            stdout,
            stderr
        );
    }

    Ok(LinkedArtifact {
        path: final_path,
        format: layout.target.flavor.format_name().to_string(),
        linker: linker.name(),
    })
}

fn build_linker_args(
    flavor: &TargetFlavor,
    object_paths: &[PathBuf],
    output_path: &Path,
    linker: &ResolvedLinker,
) -> Result<Vec<String>> {
    match flavor {
        TargetFlavor::Coff => {
            let mut args = Vec::new();

            if linker.is_rust_lld() {
                args.push("-flavor".to_string());
                args.push("link".to_string());
            } else if !linker.is_link_style() {
                bail!(
                    "COFF target requires link-compatible linker, found '{}'",
                    linker.path.display()
                );
            }

            args.push("/nologo".to_string());
            args.push("/entry:_start".to_string());
            args.push("/subsystem:console".to_string());
            args.push("/nodefaultlib".to_string());
            args.push(format!("/out:{}", output_path.display()));
            for object_path in object_paths {
                args.push(object_path.display().to_string());
            }

            Ok(args)
        }
        TargetFlavor::Elf => {
            let mut args = Vec::new();
            args.push("-o".to_string());
            args.push(output_path.display().to_string());
            for object_path in object_paths {
                args.push(object_path.display().to_string());
            }
            Ok(args)
        }
        TargetFlavor::MachO => {
            let mut args = Vec::new();
            if linker.is_rust_lld() {
                args.push("-flavor".to_string());
                args.push("darwin".to_string());
            }
            args.push("-o".to_string());
            args.push(output_path.display().to_string());
            for object_path in object_paths {
                args.push(object_path.display().to_string());
            }
            Ok(args)
        }
    }
}

fn normalize_output_path(path: &Path, flavor: TargetFlavor) -> PathBuf {
    if matches!(flavor, TargetFlavor::Coff) && path.extension().is_none() {
        return path.with_extension("exe");
    }
    path.to_path_buf()
}
