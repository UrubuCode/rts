//! Namespace implementations exposed through the new ABI.
//!
//! Each submodule registers an `abi::SPEC` consumed by codegen via
//! [`crate::abi::SPECS`]. No legacy dispatch path remains: every callee is
//! resolved to a canonical `__RTS_*` extern "C" symbol and called directly.

pub mod alloc;
pub mod audio;
#[cfg(feature = "asio")]
pub mod asio_audio;
pub mod globals;
pub mod atomic;
pub mod trace;
pub mod bigfloat;
pub mod buffer;
pub mod collections;
pub mod crypto;
pub mod date;
pub mod env;
pub mod events;
pub mod fmt;
pub mod ffi;
pub mod fs;
pub mod collector;
/// Alias retrocompatível: a pasta `gc/` foi renomeada `collector/` (Fase 2 GC,
/// rumo ao sistema de coleta no `rts-engine`). Os ~68 consumidores +
/// `crate::namespaces::gc::*` no codegen continuam resolvendo via este alias até
/// a migração do mecanismo pro engine concluir.
pub use collector as gc;
pub mod hash;
pub mod hint;
pub mod http_server;
pub mod io;
pub mod json;
pub mod math;
pub mod mem;
pub mod net;
pub mod num;
pub mod os;
pub mod path;
pub mod process;
pub mod promise;
pub mod ptr;
pub mod regex;
pub mod parallel;
pub mod runtime;
// string movido para globals/string
pub mod sync;
pub mod test;
pub mod thread;
pub mod tls;
pub mod time;
