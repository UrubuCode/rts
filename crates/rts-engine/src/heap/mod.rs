//! Heap GC tipado — movido do `rts-runtime` pro motor (`rts-engine`), que é o
//! núcleo único de backend + frontend (visão do dev). Por ora só os tipos-payload
//! puros (value-types) do `Entry` vivem aqui; o `Entry`/`HandleTable` + a
//! orquestração migram em fatias gateadas (ver `.claude/plans/`).

pub mod fixed;
