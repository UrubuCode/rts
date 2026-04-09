pub mod cranelift;
pub mod object;

use std::path::Path;

use anyhow::{Context, Result};

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
    optimize_for_production: bool,
) -> Result<ObjectArtifact> {
    let bytes = cranelift::object_builder::lower_typed_to_native_object(
        mir,
        &cranelift::object_builder::ObjectBuildOptions {
            emit_entrypoint,
            optimize_for_production,
        },
    )
    .context("failed to lower typed MIR to native object with Cranelift")?;

    object::write_object_file(output, &bytes)
}

pub fn generate_object_with_metadata_options(
    mir: &MirModule,
    metadata: &MetadataTable,
    output: &Path,
    emit_entrypoint: bool,
    optimize_for_production: bool,
) -> Result<ObjectArtifact> {
    let _ = metadata;
    let bytes = cranelift::object_builder::lower_to_native_object_with_options(
        mir,
        &cranelift::object_builder::ObjectBuildOptions {
            emit_entrypoint,
            optimize_for_production,
        },
    )
    .context("failed to lower MIR to native object with Cranelift")?;

    object::write_object_file(output, &bytes)
}
