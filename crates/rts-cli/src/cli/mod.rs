//! Command-line entry point.

pub mod apis;
pub mod clean;
pub mod compile;
pub mod emit_types;
pub mod init;
pub mod install;
pub mod ir;
pub mod new_engine;
pub mod run;
pub mod test_cmd;

use std::path::PathBuf;
use std::sync::OnceLock;

use anyhow::{Context, Result, anyhow, bail};

use crate::compile_options::{CompilationProfile, CompileOptions, FrontendMode};
use crate::diagnostics::reporter;
use crate::linker::WindowsSubsystem;

/// Resolver for the AOT runtime-support archive (`<host-triple>.a`). The embedded
/// archive + its on-demand materialization live in the `rts` BIN crate
/// (`runtime_objects.rs`), which the CLI cannot reach upward. The bin installs its
/// `rt_artifacts` here at startup so `rts compile` can locate the archive to link.
static ARCHIVE_RESOLVER: OnceLock<fn() -> Result<PathBuf>> = OnceLock::new();

/// Install the runtime-archive resolver. Called once by the bin's `main` before
/// `dispatch`. No-op if already set.
pub fn set_runtime_archive_resolver(f: fn() -> Result<PathBuf>) {
    let _ = ARCHIVE_RESOLVER.set(f);
}

/// Resolve the runtime-support archive path. Errors if the bin never installed a
/// resolver (e.g. the CLI invoked as a library without `set_runtime_archive_resolver`).
pub(crate) fn runtime_archive() -> Result<PathBuf> {
    match ARCHIVE_RESOLVER.get() {
        Some(f) => f(),
        None => bail!(
            "runtime archive resolver not installed — the `rts` bin must call \
             `rts::cli::set_runtime_archive_resolver(rts::rt_artifacts)` before dispatch"
        ),
    }
}

/// Locates the `rts-runtime-rwk` staticlib `rts compile` links the NEW engine's
/// AOT objects against.
///
/// # Why this is not [`runtime_archive`]'s embed-and-decompress mechanism
///
/// That one exists to make a shipped `rts` binary self-contained: the old
/// engine's archive is baked into the executable at build time so a user never
/// runs a second build step. Reproducing that here — a `build.rs` step that
/// compresses a staticlib into `OUT_DIR` and decompresses it into
/// `~/.rts/artifacts` — is real engineering with no new decision to justify it,
/// and this crate's own rule (CLAUDE.md's iteration-speed rule) is to spend that
/// kind of time only where it earns something a cheaper mechanism cannot. A
/// path lookup with a LOUD staleness check earns the same correctness for a
/// fraction of the cost, and is what this is.
///
/// # The staleness check
///
/// Cargo happily links a `target/debug/rts_runtime_rwk.lib` that predates the
/// last edit to `rts-core-rwk`, `rts-std-rwk` or `rts-node-rwk` — nothing
/// rebuilds it just because `rts` itself was rebuilt, since it is not on this
/// binary's own dependency graph (that would make every `rts` invocation
/// rebuild a staticlib nothing but `rts compile` needs). So this compares the
/// archive's mtime against every `.rs` file in the three source trees and
/// refuses to link a stale one — the failure CLAUDE.md's "regress explicitly"
/// rule asks for: loud, and naming what to run, rather than a binary that links
/// and then answers a question the source no longer asks.
pub(crate) fn runtime_archive_rwk() -> Result<PathBuf> {
    if let Ok(path) = std::env::var("RTS_RUNTIME_RWK_ARCHIVE") {
        return Ok(PathBuf::from(path));
    }
    let workspace = std::env::current_dir().unwrap_or_default();
    let candidates = ["release", "debug"].map(|profile| {
        workspace
            .join("target")
            .join(profile)
            .join("rts_runtime_rwk.lib")
    });
    let archive = candidates
        .into_iter()
        .find(|path| path.is_file())
        .ok_or_else(|| {
            anyhow!(
                "no `rts_runtime_rwk.lib` under target/{{debug,release}} — build it first: \
                 `cargo build -p rts-runtime-rwk` (or `--release`). `rts compile` links the \
                 new engine's AOT objects against it and there is no embedded fallback."
            )
        })?;

    let archive_mtime = std::fs::metadata(&archive)
        .and_then(|meta| meta.modified())
        .with_context(|| format!("stat {}", archive.display()))?;

    for crate_name in ["rts-core-rwk", "rts-std-rwk", "rts-node-rwk", "rts-runtime-rwk"] {
        let source_dir = workspace.join("crates").join(crate_name).join("src");
        if let Some(stale) = newer_rust_file(&source_dir, archive_mtime)? {
            bail!(
                "'{}' is newer than {} — rebuild the AOT runtime archive: \
                 `cargo build -p rts-runtime-rwk` (or `--release` to match), then re-run \
                 `rts compile`. Cargo will not do this for you: the archive is not on \
                 `rts`'s own dependency graph.",
                stale.display(),
                archive.display()
            );
        }
    }
    Ok(archive)
}

