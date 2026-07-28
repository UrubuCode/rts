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
//!
//! Migrado pro `#[rtse::class]` (macro) — piloto da escape hatch de ctor
//! (`#[rtse::ctor] fn new(...) -> Handle`): `Proxy` aloca seu PRÓPRIO
//! `Entry::Proxy` dedicado (não um `Entry::Rtse` boxed struct), então o ctor
//! opta pelo passthrough de handle em vez de `alloc_rtse`. `Proxy` é um marker
//! `struct` unit — nenhum campo, o estado vive inteiro dentro do `Entry::Proxy`
//! alocado por `ops::new_proxy`.

pub mod ops;

use rts_engine::abi::ty::Handle;

/// Marker type only — `#[rtse::class]` needs a Rust type to hang the `impl` on,
/// but Proxy carries no Rust-side fields: `target`/`handler` live inside the
/// dedicated `Entry::Proxy { target, handler }` the ctor allocates directly
/// (see `ops::new_proxy`), never boxed into an `Entry::Rtse`.
#[rtse::class("Proxy")]
pub struct Proxy;

#[rtse::class("Proxy")]
impl Proxy {
    /// `new Proxy(target, handler)` — allocates `Entry::Proxy`. Traps are NOT
    /// Registry members; they resolve at runtime inside the engine's generic
    /// property trampolines (`resolve_proxy` in `obj_get`/`obj_set`), so there
    /// is nothing else to register for this class.
    ///
    /// Returns a raw `Handle` (not `Self`) — the escape hatch added in
    /// `rts-macro/src/class/member/returns.rs`: this ctor owns its own
    /// `Entry::` variant, so it must NOT be boxed via `alloc_rtse`.
    #[rtse::ctor]
    fn new(target: Handle, handler: Handle) -> Handle {
        ops::new_proxy(target, handler)
    }
}
