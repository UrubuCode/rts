//! `globalThis` — JS global object aliases. Migrado do `#[rts_namespace]` pro
//! modelo builder hand-written do `rts-engine` (rumo à remoção da `rts-macro`):
//! o único membro (`undefined`) é `external`, aliasando o símbolo
//! `__RTS_FN_NS_GC_STRING_NEW` (`fn_ptr` null). O runtime (`rt.rs`) fica intacto.
//!
//! Em browser `globalThis === window`; em Node.js `globalThis === global`.
//! RTS não tem objeto global no heap, mas expõe as propriedades de identidade
//! mais acessadas em código portável.

pub mod rt;

use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

/// Registra a namespace `globalThis` no motor (hand-written, sem macro). O único
/// membro (`undefined`) é `external` — o extern pertence ao namespace `gc`.
pub fn register(e: &mut Engine) {
    e.ns("globalThis")
        .doc("Global object aliases — process, global, self, undefined.")
        .member(Member {
            name: "undefined".to_string(),
            kind: MemberKind::Constant,
            sig: Sig::new(Vec::new(), AbiType::I64),
            symbol: "__RTS_FN_NS_GC_STRING_NEW".to_string(),
            fn_ptr: FnPtr(core::ptr::null::<u8>()),
            flags: MemberFlags::NONE,
            aliases: Vec::new(),
            variadic: false,
            ts_signature: "undefined: undefined".to_string(),
            doc: "The undefined value (0 in RTS).".to_string(),
            pure: true,
            intrinsic: None,
        })
        .done();
}
