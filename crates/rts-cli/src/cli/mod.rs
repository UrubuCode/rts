//! Command-line entry point.

pub mod clean;
pub mod compile;
pub mod emit_types;
pub mod html_entry;
pub mod init;
pub mod install;
pub mod ir;
pub mod napi;
pub mod new_engine;
pub mod run;
pub mod test_cmd;

use std::path::{Path, PathBuf};
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

/// Locates the staticlib `rts compile` links the AOT object against —
/// `rts-runtime-jit` by default, `rts-runtime` for `--sem-compilador`/
/// `--no-compiler`. See [`CompileOptions::embed_compiler`]'s own doc for the
/// two paths and why the default carries a compiler.
///
/// # `target/` is preferred, the embedded copy is the fallback
///
/// A dev iterating on `rts-core`/`rts-std`/`rts-node` needs their
/// freshly-built archive, not whatever shipped inside this `rts` binary — so a
/// `target/{debug,release}/<archive>` on disk always wins when present.
///
/// The embedded, extract-on-demand fallback
/// ([`set_runtime_archive_resolver`]) exists for a `rts` copied to a machine
/// with no `target/` at all, and it covers `rts-runtime` ONLY — build.rs
/// never embeds `rts-runtime-jit`, which carries a whole compiler and would
/// roughly double what every downloaded `rts` binary weighs for a capability
/// most compiled programs never use. **The cost this leaves, stated rather
/// than hidden:** a `rts` copied without its `target/` compiles by DEFAULT
/// now, so a plain `rts compile` with no flags on such a binary refuses
/// outright — where before this default flipped, the embedded fallback
/// covered the (then-default) small archive and the command worked. The
/// fix, for that one case, is naming the opt-out: `rts compile --sem-
/// compilador input.ts`.
///
/// # The staleness check
///
/// Cargo happily links a `target/debug/rts_runtime.lib` that predates the
/// last edit to `rts-core`, `rts-std` or `rts-node` — nothing
/// rebuilds it just because `rts` itself was rebuilt, since (for the `target/`
/// case) it was built by a separate `cargo build -p rts-runtime` invocation,
/// not as part of this binary's own dependency graph. So this compares the
/// archive's mtime against every `.rs` file in the source trees it carries and
/// refuses to link a stale one — the failure CLAUDE.md's "regress explicitly"
/// rule asks for: loud, and naming what to run, rather than a binary that links
/// and then answers a question the source no longer asks. Both archives check
/// `rts-runtime-boot`, the crate that actually carries the startup sequence
/// now (see its own module doc for why `rts-runtime` and `rts-runtime-jit`
/// are each a thin `main` over it). `--embed-compiler` additionally checks
/// `rts-codegen`, `rts-cranelift` and `rts-host`: those are the crates
/// `rts-runtime-jit` adds, and a stale answer FROM the compiler it carries —
/// `eval`ing a construct `rts-codegen` gained since the archive was built — is
/// the same silent-wrong-answer shape the existing check exists to refuse.
///
/// This check does not apply to the embedded fallback: `rts-runtime` is now
/// a direct dependency of the `rts` bin crate (root `Cargo.toml`), so the
/// archive `build.rs` embeds was necessarily built in the SAME `cargo build`
/// invocation that produced this very binary — there is no separate source tree
/// for it to be stale against. An embedded archive is only ever as stale as the
/// `rts` executable running it.
pub(crate) fn runtime_archive(embed_compiler: bool) -> Result<PathBuf> {
    if let Ok(path) = std::env::var("RTS_RUNTIME_RWK_ARCHIVE") {
        return Ok(PathBuf::from(path));
    }
    // O nome do `staticlib` é da PLATAFORMA, não uma constante: cargo escreve
    // `rts_runtime_jit.lib` no Windows e `librts_runtime_jit.a` em todo o
    // resto. Estavam aqui as duas strings do Windows, e o erro não aparecia
    // porque o caminho SEM compilador tem um fallback embutido que respondia
    // por ele — só `--embed-compiler`, que deliberadamente não tem fallback,
    // é que ficava sem archive. Desde #2681 esse é o caminho por omissão, e o
    // CI passou a falhar em Linux e macOS enquanto o Windows continuava verde.
    let stem = if embed_compiler { "rts_runtime_jit" } else { "rts_runtime" };
    let file_name = if cfg!(target_os = "windows") {
        format!("{stem}.lib")
    } else {
        format!("lib{stem}.a")
    };
    let file_name = file_name.as_str();
    let workspace = std::env::current_dir().unwrap_or_default();
    // The profile the RUNNING `rts` was built under comes first, and it is what
    // this used to have no way of naming: the list was `["release", "debug"]`,
    // two hardcoded strings, so a perfectly good `target/fast/rts_runtime.lib`
    // was invisible and `rts compile` fell through to the embedded copy — which
    // is a placeholder unless that binary's own build found a staticlib. That
    // is the whole of what "AOT does not work from `--profile fast`" was.
    //
    // Two mechanisms, in this order, and the FIRST is the one that answers the
    // question properly: "which archive matches this binary" is answered beside
    // it, because `target/<profile>/rts.exe` and
    // `target/<profile>/rts_runtime.lib` are built by the same two commands.
    // A binary living anywhere else — installed, downloaded — simply has no
    // archive next to it.
    //
    // The named profiles stay as the fallback, with `fast` added to them, for
    // the case the first cannot reach: an `rts` invoked from outside a
    // `target/` directory while the workspace it belongs to is the cwd.
    let beside_this_binary = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join(file_name)));
    let candidates =
        ["release", "debug", "fast"].map(|profile| workspace.join("target").join(profile).join(file_name));
    let dev_archive = beside_this_binary
        .into_iter()
        .chain(candidates)
        .find(|path| path.is_file());

    let Some(archive) = dev_archive else {
        if embed_compiler {
            // No embedded fallback for this one: nothing extracts a
            // never-built `rts_runtime_jit.lib` out of a binary that was
            // compiled without it, and pretending the default archive would
            // do would silently drop the one thing `--embed-compiler` asked
            // for.
            bail!(
                "no `{file_name}` under target/{{debug,release,fast}} nor beside this binary — build it first: \
                 `cargo build -p rts-runtime-jit` (or `--release`), then re-run `rts compile --embed-compiler`."
            );
        }
        return match ARCHIVE_RESOLVER.get() {
            Some(f) => f().context(
                "no `rts_runtime.lib` under target/{debug,release,fast} nor beside this binary and the embedded \
                 new-engine runtime archive could not be materialized",
            ),
            None => bail!(
                "no `rts_runtime.lib` under target/{{debug,release,fast}} nor beside this binary — build it first: \
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

    // `rts-runtime-boot` carries the ACTUAL startup sequence for both
    // archives — `rts-runtime` and `rts-runtime-jit` are now each a `main`
    // wrapper over it — so it is checked either way.
    let mut crates = vec!["rts-core", "rts-std", "rts-node", "rts-runtime-boot", "rts-runtime"];
    if embed_compiler {
        crates.extend(["rts-codegen", "rts-cranelift", "rts-host", "rts-runtime-jit"]);
    }
    let package = if embed_compiler { "rts-runtime-jit" } else { "rts-runtime" };
    for crate_name in crates {
        let source_dir = workspace.join("crates").join(crate_name).join("src");
        if let Some(stale) = newer_rust_file(&source_dir, archive_mtime)? {
            bail!(
                "'{}' is newer than {} — rebuild the AOT runtime archive: `cargo build -p {package}` \
                 (or `--release` to match), then re-run `rts compile`. Cargo will not do this \
                 for you: the archive is not on `rts`'s own dependency graph.",
                stale.display(),
                archive.display(),
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

#[derive(Debug, Clone)]
struct CliFlags {
    profile: CompilationProfile,
    debug: bool,
    windows_subsystem: Option<WindowsSubsystem>,
    all_namespaces: bool,
    // `true` by default — see `CompileOptions::embed_compiler`'s own doc for
    // why: `rts compile` carries a compiler unless `--sem-compilador`/
    // `--no-compiler` asks for the small archive instead.
    embed_compiler: bool,
    /// `--html <file>`, repeatable, in the order given — `compile`'s own
    /// question, and the reason this struct is no longer `Copy`: a page's
    /// `<script>`s belong to the ONE command that precompiles them, not to
    /// `CompileOptions`, which every other command shares unchanged.
    html: Vec<String>,
}

impl Default for CliFlags {
    fn default() -> Self {
        Self {
            profile: CompilationProfile::Development,
            debug: false,
            windows_subsystem: None,
            all_namespaces: false,
            embed_compiler: true,
            html: Vec::new(),
        }
    }
}

impl CliFlags {
    fn as_compile_options(&self) -> CompileOptions {
        CompileOptions {
            profile: self.profile,
            debug: self.debug,
            emit_module_progress: false,
            all_namespaces: self.all_namespaces,
            embed_compiler: self.embed_compiler,
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
            &flags.html,
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
        "napi" => napi::command(positional.get(1).cloned()),
        "i" | "install" | "add" => {
            let extra: Vec<String> = positional[1..].to_vec();
            install::command(extra)
        }
        "help" => {
            print_help(&bin_name);
            Ok(())
        }
        other => {
            // Allow `rts <file.ts>` / `rts <file.html>` / `rts
            // <https://…/file.ts>` as shorthand for `rts run` — a `.html`
            // entry takes `cli::html_entry`'s shell rather than compiling the
            // page itself as TypeScript, exactly as `rts run <file.html>`
            // does below.
            if other.ends_with(".ts")
                || other.ends_with(".js")
                || crate::cli::html_entry::is_html(Path::new(other))
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
            // The default already embeds a compiler — kept as an explicit,
            // accepted synonym of it rather than removed, so a caller (this
            // repo's own CI included) that already passes it sees no change.
            "--embed-compiler" => flags.embed_compiler = true,
            // The opt-out: the small archive, for a binary that never
            // `eval`s and never runs a page `<script>` at run time. Two
            // spellings for the same reason `-p`/`--production` has two:
            // whichever a caller already reaches for.
            "--sem-compilador" | "--no-compiler" => flags.embed_compiler = false,
            "--html" => {
                let value = raw
                    .get(idx + 1)
                    .ok_or_else(|| anyhow!("missing value for --html"))?;
                if value.starts_with('-') {
                    return Err(anyhow!("invalid value for --html: {value} (expected a path)"));
                }
                flags.html.push(value.clone());
                idx += 2;
                continue;
            }
            _ if arg.starts_with("--html=") => {
                let value = arg.split_once('=').map(|(_, v)| v).unwrap_or_default();
                flags.html.push(value.to_owned());
            }
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
    println!("  {bin_name} compile <input.ts|input.html> [output.o]");
    println!("  {bin_name} run <input.ts|input.html>");
    println!("  {bin_name} init [name]");
    println!("  {bin_name} clean");
    println!("  {bin_name} test [path]");
    println!("  {bin_name} emit-types [output.d.ts]");
    println!("  {bin_name} ir <input.ts>          dump this engine's own IR to stderr (no execution)");
    println!("  {bin_name} i [pkg@version ...]   install packages from package.json or args");
    println!("  {bin_name} help");
    println!("Options:");
    println!("  --windows-subsystem <console|windows>   (compile) set PE subsystem on Windows");
    println!("  --all-namespaces                        (compile) keep all runtime symbols (needed for import(variable))");
    println!("  --embed-compiler                        (compile) DEFAULT — synonym; the .exe carries a compiler, so eval/new Function/page <script> work at run time");
    println!("  --sem-compilador, --no-compiler          (compile) opt out — link the small archive; refuses eval/new Function/page <script> at run time");
    println!("  --html <file>                           (compile) precompile this page's <script> tags into the binary (repeatable)");
    println!();
    println!("An `.html` entry needs no TypeScript at all: `{bin_name} compile pagina.html [out]` writes the");
    println!("app.ts-style window loop for you (parse+resources+scripts -> egui.openWindow -> per-frame");
    println!("render + input/event/timer pumps), with the page's HTML embedded as a build-time literal and");
    println!("its <script>s precompiled as if `--html pagina.html` had been passed. A relative <link>/<img>");
    println!("resolves against pagina.html's OWN folder as it exists on THIS machine at build time, not at");
    println!("run time — moving the .exe elsewhere loses those. `<script src=\"http…\">` never enters either");
    println!("way — fetched by a page loader, never by this compiler. `{bin_name} run pagina.html` runs the");
    println!("same loop in JIT, reading the page from disk each time instead of embedding it.");
}
