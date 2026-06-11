//! Globais JS UNIVERSAIS movidos pro `rts-shared` (rodam em qualquer alvo, incl.
//! browser/wasm). Cada um expõe `register_*_class_spec(&mut Engine)`, chamado
//! pelo registro em `rts-codegen/src/abi/mod.rs` via o facade
//! `rts-runtime::namespaces::globals`. Submódulo próprio (`globals::`) p/ evitar
//! colisão com nomes de namespace flat (ex.: ns `date` vs global `date`).

pub mod symbol;
