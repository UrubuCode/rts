//! Command-line entry point.

pub mod apis;
pub mod clean;
pub mod compile;
pub mod emit_types;
pub mod init;
pub mod install;
pub mod ir;
pub mod run;
pub mod test_cmd;

use anyhow::{Result, anyhow, bail};

use crate::compile_options::{CompilationProfile, CompileOptions, FrontendMode};
use crate::diagnostics::reporter;
use crate::linker::WindowsSubsystem;

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
            // Allow `rts <file.ts>` as shorthand for `rts run`.
            if other.ends_with(".ts") || other.ends_with(".js") {
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
