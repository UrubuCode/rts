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
    let clif = cranelift::clif_builder::lower_to_clif(mir);

    let mut bytes = clif.render().into_bytes();
    bytes.extend_from_slice(b"\n--rts-type-metadata--\n");
    bytes.extend(cranelift::metadata::emit_type_metadata(metadata));

    object::write_object_file(output, &bytes)
}
