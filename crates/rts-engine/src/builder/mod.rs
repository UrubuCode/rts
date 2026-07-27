//! O builder: a API fluente + closure-scoped que as camadas (`rts-std`,
//! `rts-node`, `rts-browser`, módulos externos) usam para registrar a sua
//! superfície no [`Registry`](crate::Registry).
//!
//! Split into a folder (was one 453-line file) per the file-layout rule: each
//! submodule is one cohesive concern —
//!  - `support` — shared symbol-stem helpers.
//!  - `module` — [`ModuleAccum`](module::ModuleAccum) (the accumulator BOTH
//!    module front-ends commit through) + the fluent [`ModuleBuilder`].
//!  - `class` — the fluent [`ClassBuilder`].
//!  - `globalb` — the fluent [`GlobalBuilder`].
//!  - `closure` — the closure-scoped [`ModuleScope`]/[`GlobalScope`] (F1/F3 of
//!    `docs/specs/rts-macro-single-source.md`), the PRIMARY path for new call
//!    sites; see its module doc for the commit-once guarantee and the
//!    `RegistryItem`/`GlobalRegistryItem` traits `.registry(...)` runs on.

mod class;
mod closure;
mod globalb;
mod module;
mod support;

pub use class::ClassBuilder;
pub use closure::{GlobalRegistryItem, GlobalScope, ModuleScope, RegistryItem};
pub use globalb::GlobalBuilder;
pub use module::ModuleBuilder;

use module::ModuleAccum;

use crate::registry::Registry;

/// O motor: dono do [`Registry`] em construção. Uma camada recebe `&mut Engine`
/// e registra os seus módulos/classes/globais; o codegen consome
/// `engine.registry()` depois.
#[derive(Debug, Default)]
pub struct Engine {
    registry: Registry,
    /// Embedded TypeScript stdlib sources (see [`Engine::include`]).
    includes: Vec<String>,
}

impl Engine {
    /// Motor vazio.
    pub fn new() -> Self {
        Self::default()
    }

    /// Mutable access to the in-progress registry — used by the builder
    /// submodules, which cannot see `Engine::registry` (a private field of a
    /// sibling module) otherwise.
    pub(crate) fn registry_mut(&mut self) -> &mut Registry {
        &mut self.registry
    }

    /// Begin a CLOSURE-SCOPED module declaration — the primary path for new
    /// call sites (F1 of `docs/specs/rts-macro-single-source.md`):
    ///
    /// ```ignore
    /// e.module("node:fs", |m| {
    ///     m.alias("fs");
    ///     m.namespace("fs");
    ///     m.registry(READ_FILE);
    /// });
    /// ```
    ///
    /// `spec` is `"<scheme>:<name>"` (`"node:fs"`) or a bare name (`"io"` →
    /// scheme `rts`) — same parsing [`Engine::ns`] does, minus its comma-alias
    /// shorthand (use `m.alias(...)` inside the closure instead, so alias
    /// declaration has one spelling instead of two). The module commits
    /// EXACTLY ONCE, after the closure returns — see the [`ModuleScope`] doc
    /// for why that is a guarantee, not a convention.
    pub fn module(&mut self, spec: &str, f: impl FnOnce(&mut ModuleScope)) {
        let (scheme, name) = spec.split_once(':').unwrap_or(("rts", spec));
        let mut scope = ModuleScope {
            engine: self,
            accum: ModuleAccum::new(scheme, name),
        };
        f(&mut scope);
        scope.accum.commit(scope.engine);
    }

    /// Begin a CLOSURE-SCOPED global declaration — the primary path for new
    /// call sites (bare idents / classes with no import, no namespace):
    ///
    /// ```ignore
    /// e.global(|g| {
    ///     g.registry(STRING_CLASS);
    ///     g.registry(PARSE_INT);
    /// });
    /// ```
    ///
    /// Unlike [`Engine::module`] there is nothing to batch-commit: each item a
    /// global scope receives is inserted into the Registry immediately (see
    /// [`GlobalScope`]), so the closure is purely an ergonomic grouping, not a
    /// correctness boundary.
    pub fn global(&mut self, f: impl FnOnce(&mut GlobalScope)) {
        let mut scope = GlobalScope { engine: self };
        f(&mut scope);
    }

