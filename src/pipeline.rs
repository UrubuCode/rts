//! End-to-end compile pipeline for the bootstrap MVP.
//!
//! Responsibilities:
//! 1. Read + parse the source file.
//! 2. Emit a user object via `codegen`.
//! 3. Extract the embedded RTS static library to a cache directory.
//! 4. Optionally link the user object + runtime + CRT into a final binary.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::codegen::ObjectArtifact;
use crate::compile_options::CompileOptions;
use crate::linker::{self, LinkedBinary};
use crate::parser;
use crate::runtime::embedded::extract_runtime_staticlib;

#[derive(Debug, Clone)]
pub struct CompileOutcome {
    pub input: PathBuf,
    pub object: ObjectArtifact,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct LinkOutcome {
    pub compile: CompileOutcome,
    pub binary: LinkedBinary,
    pub runtime_staticlib: PathBuf,
}

/// Parses `input` and emits an object file next to it.
pub fn compile_file(
    input: &Path,
    output_object: &Path,
    options: CompileOptions,
) -> Result<CompileOutcome> {
    let source = std::fs::read_to_string(input)
        .with_context(|| format!("failed to read {}", input.display()))?;
    compile_source(&source, input, output_object, options)
}

/// Parses an in-memory source and emits an object.
pub fn compile_source(
    source: &str,
    input: &Path,
    output_object: &Path,
    options: CompileOptions,
) -> Result<CompileOutcome> {
    let program = parser::parse_source_with_mode(source, options.frontend_mode)
        .with_context(|| format!("failed to parse {}", input.display()))?;

    let (object, warnings) =
        crate::codegen::compile_program_to_object(&program, output_object)?;

    Ok(CompileOutcome {
        input: input.to_path_buf(),
        object,
        warnings,
    })
}

/// Full compile + link path: produces an executable at `output_binary`.
pub fn build_executable(
    input: &Path,
    output_binary: &Path,
    options: CompileOptions,
) -> Result<LinkOutcome> {
    let user_object = output_binary
        .with_extension("o");
    let compile = compile_file(input, &user_object, options)?;

    let runtime_lib = extract_runtime_staticlib()
        .context("failed to prepare embedded RTS runtime static library")?;

    let inputs = vec![compile.object.path.clone(), runtime_lib.clone()];
    let binary = linker::link_objects_to_binary(&inputs, output_binary)
        .context("linker failed while combining user object + RTS runtime")?;

    Ok(LinkOutcome {
        compile,
        binary,
        runtime_staticlib: runtime_lib,
    })
}
