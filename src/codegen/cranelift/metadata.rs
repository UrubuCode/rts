use crate::type_system::metadata::MetadataTable;

pub fn emit_type_metadata(table: &MetadataTable) -> Vec<u8> {
    table.to_text().into_bytes()
}
