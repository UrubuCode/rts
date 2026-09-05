//! `rts compile` — AOT native build on the NEW engine (`rts-codegen` +
//! `rts-cranelift` + `rts-core` + `rts-host`).
//!
//! # Why this no longer calls `rts-codegen-new`
//!
//! `rts run`/`rts test` cut over to the new engine already; this command was
//! the one thing still calling `rts_codegen_new::front::run::compile_path_to_object`,
//! which meant an AOT binary and a JIT run of the SAME source could disagree —
//! and did, for anything the two engines answer differently, since they are not
//! the same engine. `rts-host::object` is the new engine's object-emission
//! path (increment 4 of this crate's AOT campaign) and `rts-runtime` is its
//! `staticlib` facade, so both preconditions this command was held back for now
//! exist.
//!
//! # What still needs a two-step build
//!
//! `cargo build -p rts-runtime` before `rts compile`, matching profile — the
//! same requirement the old engine's `rts-runtime` had, and for the same
//! reason: a `staticlib` is only emitted for a package built as a direct
//! target, and cargo does not do that as a side effect of depending on it. See
//! [`super::runtime_archive`] for the staleness check that makes skipping
//! this loud instead of silently linking last week's runtime.
//!
//! # A module graph, and how this decides it has one
//!
//! A program that names another file is compiled as a GRAPH — every module of
//! it into one object, dependencies first — exactly as `rts run` and `rts test`
//! compile it. It used to be REFUSED here, because `rts_host::object` compiled
//! one file and an object missing a dependency's exports would have been worse
//! than a refusal; that gap is closed, and what closed it is in
//! `rts_host::object`'s own header.
//!
//! Which shape a file is is decided by [`super::new_engine::imports_a_file`],
//! and by nothing written here. This file used to carry its own copy, and
//! claimed in a comment that the two were "the same substring test" — they were
//! not: the copy here missed `require("./x")`, `import("./x")`, `import.meta`,
//! `module.exports` and `__filename`, every one of which names a file or needs
//! a specifier. So a CommonJS program reaching a sibling file was not refused
//! and not compiled as a graph either; it was compiled ALONE, and died at run
//! time on a name that was never bound. One question, one answer, one place.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};

use crate::compile_options::CompileOptions;
use crate::linker::{LinkRequest, WindowsSubsystem, link_objects_to_binary_with_request};

pub fn command(
    input: Option<String>,
    output: Option<String>,
    options: CompileOptions,
    windows_subsystem: Option<WindowsSubsystem>,
    html_files: &[String],
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
    let graph = super::new_engine::imports_a_file(&source);

    // Read and extracted on the SAME wide-stack thread as the compile below,
    // rather than on this one: `html_scripts::window_base` runs a throwaway
    // JIT compile of its own, and that is exactly the recursion depth the
    // comment on `STACK` names.
    let html_paths: Vec<PathBuf> = html_files.iter().map(PathBuf::from).collect();

    // The emitter recurses with the shape of the expression it lowers — see
    // `rts_cli::cli::new_engine`'s own comment for the fixture that overflows
    // the 1 MB default Windows stack at COMPILE time, not at run time. Compiled
    // on the same budget that engine uses for `run`/`test`, for the same reason.
    const STACK: usize = 64 * 1024 * 1024;
    let on_disk = entry.clone();
    let program = std::thread::Builder::new()
        .stack_size(STACK)
        .spawn(move || -> Result<rts_host::object::ObjectProgram, rts_host::HostError> {
            // Extracted here rather than passed in already-extracted: a page
            // `<script>` compiles under the SAME `Scoped::Page` rules as a
            // JIT run, which is what `object::page` builds on top of the
            // main program's own `FrontEnd` — see that module's header for
            // why the two cannot be two object files.
            let page_scripts = rts_host::object::html_scripts::extract_files(&html_paths)?;
            match (graph, page_scripts.is_empty()) {
                (true, true) => rts_host::object::compile_graph_to_object(&on_disk),
                (true, false) => {
                    rts_host::object::compile_graph_to_object_with_html(&on_disk, &page_scripts)
                }
                (false, true) => rts_host::object::compile_to_object(&source),
                (false, false) => {
                    rts_host::object::compile_to_object_with_html(&source, &page_scripts)
                }
            }
        })
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

    let archive = super::runtime_archive()
        .context("locate the new engine's AOT runtime archive (rts-runtime)")?;
    let exe_path = exe_output_path(output.as_deref(), &output_base);

    let mut request = LinkRequest::from_env();
    if windows_subsystem.is_some() {
        request.windows_subsystem = windows_subsystem;
    }
    request.keep_all_runtime_symbols = options.all_namespaces;

    let linked = link_objects_to_binary_with_request(&[obj_path.clone(), archive], &exe_path, &request)
        .with_context(|| format!("link {} + runtime archive", obj_path.display()))?;

    // The sidecar `rts_host::object`'s module doc names: keys, literals,
    // template pieces and the singleton/kind numbering, read by the facade's
    // `main` before it calls the compiled entry. Written next to the FINAL exe
    // path, matching what `rts-runtime::aot::main` derives from
    // `current_exe()`.
    let manifest_path = linked.path.with_extension("rtsdata");
    rts_host::object::write_manifest(&manifest_path, &program)
        .with_context(|| format!("write {}", manifest_path.display()))?;

    println!(
        "rts compile: {} ({} bytes obj) -> {} [{}] + {}",
        entry.display(),
        program.bytes.len(),
        linked.path.display(),
        linked.backend,
        manifest_path.display(),
    );
    if !program.page_scripts.is_empty() {
        println!(
            "  {} page <script>(s) precompiled from --html: {}",
            program.page_scripts.len(),
            html_files.join(", "),
        );
    }
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
