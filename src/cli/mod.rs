//! Command-line entry point.

pub mod apis;
pub mod clean;
pub mod compile;
pub mod init;
pub mod run;

use anyhow::{Result, anyhow, bail};

use crate::compile_options::{CompilationProfile, CompileOptions, FrontendMode};
use crate::diagnostics::reporter;

#[derive(Debug, Clone, Copy)]
struct CliFlags {
    profile: CompilationProfile,
    debug: bool,
    frontend_mode: FrontendMode,
}

impl Default for CliFlags {
    fn default() -> Self {
        Self {
            profile: CompilationProfile::Development,
            debug: false,
            frontend_mode: FrontendMode::Native,
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
        ),
        "run" => run::command(positional.get(1).cloned(), flags.as_compile_options()),
        "apis" | "api" => apis::command(),
        "init" => init::command(positional.get(1).cloned()),
        "clean" => clean::command(),
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

    for arg in raw {
        match arg.as_str() {
            "--development" | "-d" => flags.profile = CompilationProfile::Development,
            "--production" | "-p" => flags.profile = CompilationProfile::Production,
            "--dump-statistics" | "-ds" | "-sd" => flags.debug = true,
            "--native" => flags.frontend_mode = FrontendMode::Native,
            "--compat" => flags.frontend_mode = FrontendMode::Compat,
            _ if arg.starts_with('-') => return Err(anyhow!("unknown option: {arg}")),
            _ => positional.push(arg),
        }
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
    println!("  {bin_name} help");
}