/// The first `.rs` file under `dir` (recursively) newer than `than`, if any.
fn newer_rust_file(dir: &std::path::Path, than: std::time::SystemTime) -> Result<Option<PathBuf>> {
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().and_then(|ext| ext.to_str()) != Some("rs") {
                continue;
            }
            if let Ok(meta) = entry.metadata() {
                if let Ok(modified) = meta.modified() {
                    if modified > than {
                        return Ok(Some(path));
                    }
                }
            }
        }
    }
    Ok(None)
}

#[derive(Debug, Clone, Copy)]
struct CliFlags {
    profile: CompilationProfile,
    debug: bool,
    frontend_mode: FrontendMode,
    windows_subsystem: Option<WindowsSubsystem>,
    all_namespaces: bool,
}

impl Default for CliFlags {
    fn default() -> Self {
        Self {
            profile: CompilationProfile::Development,
            debug: false,
            frontend_mode: FrontendMode::Native,
            windows_subsystem: None,
            all_namespaces: false,
        }
    }
}

impl CliFlags {
    fn as_compile_options(self) -> CompileOptions {
        CompileOptions {
            profile: self.profile,
            debug: self.debug,
            frontend_mode: self.frontend_mode,
            emit_module_progress: false,
            all_namespaces: self.all_namespaces,
        }
    }
}

pub fn dispatch<I, S>(args: I) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut args = args.into_iter().map(Into::into);
    let bin_name = args.next().unwrap_or_else(|| "rts".to_string());
    let raw: Vec<String> = args.collect();
    // Hack: `-e <source>` e `--eval <source>` viram positional pra que
    // parse_flags nao rejeite o ponto inicial `-` do source (snippet TS
    // nao deveria comecar com `-` mas o flag parser nao distingue).
    // Alternativa: dispatcher dedicado pra eval ANTES de parse_flags.
    if raw.first().map(|s| s.as_str()) == Some("-e")
        || raw.first().map(|s| s.as_str()) == Some("--eval")
    {
        let source = raw.get(1).cloned();
        return run::eval_command(source, CompileOptions::default());
    }
    let (flags, positional) = parse_flags(raw)?;

    reporter::reset_global_engine();

    if positional.is_empty() {
        print_help(&bin_name);
        return Ok(());
    }

    match positional[0].as_str() {
        "compile" => compile::command(
            positional.get(1).cloned(),
            positional.get(2).cloned(),
            flags.as_compile_options(),
            flags.windows_subsystem,
        ),
        "run" => run::command(positional.get(1).cloned(), flags.as_compile_options()),
        "eval" | "-e" | "--eval" => run::eval_command(
            positional.get(1).cloned(),
            flags.as_compile_options(),
        ),
        "apis" | "api" => apis::command(),
        "init" => init::command(positional.get(1).cloned()),
        "clean" => clean::command(),
        "test" => test_cmd::command(positional.get(1).cloned()),
        "emit-types" => emit_types::command(positional.get(1).cloned()),
        "ir" => ir::command(positional.get(1).cloned(), flags.as_compile_options()),
        "i" | "install" | "add" => {
            let extra: Vec<String> = positional[1..].to_vec();
            install::command(extra)
        }
        "help" => {
            print_help(&bin_name);
            Ok(())
        }
        other => {
            // Allow `rts <file.ts>` / `rts <https://…/file.ts>` as shorthand
            // for `rts run`.
            if other.ends_with(".ts")
                || other.ends_with(".js")
                || crate::url_entry::is_url(other)
            {
                return run::command(Some(other.to_string()), flags.as_compile_options());
            }
            bail!("unknown command: {other}");
        }
    }
}

