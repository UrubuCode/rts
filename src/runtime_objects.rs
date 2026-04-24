use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

use anyhow::{Context, Result, anyhow, bail};
use sha2::{Digest, Sha256};

pub(crate) const RUNTIME_OBJECTS_DIR_ENV_VAR: &str = "RTS_RUNTIME_OBJECTS_DIR";
pub(crate) const RUNTIME_OBJECTS_TOOL_NAME: &str = "rts-runtime-objects";

pub(crate) fn resolve_runtime_support_objects(deps_dir: &Path) -> Result<Vec<PathBuf>> {
    if let Some(paths) = find_prebuilt_runtime_objects(deps_dir)? {
        return Ok(paths);
    }

    if let Some(path) = find_cached_runtime_support_object()? {
        return Ok(vec![path]);
    }

    if let Some(path) = maybe_build_dev_runtime_support_object()? {
        return Ok(vec![path]);
    }

    Err(anyhow!(
        "RTS runtime support objects were not found. Provide prebuilt .o/.obj files via {} or in `runtime-objects` next to `rts`.",
        RUNTIME_OBJECTS_DIR_ENV_VAR
    ))
}

fn find_prebuilt_runtime_objects(deps_dir: &Path) -> Result<Option<Vec<PathBuf>>> {
    let mut candidates = Vec::<PathBuf>::new();

    if let Ok(explicit) = std::env::var(RUNTIME_OBJECTS_DIR_ENV_VAR) {
        let explicit = explicit.trim();
        if !explicit.is_empty() {
            candidates.push(PathBuf::from(explicit));
        }
    }

    if let Ok(current_exe) = std::env::current_exe() {
        if let Some(parent) = current_exe.parent() {
            candidates.push(parent.join("runtime-objects"));
            candidates.push(parent.join(".rts").join("runtime-objects"));
        }
    }

    candidates.push(deps_dir.join("runtime-objects"));

    for candidate in candidates {
        let objects = collect_runtime_object_files(&candidate)?;
        if !objects.is_empty() {
            return Ok(Some(objects));
        }
    }

    Ok(None)
}

fn collect_runtime_object_files(dir: &Path) -> Result<Vec<PathBuf>> {
    if !dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut files = std::fs::read_dir(dir)
        .with_context(|| format!("failed to list {}", dir.display()))?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .and_then(OsStr::to_str)
                .map(|ext| matches!(ext.to_ascii_lowercase().as_str(), "o" | "obj"))
                .unwrap_or(false)
        })
        .collect::<Vec<_>>();
    files.sort();
    Ok(files)
}

fn find_cached_runtime_support_object() -> Result<Option<PathBuf>> {
    let target = crate::linker::toolchain::TargetTriple::resolve(None);
    let base = crate::linker::toolchain::toolchains_base_dir()?
        .join(RUNTIME_OBJECTS_TOOL_NAME)
        .join(target.triple);
    if !base.is_dir() {
        return Ok(None);
    }

    let mut candidates = Vec::<PathBuf>::new();
    collect_named_runtime_objects_recursively(&base, &mut candidates)?;
    candidates.sort_by_key(|path| {
        std::fs::metadata(path)
            .ok()
            .and_then(|meta| meta.modified().ok())
            .unwrap_or(SystemTime::UNIX_EPOCH)
    });
    Ok(candidates.pop())
}

fn collect_named_runtime_objects_recursively(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    let entries =
        std::fs::read_dir(dir).with_context(|| format!("failed to list {}", dir.display()))?;
    for entry in entries {
        let entry = entry.with_context(|| format!("failed to read entry in {}", dir.display()))?;
        let path = entry.path();
        if path.is_dir() {
            collect_named_runtime_objects_recursively(&path, out)?;
            continue;
        }

        let Some(file_name) = path.file_name().and_then(OsStr::to_str) else {
            continue;
        };
        if file_name.eq_ignore_ascii_case("runtime_support.o")
            || file_name.eq_ignore_ascii_case("runtime_support.obj")
        {
            out.push(path);
        }
    }

    Ok(())
}

fn maybe_build_dev_runtime_support_object() -> Result<Option<PathBuf>> {
    let Some(manifest_dir) = find_manifest_dir_for_dev_build() else {
        return Ok(None);
    };

    let target = crate::linker::toolchain::TargetTriple::resolve(None);
    let cache_base = crate::linker::toolchain::toolchains_base_dir()?
        .join(RUNTIME_OBJECTS_TOOL_NAME)
        .join(&target.triple)
        .join("dev-build");
    std::fs::create_dir_all(&cache_base)
        .with_context(|| format!("failed to create {}", cache_base.display()))?;

    let cargo_target_dir = cache_base.join("cargo-target");
    std::fs::create_dir_all(&cargo_target_dir)
        .with_context(|| format!("failed to create {}", cargo_target_dir.display()))?;

    let mut command = Command::new("cargo");
    command
        .current_dir(&manifest_dir)
        .arg("rustc")
        .arg("--lib")
        .arg("--crate-type")
        .arg("staticlib")
        .arg("--target-dir")
        .arg(&cargo_target_dir);

    if running_from_release_executable() {
        command.arg("--release");
    }

    let output = command.output().with_context(|| {
        format!(
            "failed to invoke cargo to build runtime support objects from {}",
            manifest_dir.display()
        )
    })?;

    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        bail!(
            "failed to build runtime support object via cargo rustc (status={:?}, stdout='{}', stderr='{}')",
            output.status.code(),
            stdout,
            stderr
        );
    }

    let profile = if running_from_release_executable() {
        "release"
    } else {
        "debug"
    };

    let archive = find_generated_runtime_archive(&cargo_target_dir, profile).ok_or_else(|| {
        anyhow!(
            "cargo rustc succeeded but no runtime archive was produced under {}",
            cargo_target_dir.display()
        )
    })?;

    let object = materialize_runtime_archive_object(&archive)?;
    purge_runtime_archives(&cargo_target_dir)?;
    Ok(Some(object))
}

