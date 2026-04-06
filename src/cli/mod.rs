pub mod apis;
pub mod build;
pub mod init;
pub mod repl;
pub mod run;

use anyhow::{Result, anyhow};

use crate::compile_options::{CompilationProfile, CompileOptions};

#[derive(Debug, Clone, Copy)]
struct CliFlags {
    profile: CompilationProfile,
    debug: bool,
}

impl Default for CliFlags {
    fn default() -> Self {
        Self {
            profile: CompilationProfile::Development,
            debug: false,
        }
    }
}

impl CliFlags {
    fn as_compile_options(self) -> CompileOptions {
        CompileOptions {
            profile: self.profile,
            debug: self.debug,
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
    let raw_args = args.collect::<Vec<_>>();

    let (flags, positional) = parse_flags(raw_args)?;

    if positional.is_empty() {
        print_help(&bin_name);
        return Ok(());
    }

    let result = match positional[0].as_str() {
        "build" => build::command(
            positional.get(1).cloned(),
            positional.get(2).cloned(),
            flags.as_compile_options(),
        ),
        "run" => run::command(positional.get(1).cloned(), flags.as_compile_options()),
        "repl" => repl::command(),
        "init" => init::command(positional.get(1).cloned()),
        "apis" | "api" => apis::command(),
        "help" => {
            print_help(&bin_name);
            Ok(())
        }
        entry => run::command(Some(entry.to_string()), flags.as_compile_options()),
    };

    result.map_err(|error| render_compiler_error(error, flags))
}

fn parse_flags(raw_args: Vec<String>) -> Result<(CliFlags, Vec<String>)> {
    let mut flags = CliFlags::default();
    let mut positional = Vec::new();

    for arg in raw_args {
        match arg.as_str() {
            "--development" | "-d" => flags.profile = CompilationProfile::Development,
            "--production" | "-p" => flags.profile = CompilationProfile::Production,
            "--debug" | "-D" => flags.debug = true,
            _ if arg.starts_with('-') => bail_unknown_option(&arg)?,
            _ => positional.push(arg),
        }
    }

    Ok((flags, positional))
}

fn bail_unknown_option(option: &str) -> Result<()> {
    Err(anyhow!("unknown option: {option}"))
}

fn render_compiler_error(error: anyhow::Error, flags: CliFlags) -> anyhow::Error {
    if matches!(flags.profile, CompilationProfile::Production) && !flags.debug {
        let fingerprint = fnv1a32(format!("{error:#}").as_bytes());
        return anyhow!(
            "compiler error [RTS{:08X}] (use --development or --debug for trace route)",
            fingerprint
        );
    }

    let mut rendered = format!("compiler error ({})", flags.profile);
    rendered.push_str("\nTrace route:");
    for cause in error.chain() {
        rendered.push_str("\n  - ");
        rendered.push_str(&cause.to_string());
    }

    if flags.debug {
        rendered.push_str("\nDebug detail:");
        rendered.push_str(&format!("\n{error:?}"));
    }

    anyhow!(rendered)
}

fn fnv1a32(input: &[u8]) -> u32 {
    let mut hash: u32 = 0x811C9DC5;
    for byte in input {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
}

fn print_help(bin_name: &str) {
    println!("RTS compiler bootstrap CLI");
    println!("Usage:");
    println!("  {bin_name} [--development|-d] [--production|-p] [--debug|-D] <input.(rts|ts)>");
    println!(
        "  {bin_name} build [--development|-d] [--production|-p] [--debug|-D] [input.(rts|ts)] [output]"
    );
    println!("  {bin_name} run [--development|-d] [--production|-p] [--debug|-D] [input.(rts|ts)]");
    println!("  {bin_name} init [project-name]");
    println!("  {bin_name} apis");
    println!("  {bin_name} repl");
}