fn parse_flags(raw: Vec<String>) -> Result<(CliFlags, Vec<String>)> {
    let mut flags = CliFlags::default();
    let mut positional = Vec::new();
    let mut idx = 0usize;
    while idx < raw.len() {
        let arg = &raw[idx];
        match arg.as_str() {
            "--development" | "-d" => flags.profile = CompilationProfile::Development,
            "--production" | "-p" => flags.profile = CompilationProfile::Production,
            "--dump-statistics" | "-ds" | "-sd" => flags.debug = true,
            "--native" => flags.frontend_mode = FrontendMode::Native,
            "--compat" => flags.frontend_mode = FrontendMode::Compat,
            "--all-namespaces" => flags.all_namespaces = true,
            "--windows-subsystem" => {
                let value = raw
                    .get(idx + 1)
                    .ok_or_else(|| anyhow!("missing value for --windows-subsystem"))?;
                if value.starts_with('-') {
                    return Err(anyhow!(
                        "invalid value for --windows-subsystem: {value} (expected console|windows)"
                    ));
                }
                let parsed = WindowsSubsystem::from_raw(&value.to_ascii_lowercase())
                    .ok_or_else(|| {
                        anyhow!(
                            "invalid value for --windows-subsystem: {value} (expected console|windows)"
                        )
                    })?;
                flags.windows_subsystem = Some(parsed);
                idx += 2;
                continue;
            }
            _ if arg.starts_with("--windows-subsystem=") => {
                let value = arg
                    .split_once('=')
                    .map(|(_, v)| v)
                    .unwrap_or_default()
                    .trim()
                    .to_ascii_lowercase();
                let parsed = WindowsSubsystem::from_raw(&value).ok_or_else(|| {
                    anyhow!(
                        "invalid value for --windows-subsystem: {} (expected console|windows)",
                        arg.split_once('=').map(|(_, v)| v).unwrap_or_default()
                    )
                })?;
                flags.windows_subsystem = Some(parsed);
            }
            _ if arg.starts_with('-') => return Err(anyhow!("unknown option: {arg}")),
            _ => positional.push(arg.clone()),
        }
        idx += 1;
    }

    Ok((flags, positional))
}

fn print_help(bin_name: &str) {
    println!("RTS compiler CLI");
    println!("Usage:");
    println!("  {bin_name} compile <input.ts> [output.o]");
    println!("  {bin_name} run <input.ts>");
    println!("  {bin_name} apis");
    println!("  {bin_name} init [name]");
    println!("  {bin_name} clean");
    println!("  {bin_name} test [path]");
    println!("  {bin_name} emit-types [output.d.ts]");
    println!("  {bin_name} ir <input.ts>          dump Cranelift IR to stderr (no execution)");
    println!("  {bin_name} i [pkg@version ...]   install packages from package.json or args");
    println!("  {bin_name} help");
    println!("Options:");
    println!("  --windows-subsystem <console|windows>   (compile) set PE subsystem on Windows");
    println!("  --all-namespaces                        (compile) keep all runtime symbols (needed for import(variable))");
}
