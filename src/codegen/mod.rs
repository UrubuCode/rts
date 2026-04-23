pub mod clif_builder;
pub mod jit;
pub mod metadata;
pub(crate) mod mir_parse;
pub mod object;
pub mod object_builder;
pub mod ometa;
pub(crate) mod parse_utils;
pub mod type_layout;
pub mod typed;

use std::path::Path;

use anyhow::{Context, Result};

use crate::compile_options::CompileOptions;
use crate::mir::MirModule;
use crate::type_system::metadata::MetadataTable;

pub use object::ObjectArtifact;

pub fn generate_object_with_metadata(
    mir: &MirModule,
    metadata: &MetadataTable,
    output: &Path,
) -> Result<ObjectArtifact> {
    generate_object_with_metadata_options(mir, metadata, output, true, false)
}

pub fn generate_typed_object(
    mir: &crate::mir::TypedMirModule,
    output: &Path,
    emit_entrypoint: bool,
    options: &CompileOptions,
) -> Result<ObjectArtifact> {
    let bytes = object_builder::lower_typed_to_native_object(
        mir,
        &object_builder::ObjectBuildOptions {
            emit_entrypoint,
            optimize_for_production: options.profile.as_str() == "production",
        },
    )
    .context("failed to lower typed MIR to native object with Cranelift")?;

    let artifact = object::write_object_file(output, &bytes)?;

    if options.profile.is_development() {
        emit_ometa(mir, output);
    }

    Ok(artifact)
}

fn emit_ometa(mir: &crate::mir::TypedMirModule, obj_path: &Path) {
    use crate::codegen::ometa::OmetaWriter;

    let mut writer = OmetaWriter::new("development", "");
    for func in &mir.functions {
        if let Some(ref src) = func.source_file {
            writer.add_function(&func.name, 0, 0, src, func.source_line);
        }
    }
    if !writer.is_empty() {
        let _ = writer.write_to(obj_path);
    }
}

pub fn generate_object_with_metadata_options(
    mir: &MirModule,
    metadata: &MetadataTable,
    output: &Path,
    emit_entrypoint: bool,
    optimize_for_production: bool,
) -> Result<ObjectArtifact> {
    let _ = metadata;
    let bytes = object_builder::lower_to_native_object_with_options(
        mir,
        &object_builder::ObjectBuildOptions {
            emit_entrypoint,
            optimize_for_production,
        },
    )
    .context("failed to lower MIR to native object with Cranelift")?;

    object::write_object_file(output, &bytes)
}
