pub mod checker;
pub mod metadata;
pub mod resolver;
pub mod runtime_info;
pub mod types;

use std::collections::BTreeMap;

use types::{Type, TypeId, TypeKind};

#[derive(Debug, Default)]
pub struct TypeRegistry {
    by_id: BTreeMap<TypeId, Type>,
    by_name: BTreeMap<String, TypeId>,
    next_id: u64,
}

impl TypeRegistry {
    pub fn register(&mut self, name: impl Into<String>, kind: TypeKind) -> TypeId {
        let name = name.into();

        if let Some(existing_id) = self.by_name.get(&name) {
            return *existing_id;
        }

        self.next_id += 1;
        let id = TypeId(self.next_id);

        let ty = Type {
            id,
            name: name.clone(),
            kind,
        };

        self.by_name.insert(name, id);
        self.by_id.insert(id, ty);

        id
    }

    pub fn get_by_name(&self, name: &str) -> Option<&Type> {
        self.by_name.get(name).and_then(|id| self.by_id.get(id))
    }

    pub fn get_by_id(&self, id: TypeId) -> Option<&Type> {
        self.by_id.get(&id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Type> {
        self.by_id.values()
    }

    pub fn len(&self) -> usize {
        self.by_id.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }
}
