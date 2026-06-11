//! `rts-shared` — camada UNIVERSAL do RTS. Namespaces pure-compute que rodam em
//! qualquer alvo (incl. browser/wasm): nenhum toca I/O, OS, rede ou async.
//! Depende só do motor (`rts-engine`) + libs puras. Fase 1b da partição —
//! primeira fatia: pure-compute batch. Ver
//! `.claude/plans/partitioned-meandering-milner.md`.

pub mod math;
pub mod num;
pub mod fmt;
pub mod hash;
pub mod mem;
pub mod ptr;
pub mod hint;
pub mod alloc;
pub mod path;
pub mod bigfloat;
pub mod buffer;
pub mod regex;
pub mod date;
pub mod globals;
