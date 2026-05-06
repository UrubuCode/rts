//! `rts ir <input.ts>` — dump Cranelift IR to stderr without executing.

use std::path::PathBuf;

use anyhow::{Context, Result, anyhow};

use crate::compile_options::CompileOptions;
use crate::pipeline;

pub fn command(input: Option<String>, options: CompileOptions) -> Result<()> {
    let input = input.ok_or_else(|| anyhow!("usage: rts ir <input.ts>"))?;
    let input_path = PathBuf::from(&input);
    if !input_path.exists() {
        return Err(anyhow!("input file not found: {}", input_path.display()));
    }

    if let Ok(abs) = input_path.canonicalize() {
        if let Some(dir) = abs.parent() {
            crate::dotenv::load_from_dir(dir);
        }
    }

    let warnings = pipeline::dump_ir_with_imports(&input_path, options)
        .with_context(|| format!("IR dump of {} failed", input_path.display()))?;

    for w in &warnings {
        eprintln!("warning: {w}");
    }

    Ok(())
}
