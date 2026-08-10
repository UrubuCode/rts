//! Command-line entry point.

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

use crate::compile_options::{CompilationProfile, CompileOptions};
use crate::linker::WindowsSubsystem;

/// Resolver for the embedded AOT archive (`rts-runtime`). The archive and
/// its on-demand materialization live in the `rts` BIN crate
/// (`runtime_objects.rs`), which the CLI cannot reach upward, so the bin
/// installs its `rt_artifacts` here at startup.
///
/// [`runtime_archive`] falls back to it only when no fresh `target/` build
/// exists, so a dev who just rebuilt `rts-runtime` gets their own archive
/// rather than a possibly-stale embedded one.
static ARCHIVE_RESOLVER: OnceLock<fn() -> Result<PathBuf>> = OnceLock::new();

/// Install the embedded-archive resolver (`rts::rt_artifacts`).
/// No-op if already set. A CLI invoked without calling this still works for
/// `rts compile` as long as a fresh `target/{debug,release}/rts_runtime.lib`
/// exists — this only supplies the fallback for a binary run from elsewhere.
pub fn set_runtime_archive_resolver(f: fn() -> Result<PathBuf>) {
    let _ = ARCHIVE_RESOLVER.set(f);
}

/// Locates the `rts-runtime` staticlib `rts compile` links the AOT objects
/// against.
///
/// # `target/` is preferred, the embedded copy is the fallback
///
/// A dev iterating on `rts-core`/`rts-std`/`rts-node` needs their
/// freshly-built archive, not whatever shipped inside this `rts` binary — so a
/// `target/{debug,release}/rts_runtime.lib` on disk always wins when
/// present. A `rts` copied to a machine with no `target/` at all (the case this
/// exists for: a downloaded binary that could not `rts compile` before this
/// change) falls back to [`set_runtime_archive_resolver`]'s embedded,
/// extract-on-demand archive.
///
/// # The staleness check
///
/// Cargo happily links a `target/debug/rts_runtime.lib` that predates the
/// last edit to `rts-core`, `rts-std` or `rts-node` — nothing
/// rebuilds it just because `rts` itself was rebuilt, since (for the `target/`
/// case) it was built by a separate `cargo build -p rts-runtime` invocation,
/// not as part of this binary's own dependency graph. So this compares the
/// archive's mtime against every `.rs` file in the three source trees and
/// refuses to link a stale one — the failure CLAUDE.md's "regress explicitly"
/// rule asks for: loud, and naming what to run, rather than a binary that links
/// and then answers a question the source no longer asks.
///
/// This check does not apply to the embedded fallback: `rts-runtime` is now
/// a direct dependency of the `rts` bin crate (root `Cargo.toml`), so the
/// archive `build.rs` embeds was necessarily built in the SAME `cargo build`
/// invocation that produced this very binary — there is no separate source tree
/// for it to be stale against. An embedded archive is only ever as stale as the
/// `rts` executable running it.
pub(crate) fn runtime_archive() -> Result<PathBuf> {
    if let Ok(path) = std::env::var("RTS_RUNTIME_RWK_ARCHIVE") {
        return Ok(PathBuf::from(path));
    }
    let workspace = std::env::current_dir().unwrap_or_default();
    let candidates = ["release", "debug"].map(|profile| {
        workspace
            .join("target")
            .join(profile)
            .join("rts_runtime.lib")
    });
    let dev_archive = candidates.into_iter().find(|path| path.is_file());

    let Some(archive) = dev_archive else {
        return match ARCHIVE_RESOLVER.get() {
            Some(f) => f().context(
                "no `rts_runtime.lib` under target/{debug,release} and the embedded \
                 new-engine runtime archive could not be materialized",
            ),
            None => bail!(
                "no `rts_runtime.lib` under target/{{debug,release}} — build it first: \
                 `cargo build -p rts-runtime` (or `--release`). No embedded-archive \
                 resolver was installed either (the `rts` bin must call \
                 `rts::cli::set_runtime_archive_resolver(rts::rt_artifacts)` before \
                 dispatch)."
            ),
        };
    };

    let archive_mtime = std::fs::metadata(&archive)
        .and_then(|meta| meta.modified())
        .with_context(|| format!("stat {}", archive.display()))?;

    for crate_name in ["rts-core", "rts-std", "rts-node", "rts-runtime"] {
        let source_dir = workspace.join("crates").join(crate_name).join("src");
        if let Some(stale) = newer_rust_file(&source_dir, archive_mtime)? {
            bail!(
                "'{}' is newer than {} — rebuild the AOT runtime archive: \
                 `cargo build -p rts-runtime` (or `--release` to match), then re-run \
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
    windows_subsystem: Option<WindowsSubsystem>,
    all_namespaces: bool,
}

impl Default for CliFlags {
    fn default() -> Self {
        Self {
            profile: CompilationProfile::Development,
            debug: false,
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
    // `rts run -e "…"` is the same request as `rts -e "…"`, and is what a user
    // coming from `node -e` / `bun -e` writes first. Recognised HERE, beside the
    // bare form, because both have to be caught before `parse_flags` sees the
    // snippet — it rejects a leading `-`, and the whole reason this block exists
    // is that it cannot tell a flag from source text.
    let eval_at = match raw.first().map(|s| s.as_str()) {
        Some("-e") | Some("--eval") => Some(0),
        Some("run") | Some("eval") => match raw.get(1).map(|s| s.as_str()) {
            Some("-e") | Some("--eval") => Some(1),
            _ => None,
        },
        _ => None,
    };
    if let Some(at) = eval_at {
        let source = raw.get(at + 1).cloned();
        return run::eval_command(source, CompileOptions::default());
    }
    let (flags, positional) = parse_flags(raw)?;


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
