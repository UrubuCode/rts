//! Globais JS UNIVERSAIS movidos pro `rts-shared` (rodam em qualquer alvo, incl.
//! browser/wasm). Cada um expõe `register_*_class_spec(&mut Engine)`, chamado
//! pelo registro em `rts-codegen/src/abi/mod.rs` via o facade
//! `rts-runtime::namespaces::globals`. Submódulo próprio (`globals::`) p/ evitar
//! colisão com nomes de namespace flat (ex.: ns `date` vs global `date`).

// symbol → PRIMORDIAL: movido p/ `rts-primitives` (layering fix). Re-exportado
// pela fachada `rts-runtime::namespaces::globals::symbol`.
// boolean → PRIMORDIAL: movido p/ `rts-primitives` (Fase 2). Re-exportado pela
// fachada `rts-runtime::namespaces::globals::boolean`.
// bigint: register_bigint_class_spec removido (DRAIN_MOTOR) — nunca chamado;
// BigInt real é PRIMORDIAL via rts_adapters::value::taops (registry_build.rs).
// number → PRIMORDIAL: movido p/ `rts-primitives` (Fase 2).
pub mod date;
pub mod point;
pub mod point3;
// DOMException — DRAIN_MOTOR §11 (owner correction): reimplemented as
// `#[rtse::class]`, replacing the removed ambient `.ts` DOMEXCEPTION_TS.
pub mod dom_exception;
// finalization_registry → PRIMORDIAL (GC-coupled): movido p/ `rts-primitives`
// (layering fix). Re-exportado pela fachada
// `rts-runtime::namespaces::globals::finalization_registry`.
pub mod global_this;
// intl: 7 register_*_class_spec (NumberFormat/DateTimeFormat/Collator/Segmenter/
// PluralRules/ListFormat/RelativeTimeFormat) removidos (DRAIN_MOTOR) — nunca
// chamados (não wired em registry_build.rs), sem `.ts` substituto encontrado.
pub mod json;
pub mod json5;
// regexp → PRIMORDIAL (native `/re/` syntax): movido p/ `rts-primitives`
// (layering fix). Re-exportado pela fachada
// `rts-runtime::namespaces::globals::regexp`.
pub mod url;
// weakmap: register_weakmap_class_spec removido (DRAIN_MOTOR) — nunca
// chamado; WeakMap real é o `.ts` stdlib WEAKMAP_SET_TS.
// weakref → PRIMORDIAL (GC-coupled): movido p/ `rts-primitives` (layering
// fix). Re-exportado pela fachada `rts-runtime::namespaces::globals::weakref`.
// weakset: register_weakset_class_spec removido (DRAIN_MOTOR) — nunca
// chamado; WeakSet real é o `.ts` stdlib WEAKMAP_SET_TS.
// error → PRIMORDIAL: movido p/ `rts-primitives` (Fase 2).
// function → PRIMORDIAL: movido p/ `rts-primitives` (Fase 2.3). Desacoplado de
// collections/proxy via shims extern-C; chama-os por símbolo (link-time).
// proxy/reflect → PRIMORDIAL: movidos p/ `rts-primitives` (layering fix).
// Re-exportados pela fachada `rts-runtime::namespaces::globals::{proxy,reflect}`.
// string → PRIMORDIAL: movido p/ `rts-primitives` (Fase 2). collections/vec.rs
// chama rts_primitives::string::rt::* por path.
