//! `rts compile` — AOT native build on the NEW engine.
//!
//! The new engine shares its whole front-end + lowering between JIT and AOT
//! (design doc pilar 5); only the Cranelift `Module` backend differs. This
//! command drives the AOT half: it lowers the program into an `ObjectModule`
//! (with a synthesized `main` entry) and writes the resulting relocatable object
//! (`<output>.o`).
//!
//! LINK STEP: the emitted object is linked against the embedded runtime-support
//! archive (the `rts-runtime` staticlib, which also carries the `__rtsadp_*`
//! value-model adapter symbols since the `rts-adapters` crate folded into it) +
//! the system CRT via
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
    // An http(s) URL entry: mirror it (plus its relative-import graph) into the
    // system temp dir and compile the LOCAL copy. Default outputs must NOT land
    // in the temp mirror — they derive from the URL's file name in the cwd.
    let (entry, output_base) = if crate::url_entry::is_url(&input) {
        let local = crate::url_entry::fetch_program(&input)?;
        let name = local
            .file_name()
            .map(PathBuf::from)
            .ok_or_else(|| anyhow!("URL entry has no file name: {input}"))?;
        (local, name)
    } else {
        let entry = PathBuf::from(&input);
        let base = entry.clone();
        (entry, base)
    };
    if !entry.exists() {
        return Err(anyhow!("input file not found: {}", entry.display()));
    }

    // Lower the program into a native object (single shared lowering path; the
    // backend is an `ObjectModule` instead of the JIT). An `Unsupported` bail
    // here means the program steps outside the new engine's implemented subset —
    // surfaced verbatim, never a silent miscompile.
    let object_bytes = rts_codegen_new::front::run::compile_path_to_object(&entry)
        .map_err(|e| anyhow!("compile: {e}"))?;

    let obj_path = object_output_path(output.as_deref(), &output_base);
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
    let exe_path = exe_output_path(output.as_deref(), &output_base);

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
/// Windows) or `<stem>` next to the fallback base (the input path, or the URL's
/// bare file name in the cwd for a URL entry).
fn exe_output_path(output: Option<&str>, fallback: &Path) -> PathBuf {
    let base = match output {
        Some(o) => PathBuf::from(o),
        None => fallback.with_extension(""),
    };
    if cfg!(target_os = "windows") {
        base.with_extension("exe")
    } else {
        base
    }
}

/// Derive the object-file path. With an explicit `output`, use it as a base
/// (stripping a trailing executable extension) + the platform object suffix;
/// otherwise sit next to the fallback base as `<stem>.o`/`.obj` (the input path,
/// or the URL's bare file name in the cwd for a URL entry).
fn object_output_path(output: Option<&str>, fallback: &Path) -> PathBuf {
    let ext = if cfg!(target_os = "windows") { "obj" } else { "o" };
    match output {
        Some(o) => {
            let base = PathBuf::from(o);
            base.with_extension(ext)
        }
        None => fallback.with_extension(ext),
    }
}
