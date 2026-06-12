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
pub mod error;
pub mod function;
pub mod proxy;
pub mod reflect;
pub mod string;
