//! `rts compile` — AOT native build on the NEW engine (`rts-codegen` +
//! `rts-cranelift` + `rts-core-rwk` + `rts-host-rwk`).
//!
//! # Why this no longer calls `rts-codegen-new`
//!
//! `rts run`/`rts test` cut over to the new engine already; this command was
//! the one thing still calling `rts_codegen_new::front::run::compile_path_to_object`,
//! which meant an AOT binary and a JIT run of the SAME source could disagree —
//! and did, for anything the two engines answer differently, since they are not
//! the same engine. `rts-host-rwk::object` is the new engine's object-emission
//! path (increment 4 of this crate's AOT campaign) and `rts-runtime-rwk` is its
//! `staticlib` facade, so both preconditions this command was held back for now
//! exist.
//!
//! # What still needs a two-step build
//!
//! `cargo build -p rts-runtime-rwk` before `rts compile`, matching profile — the
//! same requirement the old engine's `rts-runtime` had, and for the same
//! reason: a `staticlib` is only emitted for a package built as a direct
//! target, and cargo does not do that as a side effect of depending on it. See
//! [`super::runtime_archive_rwk`] for the staleness check that makes skipping
//! this loud instead of silently linking last week's runtime.
//!
//! # What this does not do yet
//!
//! A relative import (`import "./x"`) needs the module-graph object path —
//! `rts_host_rwk::object` only compiles one file today, where `run.rs`'s
//! `compile_graph` has a memory counterpart and this does not yet. That is a
//! stated gap, not a silent one: this refuses a program that imports a file
//! rather than emitting an object missing a dependency's exports. See the AOT
//! campaign's final report for why it was left here.

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

    let source = std::fs::read_to_string(&entry)
        .with_context(|| format!("read {}", entry.display()))?;
    if imports_a_file(&source) {
        return Err(anyhow!(
            "rts compile: '{}' imports another file by a relative specifier — the \
             new engine's AOT path does not compile a module graph yet (single-file \
             programs only). `rts run`/`rts test` support this; `rts compile` does \
             not.",
            entry.display()
        ));
    }

    // The emitter recurses with the shape of the expression it lowers — see
    // `rts_cli::cli::new_engine`'s own comment for the fixture that overflows
    // the 1 MB default Windows stack at COMPILE time, not at run time. Compiled
    // on the same budget that engine uses for `run`/`test`, for the same reason.
    const STACK: usize = 64 * 1024 * 1024;
    let program = std::thread::Builder::new()
        .stack_size(STACK)
        .spawn(move || rts_host_rwk::object::compile_to_object(&source))
        .expect("a thread to compile the new engine's AOT object on")
        .join()
        .expect("the compile thread not to panic")
        .map_err(|e| anyhow!("compile: {e:?}"))?;

    let obj_path = object_output_path(output.as_deref(), &output_base);
    if let Some(parent) = obj_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create output dir {}", parent.display()))?;
        }
    }
    std::fs::write(&obj_path, &program.bytes)
        .with_context(|| format!("write object {}", obj_path.display()))?;

    let archive = super::runtime_archive_rwk()
        .context("locate the new engine's AOT runtime archive (rts-runtime-rwk)")?;
    let exe_path = exe_output_path(output.as_deref(), &output_base);

    let mut request = LinkRequest::from_env();
    if windows_subsystem.is_some() {
        request.windows_subsystem = windows_subsystem;
    }
    request.keep_all_runtime_symbols = options.all_namespaces;

    let linked = link_objects_to_binary_with_request(&[obj_path.clone(), archive], &exe_path, &request)
        .with_context(|| format!("link {} + runtime archive", obj_path.display()))?;

    // The sidecar `rts_host_rwk::object`'s module doc names: keys, literals,
    // template pieces and the singleton/kind numbering, read by the facade's
    // `main` before it calls the compiled entry. Written next to the FINAL exe
    // path, matching what `rts-runtime-rwk::aot::main` derives from
    // `current_exe()`.
    let manifest_path = linked.path.with_extension("rtsdata");
    rts_host_rwk::object::write_manifest(&manifest_path, &program)
        .with_context(|| format!("write {}", manifest_path.display()))?;

    println!(
        "rts compile: {} ({} bytes obj) -> {} [{}] + {}",
        entry.display(),
        program.bytes.len(),
        linked.path.display(),
        linked.backend,
        manifest_path.display(),
    );
    Ok(())
}

/// The same substring test `rts_cli::cli::new_engine::imports_a_file` and
/// `rts-host-rwk/examples/suite_run.rs` use — see that module's doc comment for
/// why a relative import is checked this way rather than by parsing.
fn imports_a_file(source: &str) -> bool {
    source.contains("from \"./")
        || source.contains("from \"../")
        || source.contains("from './")
        || source.contains("from '../")
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
