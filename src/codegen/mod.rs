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
    let bytes = match cranelift::object_builder::lower_to_native_object(mir) {
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
