//! `rts-std` — camada backend do RTS app. Namespaces de plataforma que dependem
//! de I/O, OS, rede, áudio, async. Primeira fatia da partição (Fase 1b): `audio`
//! + `asio_audio` saem do `rts-runtime` pra cá. O resto migra em batches gateados.

pub mod audio;
#[cfg(feature = "asio")]
pub mod asio_audio;
pub mod engine;
pub mod io;
pub mod os;
pub mod env;
pub mod runtime;
pub mod test;
pub mod net;
pub mod process;
pub mod sync;
pub mod atomic;
pub mod ffi;
pub mod fs;
pub mod tls;
pub mod thread;
pub mod http_server;
pub mod events;
pub mod promise_slot;
pub mod collector;
pub mod time;
pub mod crypto;
pub mod promise;
pub mod event_loop;
pub mod gc_surface;
pub mod globals;
