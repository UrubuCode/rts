//! `rts-shared` — camada UNIVERSAL do RTS. Namespaces pure-compute que rodam em
//! qualquer alvo (incl. browser/wasm): nenhum toca I/O, OS, rede ou async.
//! Depende só do motor (`rts-engine`) + libs puras. Fase 1b da partição —
//! primeira fatia: pure-compute batch. Ver
//! `.claude/plans/partitioned-meandering-milner.md`.
pub mod alloc;
pub mod bigfloat;
pub mod buffer;
pub mod collections;
pub mod date;
pub mod fmt;
pub mod gc_surface;
pub mod globals;
pub mod hash;
pub mod hint;
pub mod json;
pub mod math;
pub mod mem;
pub mod num;
pub mod path;
pub mod protobuf;
pub mod ptr;
pub mod regex;
pub mod stdlib;
pub mod trace;
