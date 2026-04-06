#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TypeAnnotation {
    pub name: String,
    pub resolved_id: Option<u64>,
}

impl TypeAnnotation {
    pub fn unresolved(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            resolved_id: None,
        }
    }

    pub fn resolved(name: impl Into<String>, id: u64) -> Self {
        Self {
            name: name.into(),
            resolved_id: Some(id),
        }
    }
}
