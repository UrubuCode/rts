//! `rts-std` — camada backend do RTS app. Namespaces de plataforma que dependem
//! de I/O, OS, rede, áudio, async. Primeira fatia da partição (Fase 1b): `audio`
//! + `asio_audio` saem do `rts-runtime` pra cá. O resto migra em batches gateados.

pub mod audio;
#[cfg(feature = "asio")]
pub mod asio_audio;
