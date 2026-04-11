pub mod apis;
pub mod compile;
pub mod eval;
pub mod init;
pub mod repl;
pub mod run;
pub mod test;

use anyhow::{Result, anyhow};

use crate::compile_options::{CompilationProfile, CompileOptions, FrontendMode};

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
    let raw_args = args.collect::<Vec<_>>();

    let (flags, positional, eval_source) = parse_flags(raw_args)?;

    if let Some(source) = eval_source {
        let result = eval::command(Some(source), flags.as_compile_options());
        return result.map_err(|error| render_compiler_error(error, flags));
    }

    if positional.is_empty() {
        print_help(&bin_name);
        return Ok(());
    }

    let result = match positional[0].as_str() {
        "compile" => compile::command(
            positional.get(1).cloned(),
            positional.get(2).cloned(),
            flags.as_compile_options(),
        ),
        "run" => run::command(positional.get(1).cloned(), flags.as_compile_options()),
        "test" => test::command(positional.get(1).cloned(), flags.as_compile_options()),
        "repl" => repl::command(),
        "init" => init::command(positional.get(1).cloned()),
        "apis" | "api" => apis::command(),
        "emit-types" => {
            let output_dir = positional.get(1).cloned()
                .unwrap_or_else(|| "packages/rts-types".to_string());
            let dir = std::path::Path::new(&output_dir);
            crate::namespaces::emit_split_typescript_declarations(dir)?;
            crate::namespaces::emit_typescript_declarations(
                &dir.join("rts.d.ts"),
            )?;
            println!("types emitted to {output_dir}");
            Ok(())
        }
        "help" => {
            print_help(&bin_name);
            Ok(())
        }
        entry => run::command(Some(entry.to_string()), flags.as_compile_options()),
    };

    result.map_err(|error| render_compiler_error(error, flags))
}

fn parse_flags(raw_args: Vec<String>) -> Result<(CliFlags, Vec<String>, Option<String>)> {
    let mut flags = CliFlags::default();
    let mut positional = Vec::new();
    let mut eval_source = None::<String>;
    let mut index = 0usize;

    while index < raw_args.len() {
        let arg = &raw_args[index];
        match arg.as_str() {
            "--development" | "-d" => flags.profile = CompilationProfile::Development,
            "--production" | "-p" => flags.profile = CompilationProfile::Production,
            "--debug" | "-D" => flags.debug = true,
            "--native" => flags.frontend_mode = FrontendMode::Native,
            "--compat" => flags.frontend_mode = FrontendMode::Compat,
            "--eval" | "-e" => {
                if eval_source.is_some() {
                    return Err(anyhow!("option '-e/--eval' can only be provided once"));
                }

                index += 1;
                let Some(source) = raw_args.get(index) else {
                    return Err(anyhow!("missing source for '-e/--eval'"));
                };
                eval_source = Some(source.clone());
            }
            _ if arg.starts_with('-') => bail_unknown_option(arg)?,
            _ => positional.push(arg.clone()),
        }
        index += 1;
    }

    Ok((flags, positional, eval_source))
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
    println!(
        "  {bin_name} -e|--eval <code> [--development|-d] [--production|-p] [--debug|-D] [--native|--compat]"
    );
    println!(
        "  {bin_name} [--development|-d] [--production|-p] [--debug|-D] [--native|--compat] <input.(rts|ts|js)>"
    );
    println!(
        "  {bin_name} compile [--development|-d] [--production|-p] [--debug|-D] [--native|--compat] [input.(rts|ts|js)] [output]"
    );
    println!(
        "  {bin_name} run [--development|-d] [--production|-p] [--debug|-D] [--native|--compat] [input.(rts|ts|js)]"
    );
    println!("  {bin_name} test [path]");
    println!("  {bin_name} init [project-name]");
    println!("  {bin_name} apis");
    println!("  {bin_name} repl");
}

#[cfg(test)]
mod tests {
    use super::parse_flags;

    #[test]
    fn parse_eval_flag_extracts_source() {
        let (flags, positional, eval_source) = parse_flags(vec![
            "--compat".to_string(),
            "-e".to_string(),
            "const valor = 42;".to_string(),
        ])
        .expect("flags should parse");

        assert!(matches!(
            flags.frontend_mode,
            crate::compile_options::FrontendMode::Compat
        ));
        assert!(positional.is_empty());
        assert_eq!(eval_source.as_deref(), Some("const valor = 42;"));
    }

    #[test]
    fn parse_eval_flag_requires_source() {
        let error = parse_flags(vec!["-e".to_string()]).expect_err("must fail");
        assert!(error.to_string().contains("missing source for '-e/--eval'"));
    }

    #[test]
    fn parse_regular_positional_without_eval() {
        let (_flags, positional, eval_source) =
            parse_flags(vec!["run".to_string()]).expect("flags should parse");
        assert_eq!(positional, vec!["run".to_string()]);
        assert!(eval_source.is_none());
    }
}
