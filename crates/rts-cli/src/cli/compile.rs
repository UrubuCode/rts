//! `rts compile` — AOT native build on the NEW engine.
//!
//! The new engine shares its whole front-end + lowering between JIT and AOT
//! (design doc pilar 5); only the Cranelift `Module` backend differs. This
//! command drives the AOT half: it lowers the program into an `ObjectModule`
//! (with a synthesized `main` entry) and writes the resulting relocatable object
//! (`<output>.o`).
//!
//! LINK STEP (pending): turning the `.o` into a standalone executable still needs
//! a combined runtime archive — the embedded `runtime_support.a` is built from
//! `rts-runtime` ONLY and so lacks the `__rtsadp_*` adapter symbols that now live
//! in the `rts-adapters` staticlib. Linking therefore requires (a) the build to
//! bundle `rts-adapters` into the runtime archive and (b) the bin→cli plumbing to
//! hand both the archive and the CRT/system libs to `rts-linker`. Until then this
//! emits the object and reports the precise remaining step rather than producing
//! a half-linked binary (honesty floor: no crash/partial passed off as done).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};

use crate::compile_options::CompileOptions;
use crate::linker::WindowsSubsystem;

pub fn command(
    input: Option<String>,
    output: Option<String>,
    _options: CompileOptions,
    _windows_subsystem: Option<WindowsSubsystem>,
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

    println!(
        "rts compile: emitted native object {} ({} bytes).",
        obj_path.display(),
        object_bytes.len()
    );
    println!(
        "note: native linking to a standalone executable is pending — the runtime \
         archive must bundle the `rts-adapters` staticlib (__rtsadp_* symbols). \
         The object links against `rts-runtime` + `rts-adapters` + the system CRT."
    );
    Ok(())
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
