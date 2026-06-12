//! Globais JS UNIVERSAIS movidos pro `rts-shared` (rodam em qualquer alvo, incl.
//! browser/wasm). Cada um expõe `register_*_class_spec(&mut Engine)`, chamado
//! pelo registro em `rts-codegen/src/abi/mod.rs` via o facade
//! `rts-runtime::namespaces::globals`. Submódulo próprio (`globals::`) p/ evitar
//! colisão com nomes de namespace flat (ex.: ns `date` vs global `date`).

pub mod symbol;
// boolean → PRIMORDIAL: movido p/ `rts-primitives` (Fase 2). Re-exportado pela
// fachada `rts-runtime::namespaces::globals::boolean`.
pub mod bigint;
// number → PRIMORDIAL: movido p/ `rts-primitives` (Fase 2).
pub mod url;
pub mod weakmap;
pub mod weakset;
pub mod weakref;
pub mod finalization_registry;
pub mod regexp;
pub mod json;
pub mod json5;
pub mod intl;
pub mod dom_exception;
pub mod global_this;
pub mod date;
// error → PRIMORDIAL: movido p/ `rts-primitives` (Fase 2).
// function → PRIMORDIAL: movido p/ `rts-primitives` (Fase 2.3). Desacoplado de
// collections/proxy via shims extern-C; chama-os por símbolo (link-time).
pub mod proxy;
pub mod reflect;
// string → PRIMORDIAL: movido p/ `rts-primitives` (Fase 2). collections/vec.rs
// chama rts_primitives::string::rt::* por path.
