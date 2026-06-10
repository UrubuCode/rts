//! `rts-abi` — **shim de compatibilidade**. O conteúdo real foi dobrado em
//! `rts_engine::abi` (o núcleo cru). Este crate apenas re-exporta, mantendo os
//! consumidores (`rts_abi::ty`, `rts_abi::NamespaceMember`, `rts_abi::signature`,
//! …) compilando enquanto migram para `rts_engine` crate-por-crate. Será
//! deletado quando o último consumidor flipar.
//!
//! O glob abaixo re-exporta tanto os itens (`AbiType`, `MemberKind`, …) quanto
//! os submódulos públicos (`ty`, `str_abi`, `signature`, `symbols`, `handles`,
//! `guards`, `global_class`, `js_error`, `member`, `types`).

pub use rts_engine::abi::*;
