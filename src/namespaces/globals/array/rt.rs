//! Array — runtime implementations (re-exports from collections).
//!
//! As funções de runtime para Array já estão implementadas em
//! `namespaces::collections::vec`. Este módulo apenas re-exporta
//! os símbolos necessários e fornece wrappers quando necessário.

// Re-exporta as funções do namespace collections que implementam
// os métodos do Array global.
pub use crate::namespaces::collections::vec::{
    // Métodos de instância
    __RTS_FN_NS_COLLECTIONS_VEC_PUSH,
    __RTS_FN_NS_COLLECTIONS_VEC_POP,
    __RTS_FN_NS_COLLECTIONS_VEC_AT,
    __RTS_FN_NS_COLLECTIONS_VEC_JOIN,
    __RTS_FN_NS_COLLECTIONS_VEC_CLEAR,
    __RTS_FN_NS_COLLECTIONS_VEC_INDEX_OF,
    __RTS_FN_NS_COLLECTIONS_VEC_LAST_INDEX_OF,
    __RTS_FN_NS_COLLECTIONS_VEC_INCLUDES,
    __RTS_FN_NS_COLLECTIONS_VEC_REVERSE,
    __RTS_FN_NS_COLLECTIONS_VEC_FLAT,
    __RTS_FN_NS_COLLECTIONS_VEC_SHIFT,
    __RTS_FN_NS_COLLECTIONS_VEC_UNSHIFT,
    __RTS_FN_NS_COLLECTIONS_VEC_SLICE,
    __RTS_FN_NS_COLLECTIONS_VEC_CONCAT,
    __RTS_FN_NS_COLLECTIONS_VEC_FILL,
    __RTS_FN_NS_COLLECTIONS_VEC_SPLICE_REMOVE,
    __RTS_FN_NS_COLLECTIONS_VEC_LEN,
    // Array.from
    __RTS_FN_NS_COLLECTIONS_VEC_FROM,
};

// Implementação específica para Array.isArray
// Usa a função de verificação de tipo do GC
pub use crate::namespaces::gc::__RTS_FN_NS_GC_IS_VEC as __RTS_FN_GL_ARRAY_IS_ARRAY;
