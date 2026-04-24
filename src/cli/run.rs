//! `rts run <input.ts>` — compile + link to a temp binary and execute it.

use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, Result, anyhow};

use crate::compile_options::CompileOptions;
use crate::pipeline;

pub fn command(input: Option<String>, options: CompileOptions) -> Result<()> {
    let input = input.ok_or_else(|| anyhow!("usage: rts run <input.ts>"))?;
    let input_path = PathBuf::from(&input);
    if !input_path.exists() {
        return Err(anyhow!("input file not found: {}", input_path.display()));
    }

    // Opt-in JIT path via `RTS_JIT=1`. Skips disk I/O and the system
    // linker, running the program straight out of executable memory.
    // Default remains AOT (object + linker) until JIT validation is wider.
    if std::env::var("RTS_JIT").ok().as_deref() == Some("1") {
        let (exit_code, warnings) = pipeline::run_jit(&input_path, options)
            .with_context(|| format!("JIT run of {} failed", input_path.display()))?;
        if options.debug {
            for warning in &warnings {
                eprintln!("warning: {warning}");
            }
        }
        std::process::exit(exit_code);
    }

    let out_path = temp_binary_path(&input_path);
    let outcome = pipeline::build_executable(&input_path, &out_path, options)
        .with_context(|| format!("compile of {} failed", input_path.display()))?;

    if options.debug {
        for warning in &outcome.compile.warnings {
            eprintln!("warning: {warning}");
        }
    }

    let status = Command::new(&outcome.binary.path)
        .status()
        .with_context(|| format!("failed to execute {}", outcome.binary.path.display()))?;

    std::process::exit(status.code().unwrap_or(0));
}

fn temp_binary_path(input: &std::path::Path) -> PathBuf {
    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("rts_program");
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let name = format!("rts_run_{stem}_{ts}");
    let mut p = std::env::temp_dir().join(name);
    #[cfg(target_os = "windows")]
    {
        p.set_extension("exe");
    }
    p
}
