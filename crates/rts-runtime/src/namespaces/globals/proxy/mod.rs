//! Proxy global (#218) — Phase 1: traps get/set/has/deleteProperty.
//!
//! `Proxy` wraps `(target, handler)` num Entry::Proxy. Quando user faz
//! `obj.x` ou `obj.x = v` num handle Proxy, MAP_GET_CHAIN/MAP_SET/etc
//! detectam via `resolve_proxy(handle)` e despacham pra trap correta
//! no handler — ou faz fallback direto no target se a trap nao existe.
//!
//! Out of scope v0:
//! - apply trap (Proxy sobre fn)
//! - construct trap
//! - getPrototypeOf/setPrototypeOf/isExtensible/preventExtensions traps
//! - ownKeys trap
//! - Invariants do spec (e.g. configurable check)
//!
//! Ver issue #218 (Proxy fase 1).

pub mod ops;
