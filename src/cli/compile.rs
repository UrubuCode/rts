//! `rts compile <input.ts> [output]` — full compile + link pipeline.
//!
//! Emits a native executable by combining the user program (compiled
//! via Cranelift) with the RTS static runtime support library.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};

use crate::compile_options::CompileOptions;
use crate::pipeline;

pub fn command(
    input: Option<String>,
    output: Option<String>,
    options: CompileOptions,
) -> Result<()> {
    let input = input.ok_or_else(|| anyhow!("missing input file for `rts compile`"))?;
    let input_path = PathBuf::from(&input);

    if !input_path.exists() {
        return Err(anyhow!("input file not found: {}", input_path.display()));
    }

    let output_path = match output {
        Some(value) => PathBuf::from(value),
        None => default_output_path(&input_path),
    };

    let outcome = pipeline::build_executable(&input_path, &output_path, options)
        .with_context(|| format!("compile of {} failed", input_path.display()))?;

    if options.debug {
        for warning in &outcome.compile.warnings {
            eprintln!("warning: {warning}");
        }
    }

    println!(
        "wrote {}  ({} byte(s), {} call(s) emitted, {} warning(s))",
        outcome.binary.path.display(),
        outcome.compile.object.bytes_written,
        outcome.compile.object.emitted_calls,
        outcome.compile.warnings.len(),
    );
    println!(
        "linker backend: {}, format: {}",
        outcome.binary.backend, outcome.binary.format
    );

    Ok(())
}

fn default_output_path(input: &Path) -> PathBuf {
    let mut out = input.to_path_buf();
    #[cfg(target_os = "windows")]
    {
        out.set_extension("exe");
    }
    #[cfg(not(target_os = "windows"))]
    {
        out.set_extension("");
    }
    out
}