fn find_manifest_dir_for_dev_build() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let profile_dir = exe.parent()?;
    let profile_name = profile_dir.file_name()?.to_str()?;
    if !profile_name.eq_ignore_ascii_case("debug") && !profile_name.eq_ignore_ascii_case("release")
    {
        return None;
    }

    let target_dir = profile_dir.parent()?;
    if !target_dir
        .file_name()
        .and_then(OsStr::to_str)
        .map(|name| name.eq_ignore_ascii_case("target"))
        .unwrap_or(false)
    {
        return None;
    }

    let manifest_dir = target_dir.parent()?;
    let cargo_toml = manifest_dir.join("Cargo.toml");
    let src_dir = manifest_dir.join("src");
    if cargo_toml.is_file() && src_dir.is_dir() {
        return Some(manifest_dir.to_path_buf());
    }

    None
}

fn running_from_release_executable() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|path| {
            path.parent()
                .and_then(|parent| parent.file_name())
                .and_then(|name| name.to_str())
                .map(|name| name.eq_ignore_ascii_case("release"))
        })
        .unwrap_or(false)
}

fn find_generated_runtime_archive(cargo_target_dir: &Path, profile: &str) -> Option<PathBuf> {
    let profile_dir = cargo_target_dir.join(profile);
    for name in runtime_archive_names() {
        let direct = profile_dir.join(name);
        if direct.is_file() {
            return Some(direct);
        }
    }

    let deps = profile_dir.join("deps");
    if deps.is_dir() {
        let mut matches = std::fs::read_dir(&deps)
            .ok()?
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| {
                let Some(file_name) = path.file_name().and_then(OsStr::to_str) else {
                    return false;
                };
                runtime_archive_names().iter().any(|suffix| {
                    file_name.eq_ignore_ascii_case(suffix)
                        || file_name.ends_with(&format!("-{}", suffix))
                })
            })
            .collect::<Vec<_>>();
        matches.sort();
        if let Some(path) = matches.pop() {
            return Some(path);
        }
    }

    None
}

fn materialize_runtime_archive_object(archive_path: &Path) -> Result<PathBuf> {
    let bytes = std::fs::read(archive_path)
        .with_context(|| format!("failed to read {}", archive_path.display()))?;
    if bytes.is_empty() {
        bail!("runtime archive {} is empty", archive_path.display());
    }
    if !bytes.starts_with(b"!<arch>\n") {
        bail!(
            "runtime archive {} is not a valid static archive",
            archive_path.display()
        );
    }

    let digest = format!("{:x}", Sha256::digest(&bytes));
    let target = crate::linker::toolchain::TargetTriple::resolve(None);
    let cache_dir = crate::linker::toolchain::toolchains_base_dir()?
        .join(RUNTIME_OBJECTS_TOOL_NAME)
        .join(target.triple)
        .join(digest);
    std::fs::create_dir_all(&cache_dir)
        .with_context(|| format!("failed to create {}", cache_dir.display()))?;

    let object_path = cache_dir.join("runtime_support.o");
    if !object_path.is_file() {
        std::fs::write(&object_path, &bytes)
            .with_context(|| format!("failed to write {}", object_path.display()))?;
    }
    Ok(object_path)
}

fn purge_runtime_archives(cargo_target_dir: &Path) -> Result<()> {
    if !cargo_target_dir.is_dir() {
        return Ok(());
    }

    purge_runtime_archives_recursively(cargo_target_dir)
}

fn purge_runtime_archives_recursively(dir: &Path) -> Result<()> {
    let entries =
        std::fs::read_dir(dir).with_context(|| format!("failed to list {}", dir.display()))?;
    for entry in entries {
        let entry = entry.with_context(|| format!("failed to read entry in {}", dir.display()))?;
        let path = entry.path();
        if path.is_dir() {
            purge_runtime_archives_recursively(&path)?;
            continue;
        }

        let Some(file_name) = path.file_name().and_then(OsStr::to_str) else {
            continue;
        };
        if runtime_archive_names().iter().any(|name| {
            file_name.eq_ignore_ascii_case(name) || file_name.ends_with(&format!("-{name}"))
        }) {
            let _ = std::fs::remove_file(&path);
        }
    }

    Ok(())
}

fn runtime_archive_names() -> &'static [&'static str] {
    if cfg!(target_os = "windows") {
        &["rts.lib", "librts.lib"]
    } else {
        &["librts.a", "rts.a"]
    }
}
