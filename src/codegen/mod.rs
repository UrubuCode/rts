pub mod cranelift;
pub mod object;

use std::path::Path;

use anyhow::Result;

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

pub fn generate_object_with_metadata_options(
    mir: &MirModule,
    metadata: &MetadataTable,
    output: &Path,
    emit_entrypoint: bool,
    optimize_for_production: bool,
) -> Result<ObjectArtifact> {
    let bytes = match cranelift::object_builder::lower_to_native_object_with_options(
        mir,
        &cranelift::object_builder::ObjectBuildOptions {
            emit_entrypoint,
            optimize_for_production,
        },
    ) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!(
                "RTS AOT: failed to emit native object ({}). Falling back to CLIF payload object.",
                error
            );

            let clif = cranelift::clif_builder::lower_to_clif(mir);
            let mut fallback = clif.render().into_bytes();
            fallback.extend_from_slice(b"\n--rts-type-metadata--\n");
            fallback.extend(cranelift::metadata::emit_type_metadata(metadata));
            fallback
        }
    };

    object::write_object_file(output, &bytes)
}