    /// Começa a registrar um módulo importável `"<scheme>:<name>"` via a API
    /// FLUENTE, `.done()`-terminated. Prefer [`Engine::module`] (the
    /// closure-scoped form) in new code — this stays for the existing `.ns(...)`
    /// call sites until their migration (see the spec's F7 phase).
    pub fn module_fluent<'e>(&'e mut self, scheme: &str, name: &str) -> ModuleBuilder<'e> {
        ModuleBuilder::new(self, scheme, name)
    }

    /// Começa a registrar um módulo importável a partir de UM spec-string:
    ///  - `"bigfloat"`            → esquema `rts` (`rts:bigfloat`);
    ///  - `"node:fs"`             → esquema explícito (`node:fs`);
    ///  - `"node:fs, fs"`         → canônico + ALIAS(es) bare (vírgula-separados).
    ///
    /// Implemented ON TOP of [`Engine::module_fluent`] (the fluent front-end),
    /// which itself commits through the same [`ModuleAccum`] the closure form
    /// uses — `ns()` is a sibling of the closure form, not a dependency of it.
    pub fn ns<'e>(&'e mut self, spec: &str) -> ModuleBuilder<'e> {
        let mut parts = spec.split(',').map(str::trim);
        let primary = parts.next().unwrap_or("");
        let (scheme, name) = primary.split_once(':').unwrap_or(("rts", primary));
        let mut b = self.module_fluent(scheme, name);
        for alias in parts.filter(|a| !a.is_empty()) {
            b = b.alias(alias);
        }
        b
    }

    /// Começa a registrar uma classe global (`new <Name>()`).
    pub fn class<'e>(&'e mut self, name: &str) -> ClassBuilder<'e> {
        ClassBuilder::new(self, name)
    }

    /// Começa a registrar membros de escopo global via a API FLUENTE. Prefer
    /// [`Engine::global`] (the closure-scoped form) in new code.
    pub fn global_fluent<'e>(&'e mut self) -> GlobalBuilder<'e> {
        GlobalBuilder::new(self)
    }

    /// Embed a TypeScript stdlib source compiled as a PRELUDE ahead of the user
    /// program by the new engine (its top-level classes/functions become ambient,
    /// shadowing natives). Declarations-only.
    pub fn include(&mut self, src: &str) {
        self.includes.push(src.to_string());
    }

    /// O registry montado — o que o codegen lê.
    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    /// Consome o motor, devolvendo o registry (para mover para um `OnceLock`).
    pub fn into_registry(mut self) -> Registry {
        self.registry.includes = self.includes;
        self.registry
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_include_and_ns_flags_roundtrip() {
        let mut e = Engine::new();
        e.include("class Foo {}");
        e.ns("secret").private().order(5).done();
        e.ns("pub_ns").done();
        let reg = e.into_registry();
        assert_eq!(reg.includes(), &["class Foo {}".to_string()]);
        let secret = reg.module("rts:secret").expect("secret ns");
        assert!(secret.private);
        assert_eq!(secret.load_order, 5);
        let pubm = reg.module("rts:pub_ns").expect("pub ns");
        assert!(!pubm.private);
        assert_eq!(pubm.load_order, 0);
    }

    #[test]
    fn closure_module_commits_once_and_carries_namespace() {
        let mut e = Engine::new();
        e.module("node:fs", |m| {
            m.doc("fs");
            m.alias("fs");
            m.namespace("fs");
        });
        let reg = e.registry();
        assert_eq!(reg.module_count(), 1);
        let m = reg.module("node:fs").expect("node:fs");
        assert_eq!(m.namespace.as_deref(), Some("fs"));
        // the bare alias resolves too, and to the SAME namespace-carrying module
        let aliased = reg.module("fs").expect("bare fs alias");
        assert_eq!(aliased.namespace.as_deref(), Some("fs"));
        // the namespace object itself resolves by its OWN (distinct) lookup
        let by_ns = reg.namespace("fs").expect("fs namespace object");
        assert_eq!(by_ns.name, "fs");
    }

    #[test]
    fn closure_global_installs_immediately() {
        let mut e = Engine::new();
        extern "C" fn toy() -> i64 {
            1
        }
        e.global(|g| {
            g.constant("TOY", toy as *const u8, crate::abi::AbiType::I64);
        });
        assert!(e.registry().global("TOY").is_some());
    }
}
