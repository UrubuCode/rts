use super::TypeRegistry;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldMetadata {
    pub name: String,
    pub type_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeMetadata {
    pub name: String,
    pub kind: String,
    pub fields: Vec<FieldMetadata>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MetadataTable {
    pub types: Vec<TypeMetadata>,
}

impl MetadataTable {
    pub fn from_registry(registry: &TypeRegistry) -> Self {
        let mut types = Vec::new();

        for ty in registry.iter() {
            let (kind, fields) = match &ty.kind {
                super::types::TypeKind::Primitive => ("primitive".to_string(), Vec::new()),
                super::types::TypeKind::Interface { fields } => (
                    "interface".to_string(),
                    fields
                        .iter()
                        .map(|field| FieldMetadata {
                            name: field.name.clone(),
                            type_name: field.type_name.clone(),
                        })
                        .collect(),
                ),
                super::types::TypeKind::Class { fields } => (
                    "class".to_string(),
                    fields
                        .iter()
                        .map(|field| FieldMetadata {
                            name: field.name.clone(),
                            type_name: field.type_name.clone(),
                        })
                        .collect(),
                ),
                super::types::TypeKind::Alias { .. } => ("alias".to_string(), Vec::new()),
                super::types::TypeKind::Enum { .. } => ("enum".to_string(), Vec::new()),
                super::types::TypeKind::GenericParameter { .. } => {
                    ("generic_parameter".to_string(), Vec::new())
                }
            };

            types.push(TypeMetadata {
                name: ty.name.clone(),
                kind,
                fields,
            });
        }

        Self { types }
    }

    pub fn to_text(&self) -> String {
        let mut lines = Vec::new();

        for ty in &self.types {
            let mut line = format!("type {} kind={}", ty.name, ty.kind);

            if !ty.fields.is_empty() {
                let fields = ty
                    .fields
                    .iter()
                    .map(|field| format!("{}:{}", field.name, field.type_name))
                    .collect::<Vec<_>>()
                    .join(",");
                line.push_str(&format!(" fields=[{}]", fields));
            }

            lines.push(line);
        }

        lines.join("\n")
    }
}
