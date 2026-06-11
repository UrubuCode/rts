//! Namespace implementations exposed through the new ABI.
//!
//! Each submodule registers an `abi::SPEC` consumed by codegen via
//! [`crate::abi::SPECS`]. No legacy dispatch path remains: every callee is
//! resolved to a canonical `__RTS_*` extern "C" symbol and called directly.

pub use rts_shared::alloc;
/// `audio`/`asio_audio` migraram pro crate backend `rts-std` (Fase 1b, partição
/// de crates). Facade → `crate::namespaces::audio` (register_builtins, jit.rs)
/// segue resolvendo.
pub use rts_std::audio;
#[cfg(feature = "asio")]
pub use rts_std::asio_audio;
pub mod globals;
pub use rts_std::atomic;
pub use rts_shared::trace;
pub use rts_shared::bigfloat;
pub use rts_shared::buffer;
pub use rts_shared::collections;
pub mod crypto;
pub use rts_shared::date;
pub use rts_std::env;
pub mod events;
pub use rts_shared::fmt;
pub use rts_std::ffi;
pub use rts_std::fs;
pub mod collector;
/// Alias retrocompatível: a pasta `gc/` foi renomeada `collector/` (Fase 2 GC,
/// rumo ao sistema de coleta no `rts-engine`). Os ~68 consumidores +
/// `crate::namespaces::gc::*` no codegen continuam resolvendo via este alias até
/// a migração do mecanismo pro engine concluir.
pub use collector as gc;
pub use rts_shared::hash;
pub use rts_shared::hint;
pub use rts_std::http_server;
pub use rts_std::io;
pub use rts_shared::json;
pub use rts_shared::math;
pub use rts_shared::mem;
pub use rts_std::net;
pub use rts_shared::num;
pub use rts_std::os;
pub use rts_shared::path;
pub use rts_std::process;
pub mod promise;
pub use rts_shared::ptr;
pub use rts_shared::regex;
pub mod parallel;
pub use rts_std::runtime;
// string movido para globals/string
pub use rts_std::sync;
pub use rts_std::test;
pub use rts_std::thread;
pub use rts_std::tls;
pub mod time;
