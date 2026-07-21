//! ABI: o vocabulário estável entre codegen e runtime (tipos, símbolos,
//! assinaturas, handles, guards). Antes era o crate `rts-abi`; dobrado aqui
//! para o `rts-engine` ser o núcleo único. O crate `rts-abi` ainda existe como
//! shim re-exportando este módulo enquanto os consumidores migram.

pub mod global_class;
pub mod guards;
pub mod handles;
pub mod js_error;
pub mod member;
pub mod signature;
pub mod str_abi;
pub mod symbols;
pub mod ty;
pub mod types;

pub use global_class::GlobalClassSpec;
pub use js_error::JsErrorKind;
pub use member::{
    DefaultArg, MemberFlags, MemberKind, NamespaceMember, NamespaceSpec, concat_members,
};
pub use types::AbiType;
