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
            let requires_runtime = requires_windows_runtime_support(object_paths);

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
            // Bootstrap codegen emits a `main` symbol; the MSVC CRT entry
            // `mainCRTStartup` initialises the runtime and calls into it.
            args.push("/entry:mainCRTStartup".to_string());
            args.push("/subsystem:console".to_string());
            if !requires_runtime {
                args.push("/nodefaultlib".to_string());
            }
            args.push(format!("/out:{}", output_path.display()));
            for object_path in object_paths {
                args.push(object_path.display().to_string());
            }

            if requires_runtime {
                for path in windows_runtime_lib_paths() {
                    args.push(format!("/libpath:{}", path.display()));
                }
                for lib in windows_runtime_default_libs() {
                    args.push(format!("/defaultlib:{lib}"));
                }
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

fn requires_windows_runtime_support(object_paths: &[PathBuf]) -> bool {
    object_paths.iter().any(|path| {
        path.extension()
            .and_then(|value| value.to_str())
            .map(|value| value.eq_ignore_ascii_case("lib"))
            .unwrap_or(false)
    })
}

fn windows_runtime_default_libs() -> &'static [&'static str] {
    &[
        "kernel32.lib",
        "user32.lib",
        "gdi32.lib",
        "oleaut32.lib",
        "userenv.lib",
        "advapi32.lib",
        "bcrypt.lib",
        "ws2_32.lib",
        "ntdll.lib",
        "shell32.lib",
        "ole32.lib",
        "synchronization.lib",
        // Rust staticlib on MSVC uses the dynamic CRT by default; keep only
        // the matching dynamic import libraries to avoid duplicate symbols
        // like `__report_gsfailure` that appear in both static and dynamic
        // variants.
        "ucrt.lib",
        "vcruntime.lib",
        "msvcrt.lib",
        "legacy_stdio_definitions.lib",
    ]
}

fn windows_runtime_lib_paths() -> Vec<PathBuf> {
    let mut paths = Vec::<PathBuf>::new();
    if let Ok(raw) = std::env::var("LIB") {
        for part in raw
            .split(';')
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            let candidate = PathBuf::from(part);
            if candidate.is_dir() {
                paths.push(candidate);
            }
        }
    }

    let sdk_root = std::env::var("WindowsSdkDir")
        .ok()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Program Files (x86)\Windows Kits\10"));
    let lib_root = sdk_root.join("Lib");
    if lib_root.is_dir() {
        let mut versions = std::fs::read_dir(&lib_root)
            .ok()
            .into_iter()
            .flatten()
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
            .collect::<Vec<_>>();
        versions.sort();
        if let Some(version) = versions.pop() {
            let arch = if cfg!(target_arch = "x86_64") {
                "x64"
            } else if cfg!(target_arch = "x86") {
                "x86"
            } else if cfg!(target_arch = "aarch64") {
                "arm64"
            } else {
                "x64"
            };

            let um = version.join("um").join(arch);
            if um.is_dir() {
                paths.push(um);
            }

            let ucrt = version.join("ucrt").join(arch);
            if ucrt.is_dir() {
                paths.push(ucrt);
            }
        }
    }

    // MSVC toolchain libs (libcmt.lib, msvcrt.lib, vcruntime.lib) live under
    // the MSVC install, separate from the Windows SDK. Auto-discover the
    // latest installed VC Tools directory so `rts compile` works without
    // requiring the Developer Command Prompt environment.
    for msvc_path in msvc_tool_lib_paths() {
        paths.push(msvc_path);
    }

    paths.sort();
    paths.dedup();
    paths
}

/// Walks common Visual Studio install roots looking for the MSVC `lib`
/// directory corresponding to the current architecture.
fn msvc_tool_lib_paths() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    for env in ["ProgramFiles", "ProgramFiles(x86)"] {
        if let Ok(base) = std::env::var(env) {
            roots.push(PathBuf::from(base).join("Microsoft Visual Studio"));
        }
    }
    roots.push(PathBuf::from(r"C:\Program Files\Microsoft Visual Studio"));
    roots.push(PathBuf::from(r"C:\Program Files (x86)\Microsoft Visual Studio"));

    let arch = if cfg!(target_arch = "x86_64") {
        "x64"
    } else if cfg!(target_arch = "x86") {
        "x86"
    } else if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "x64"
    };

    let mut out = Vec::new();
    for root in roots {
        if !root.is_dir() {
            continue;
        }
        // Layout: <root>/<year>/<edition>/VC/Tools/MSVC/<version>/lib/<arch>
        let Ok(years) = std::fs::read_dir(&root) else {
            continue;
        };
        for year in years.flatten() {
            let Ok(editions) = std::fs::read_dir(year.path()) else {
                continue;
            };
            for edition in editions.flatten() {
                let msvc = edition.path().join("VC").join("Tools").join("MSVC");
                if !msvc.is_dir() {
                    continue;
                }
                let Ok(versions) = std::fs::read_dir(&msvc) else {
                    continue;
                };
                let mut version_dirs: Vec<PathBuf> = versions
                    .flatten()
                    .map(|entry| entry.path())
                    .filter(|p| p.is_dir())
                    .collect();
                version_dirs.sort();
                if let Some(latest) = version_dirs.last() {
                    let lib = latest.join("lib").join(arch);
                    if lib.is_dir() {
                        out.push(lib);
                    }
                }
            }
        }
    }
    out
}

fn normalize_output_path(path: &Path, flavor: TargetFlavor) -> PathBuf {
    if matches!(flavor, TargetFlavor::Coff) && path.extension().is_none() {
        return path.with_extension("exe");
    }
    path.to_path_buf()
}
