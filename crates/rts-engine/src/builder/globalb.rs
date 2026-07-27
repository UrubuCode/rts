//! Global-scope registration (bare ident, no import): [`GlobalBuilder`], the
//! fluent form. Each call commits IMMEDIATELY (there is no batching to guard —
//! `Registry::insert_global` is per-member, not per-declaration).

use super::support::simple_member;
use crate::Sig;
use crate::abi::AbiType;
use crate::member::Member;
use crate::{Engine, MemberKind};

/// Registra membros de escopo global (bare ident, sem import): `NaN`, `isNaN`,
/// variáveis globais. Cada chamada insere direto no registry.
pub struct GlobalBuilder<'e> {
    engine: &'e mut Engine,
}

impl<'e> GlobalBuilder<'e> {
    pub(crate) fn new(engine: &'e mut Engine) -> Self {
        Self { engine }
    }

    /// Função global: `isNaN(x)`.
    pub fn function(self, name: &str, ptr: *const u8, sig: Sig) -> Self {
        let m = simple_member("GLOBAL", name, MemberKind::Function, sig, ptr);
        self.engine.registry_mut().insert_global(m);
        self
    }

    /// Constante global: `NaN`, `Infinity`. `ptr` = getter `fn() -> ty`.
    pub fn constant(self, name: &str, ptr: *const u8, ty: AbiType) -> Self {
        let m = simple_member(
            "GLOBAL",
            name,
            MemberKind::Constant,
            Sig::new(Vec::new(), ty),
            ptr,
        );
        self.engine.registry_mut().insert_global(m);
        self
    }

    /// Escape hatch: insert a fully-built [`Member`] as a bare global.
    pub fn member(self, member: Member) -> Self {
        self.engine.registry_mut().insert_global(member);
        self
    }
}
