//! `rts compile` — AOT native build on the NEW engine.
//!
//! The new engine shares its whole front-end + lowering between JIT and AOT
//! (design doc pilar 5); only the Cranelift `Module` backend differs. This
//! command drives the AOT half: it lowers the program into an `ObjectModule`
//! (with a synthesized `main` entry) and writes the resulting relocatable object
//! (`<output>.o`).
//!
//! LINK STEP: the emitted object is linked against the embedded runtime-support
//! archive (now the `rts-adapters` staticlib — a SUPERSET of `rts-runtime` that
//! also carries the `__rtsadp_*` adapter symbols) + the system CRT via
//! `rts-linker`, producing a standalone native executable. The archive is located
//! through the bin-installed resolver ([`super::runtime_archive`]).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};

use crate::compile_options::CompileOptions;
use crate::linker::{LinkRequest, WindowsSubsystem, link_objects_to_binary_with_request};

pub fn command(
    input: Option<String>,
    output: Option<String>,
    options: CompileOptions,
    windows_subsystem: Option<WindowsSubsystem>,
) -> Result<()> {
    let input = input.ok_or_else(|| anyhow!("usage: rts compile <input.ts> [output]"))?;
    let entry = PathBuf::from(&input);
    if !entry.exists() {
        return Err(anyhow!("input file not found: {}", entry.display()));
    }

    // Lower the program into a native object (single shared lowering path; the
    // backend is an `ObjectModule` instead of the JIT). An `Unsupported` bail
    // here means the program steps outside the new engine's implemented subset —
    // surfaced verbatim, never a silent miscompile.
    let object_bytes = rts_codegen_new::front::run::compile_path_to_object(&entry)
        .map_err(|e| anyhow!("compile: {e}"))?;

    let obj_path = object_output_path(output.as_deref(), &entry);
    if let Some(parent) = obj_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create output dir {}", parent.display()))?;
        }
    }
    std::fs::write(&obj_path, &object_bytes)
        .with_context(|| format!("write object {}", obj_path.display()))?;

    // Locate the runtime-support archive (bin-owned embed) and link
    // [program.o, archive] into a standalone executable.
    let archive =
        super::runtime_archive().context("locate runtime-support archive for AOT link")?;
    let exe_path = exe_output_path(output.as_deref(), &entry);

    let mut request = LinkRequest::from_env();
    if windows_subsystem.is_some() {
        request.windows_subsystem = windows_subsystem;
    }
    request.keep_all_runtime_symbols = options.all_namespaces;

    let linked = link_objects_to_binary_with_request(
        &[obj_path.clone(), archive],
        &exe_path,
        &request,
    )
    .with_context(|| format!("link {} + runtime archive", obj_path.display()))?;

    println!(
        "rts compile: {} ({} bytes obj) -> {} [{}]",
        entry.display(),
        object_bytes.len(),
        linked.path.display(),
        linked.backend,
    );
    Ok(())
}

/// Derive the executable output path: explicit `output` (its `.exe` is added on
/// Windows) or `<input-stem>` next to the input.
fn exe_output_path(output: Option<&str>, entry: &Path) -> PathBuf {
    let base = match output {
        Some(o) => PathBuf::from(o),
        None => entry.with_extension(""),
    };
    if cfg!(target_os = "windows") {
        base.with_extension("exe")
    } else {
        base
    }
}

/// Derive the object-file path. With an explicit `output`, use it as a base
/// (stripping a trailing executable extension) + the platform object suffix;
/// otherwise sit next to the input as `<stem>.o`/`.obj`.
fn object_output_path(output: Option<&str>, entry: &Path) -> PathBuf {
    let ext = if cfg!(target_os = "windows") { "obj" } else { "o" };
    match output {
        Some(o) => {
            let base = PathBuf::from(o);
            base.with_extension(ext)
        }
        None => entry.with_extension(ext),
    }
}
