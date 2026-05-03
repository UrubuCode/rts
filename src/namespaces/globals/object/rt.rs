//! Object — runtime implementations (re-exports from collections).
//!
//! As funções de runtime para Object já estão implementadas em
//! `namespaces::collections::map`. Este módulo apenas re-exporta
//! os símbolos necessários.

// Re-exporta as funções do namespace collections que implementam
// os métodos do Object global.
pub use crate::namespaces::collections::map::{
    __RTS_FN_GL_OBJECT_CREATE,
    __RTS_FN_GL_OBJECT_HAS_OWN_PROPERTY,
    __RTS_FN_NS_COLLECTIONS_MAP_KEYS,
    __RTS_FN_NS_COLLECTIONS_MAP_VALUES,
    __RTS_FN_NS_COLLECTIONS_MAP_ENTRIES,
    __RTS_FN_NS_COLLECTIONS_MAP_ASSIGN,
    __RTS_FN_NS_COLLECTIONS_MAP_FREEZE,
    __RTS_FN_NS_COLLECTIONS_MAP_FROM_ENTRIES,
};
