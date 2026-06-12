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

use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

/// Registra a classe global `Proxy` (não-primordial) no motor. Só o construtor
/// `new Proxy(target, handler)` → símbolo `__RTS_FN_GL_PROXY_NEW`. Com isso o
/// dispatch genérico de construtor (`global_class_lookup("Proxy")` em new_expr)
/// cobre a construção, sem braço hardcoded `class_name == "Proxy"` no motor.
/// As traps (get/set/has/delete/apply) seguem resolvidas em runtime via
/// `resolve_proxy` nos caminhos MAP_*/INVOKE — não há lógica de método de Proxy
/// no codegen. fn_ptr null: o símbolo resolve via o namespace que o registra.
pub fn register_proxy_class_spec(e: &mut Engine) {
    e.class("Proxy")
        .doc("Proxy(target, handler) — Entry::Proxy; traps resolvidas em runtime.")
        .member(Member {
            name: "new".to_string(),
            kind: MemberKind::Constructor,
            sig: Sig::new(vec![AbiType::Handle, AbiType::Handle], AbiType::Handle),
            symbol: "__RTS_FN_GL_PROXY_NEW".to_string(),
            fn_ptr: FnPtr(core::ptr::null::<u8>()),
            flags: MemberFlags::NONE,
            aliases: Vec::new(),
            variadic: false,
            ts_signature: "new (target: object, handler: ProxyHandler<object>): any".to_string(),
            doc: "new Proxy(target, handler) — cria Entry::Proxy.".to_string(),
            pure: false,
            intrinsic: None,
        })
        .done();
}
