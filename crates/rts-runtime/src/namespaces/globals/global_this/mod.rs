//! `globalThis` — JS global object aliases. Migrado ao modelo
//! `#[rts_namespace]` (stage 2c): o único membro (`undefined`) é `external`,
//! aliasando o símbolo `__RTS_FN_NS_GC_STRING_NEW`. O runtime (`rt.rs`) fica
//! intacto.
//!
//! Em browser `globalThis === window`; em Node.js `globalThis === global`.
//! RTS não tem objeto global no heap, mas expõe as propriedades de identidade
//! mais acessadas em código portável.

pub mod rt;

#[allow(unused_imports)]
use rts_engine::abi::ty::I64;
use rts_macro::rts_namespace;

/// Global object aliases — process, global, self, undefined.
#[rts_namespace(globalThis, sym = "NS_GC")]
impl GlobalThisNs {
    /// The undefined value (0 in RTS).
    #[rts_const(
        external,
        symbol = "__RTS_FN_NS_GC_STRING_NEW",
        ts = "undefined: undefined",
        pure
    )]
    pub fn undefined() -> I64 {
        unreachable!()
    }
}
