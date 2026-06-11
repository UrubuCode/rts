//! Globais JS platform-divergent que vivem na camada BACKEND (`rts-std`): tocam
//! I/O (console→io), tempo/timers, rede (fetch), perf, ou modelam recursos de
//! plataforma (blob/headers/form_data/streams/eventos). NÃO universais (não
//! rodam em wasm sem backend). Cada um expõe `register*`/`register_*_class_spec`,
//! chamado pelo registro em `rts-codegen/src/abi/mod.rs` via o facade
//! `rts-runtime::namespaces::globals`.
//!
//! Externs do collector do runtime (`__RTS_FN_RT_ERROR_SET`) e da função em
//! rts-shared (`__RTS_FN_RT_INVOKE_AUTO`) são resolvidos por link (`extern "C"`).

pub mod console;
pub mod performance;
pub mod headers;
pub mod form_data;
pub mod event_target;
pub mod message_channel;
pub mod abort;
pub mod events;
pub mod text_encoding;
