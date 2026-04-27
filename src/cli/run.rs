//! `rts run <input.ts>` — compile + execute via Cranelift JIT.

use std::path::PathBuf;

use anyhow::{Context, Result, anyhow};

use crate::compile_options::CompileOptions;
use crate::pipeline;

pub fn command(input: Option<String>, options: CompileOptions) -> Result<()> {
    let input = input.ok_or_else(|| anyhow!("usage: rts run <input.ts>"))?;
    let input_path = PathBuf::from(&input);
    if !input_path.exists() {
        return Err(anyhow!("input file not found: {}", input_path.display()));
    }

    let (exit_code, warnings) = pipeline::run_jit_with_imports(&input_path, options)
        .with_context(|| format!("JIT run of {} failed", input_path.display()))?;
    for warning in &warnings {
        eprintln!("warning: {warning}");
    }
    std::process::exit(exit_code);
}
