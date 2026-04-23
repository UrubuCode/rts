//! Cranelift-backed object emitter.
//!
//! Compiles a full [`Program`] — user functions, control flow, variables,
//! arithmetic, and namespace calls — into a native `.o` file with a `main`
//! entry point.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{anyhow, Context, Result};
use cranelift_codegen::settings::{self, Configurable};
use cranelift_object::{ObjectBuilder, ObjectModule};

use crate::codegen::lower::compile_program;
use crate::codegen::object::ObjectArtifact;
use crate::parser::ast::Program;

/// Compiles a program into a native object file.
pub fn compile_program_to_object(
    program: &Program,
    output_path: &Path,
) -> Result<(ObjectArtifact, Vec<String>)> {
    let mut module = build_module()?;
    let mut extern_cache = HashMap::new();
    let mut data_counter: u32 = 0;

    let warnings = compile_program(program, &mut module, &mut extern_cache, &mut data_counter)?;

    let product = module.finish();
    let bytes = product
        .emit()
        .map_err(|err| anyhow!("cranelift object emission failed: {err}"))?;

    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    std::fs::write(output_path, &bytes)
        .with_context(|| format!("failed to write object to {}", output_path.display()))?;

    Ok((
        ObjectArtifact {
            path: output_path.to_path_buf(),
            bytes_written: bytes.len(),
            emitted_calls: 0,
        },
        warnings,
    ))
}

fn build_module() -> Result<ObjectModule> {
    let mut flag_builder = settings::builder();
    flag_builder
        .set("is_pic", "true")
        .map_err(|e| anyhow!("cranelift flag error: {e}"))?;
    flag_builder
        .set("opt_level", "none")
        .map_err(|e| anyhow!("cranelift flag error: {e}"))?;
    let flags = settings::Flags::new(flag_builder);

    let isa_builder = cranelift_native::builder()
        .map_err(|e| anyhow!("failed to detect native target: {e}"))?;
    let isa = isa_builder
        .finish(flags)
        .map_err(|e| anyhow!("failed to finalise ISA: {e}"))?;

    let builder = ObjectBuilder::new(
        isa,
        b"rts_entry".to_vec(),
        cranelift_module::default_libcall_names(),
    )
    .map_err(|e| anyhow!("failed to build object module: {e}"))?;

    Ok(ObjectModule::new(builder))
}
