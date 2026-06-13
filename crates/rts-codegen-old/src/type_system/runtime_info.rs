use super::metadata::TypeMetadata;

#[derive(Debug, Clone, Default)]
pub struct RuntimeTypeInfo {
    pub registry_symbol: String,
    pub exported_types: Vec<TypeMetadata>,
}

impl RuntimeTypeInfo {
    pub fn new(exported_types: Vec<TypeMetadata>) -> Self {
        Self {
            registry_symbol: "__rts_type_registry".to_string(),
            exported_types,
        }
    }
}
