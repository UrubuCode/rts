use std::collections::BTreeMap;

use super::TypeRegistry;
use super::types::TypeId;

#[derive(Debug, Clone, Default)]
pub struct TypeResolver {
    by_name: BTreeMap<String, TypeId>,
}

impl TypeResolver {
    pub fn from_registry(registry: &TypeRegistry) -> Self {
        let mut by_name = BTreeMap::new();

        for ty in registry.iter() {
            by_name.insert(ty.name.clone(), ty.id);
        }

        Self { by_name }
    }

    pub fn resolve(&self, name: &str) -> Option<TypeId> {
        self.by_name.get(name).copied()
    }
}
