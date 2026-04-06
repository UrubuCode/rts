use std::path::PathBuf;

use anyhow::{Context, Result};

pub fn command(input_arg: Option<String>, output_arg: Option<String>) -> Result<()> {
    let input = input_arg
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("examples/hello_world.ts"));

    let output = output_arg
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/rts_app"));

    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    let summary = crate::compile_file(&input, &output)
        .with_context(|| format!("failed to compile {}", input.display()))?;

    println!(
        "Build complete: {} (modules={}, types={}, functions={}, linker={}, format={})",
        summary.binary_file.display(),
        summary.compiled_modules,
        summary.discovered_types,
        summary.lowered_functions,
        summary.link_backend,
        summary.link_format
    );

    Ok(())
}
