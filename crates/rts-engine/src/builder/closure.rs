//! Closure-scoped registration (F1/F2/F3 of
//! `docs/specs/rts-macro-single-source.md`): [`ModuleScope`] and
//! [`GlobalScope`], the nesting-capable alternative to the fluent
//! `.done()`-terminated chain.
//!
//! ```ignore
//! e.module("node:fs", |m| {
//!     m.alias("fs");
//!     m.namespace("fs");
//!     m.registry(READ_FILE);   // a free function (a `Member`)
//!     m.registry(Stats::register); // a class (a `fn(&mut Engine)`)
//! });
//!
//! e.global(|g| {
//!     g.registry(STRING_CLASS);
//!     g.registry(PARSE_INT);
//! });
//! ```
//!
//! # Why a closure and not a fluent chain
//!
//! A fluent chain cannot nest — every sub-declaration has to be a method on the
//! SAME builder, which is why `alias`/`member` live at one flat level on
//! [`super::module::ModuleBuilder`]. A closure gives the caller a mutable scope
//! it can pass to helper functions (`add_members(m, which)`), branch on, or
//! loop over, before the module is committed.
//!
//! # The commit-once guarantee
//!
//! [`ModuleScope`] has NO public commit/finish method — [`super::Engine::module`]
//! is the only place [`super::module::ModuleAccum::commit`] is called, and it is
//! called exactly once, AFTER the closure returns. There is therefore no
//! sequence of calls on `ModuleScope` that can commit twice or commit a partial
//! module — the accumulate/commit split is enforced by what the type does NOT
//! expose, not by caller discipline. This is the same trap `#[rtse::constant]`
//! documents (`ModuleBuilder::done` REPLACES the module at its key), avoided
//! here by construction rather than convention.

use super::module::{ModuleAccum, push_variable};
use crate::Sig;
use crate::abi::AbiType;
use crate::member::{Member, VarKind};
use crate::{Engine, MemberKind};

use super::support::{module_stem, simple_member};

/// The mutable scope handed to an [`Engine::module`] closure. Accumulates one
/// module's fields; committed once, by the caller of the closure, never by
/// `ModuleScope` itself.
pub struct ModuleScope<'a> {
    pub(crate) engine: &'a mut Engine,
    pub(crate) accum: ModuleAccum,
}

impl ModuleScope<'_> {
    /// Resumo do módulo.
    pub fn doc(&mut self, doc: &str) -> &mut Self {
        self.accum.doc = doc.to_string();
        self
    }

    /// Mark this namespace PRIVATE — see [`super::module::ModuleBuilder::private`].
    pub fn private(&mut self) -> &mut Self {
        self.accum.private = true;
        self
    }

    /// Load-order preference — see [`super::module::ModuleBuilder::order`].
    pub fn order(&mut self, n: i32) -> &mut Self {
        self.accum.load_order = n;
        self
    }

    /// Declare an alternate bare name the module ALSO resolves under (the bare
    /// `fs` in `import … from "fs"` resolving `"node:fs"`).
    pub fn alias(&mut self, name: &str) -> &mut Self {
        self.accum.aliases.push(name.to_string());
        self
    }

    /// Publish a JS-visible NAMESPACE OBJECT for this module — see
    /// [`crate::registry::Module::namespace`]. DISTINCT from the module
    /// specifier: `m.namespace("fs")` makes `fs.readFile(...)` resolve with no
    /// import, independent of whichever specifier(s) this module is imported by.
    pub fn namespace(&mut self, name: &str) -> &mut Self {
        self.accum.namespace = Some(name.to_string());
        self
    }

    /// Uma função: `ns.fn(args)`.
    pub fn function(&mut self, name: &str, ptr: *const u8, sig: Sig) -> &mut Self {
        let stem = module_stem(&self.accum.name);
        self.accum
            .members
            .push(simple_member(&stem, name, MemberKind::Function, sig, ptr));
        self
    }

    /// Uma constante zero-arg `ns.K`.
    pub fn constant(&mut self, name: &str, ptr: *const u8, ty: AbiType) -> &mut Self {
        let stem = module_stem(&self.accum.name);
        self.accum.members.push(simple_member(
            &stem,
            name,
            MemberKind::Constant,
            Sig::new(Vec::new(), ty),
            ptr,
        ));
        self
    }

    /// Uma variável de módulo — see [`super::module::ModuleBuilder::variable`].
    pub fn variable(
        &mut self,
        name: &str,
        kind: VarKind,
        get: *const u8,
        set: Option<*const u8>,
        ty: AbiType,
    ) -> &mut Self {
        push_variable(&mut self.accum.members, &self.accum.name, name, kind, get, set, ty);
        self
    }

    /// Escape hatch: empurra um [`Member`] totalmente construído.
    pub fn member(&mut self, member: Member) -> &mut Self {
        self.accum.members.push(member);
        self
    }

    /// Add a [`RegistryItem`] — a free function (`Member`) or a class
    /// (`fn(&mut Engine)`, the `register` a `#[rtse::class]` impl emits). See
    /// the [`RegistryItem`] doc for how the two are unified.
    pub fn registry<T: RegistryItem>(&mut self, item: T) -> &mut Self {
        item.install(self);
        self
    }
}

/// The mutable scope handed to an [`Engine::global`] closure.
pub struct GlobalScope<'a> {
    pub(crate) engine: &'a mut Engine,
}

impl GlobalScope<'_> {
    /// Função global: `isNaN(x)`.
    pub fn function(&mut self, name: &str, ptr: *const u8, sig: Sig) -> &mut Self {
        let m = simple_member("GLOBAL", name, MemberKind::Function, sig, ptr);
        self.engine.registry_mut().insert_global(m);
        self
    }

    /// Constante global: `NaN`, `Infinity`.
    pub fn constant(&mut self, name: &str, ptr: *const u8, ty: AbiType) -> &mut Self {
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
    pub fn member(&mut self, member: Member) -> &mut Self {
        self.engine.registry_mut().insert_global(member);
        self
    }

    /// Add a [`RegistryItem`] — a free function/constant (`Member`) or a class
    /// (`fn(&mut Engine)`). A global has no batching to protect (each insert
    /// commits immediately), so a class item runs its `register` right away.
    pub fn registry<T: GlobalRegistryItem>(&mut self, item: T) -> &mut Self {
        item.install(self);
        self
    }
}

/// What `ModuleScope::registry(...)` accepts: either kind installs itself
/// differently, which is exactly why this is a trait and not one shared
/// struct — a free function has no receiver and pushes a `Member` into the
/// module's own member list, while a class has its own top-level Registry slot
/// (`Registry::classes`, keyed by class name, not nested under any module) and
/// registers itself by calling its generated `register(&mut Engine)`.
///
/// `#[rtse::function]` emits a `pub fn <name>_entry() -> Member` (the `Member`
/// impl below covers it directly); `#[rtse::class]`'s `gen_impl` already emits
/// `pub fn register(e: &mut Engine)`, which IS an `fn(&mut Engine)` and needs no
/// new macro output to satisfy this trait.
pub trait RegistryItem {
    fn install(self, scope: &mut ModuleScope);
}

impl RegistryItem for Member {
    fn install(self, scope: &mut ModuleScope) {
        scope.accum.members.push(self);
    }
}

impl RegistryItem for fn(&mut Engine) {
    fn install(self, scope: &mut ModuleScope) {
        self(scope.engine);
    }
}

/// The [`GlobalScope`] counterpart of [`RegistryItem`] — same two shapes
/// (`Member` / `fn(&mut Engine)`), different install target (global scope has
/// no local member list to accumulate into; a `Member` commits straight into
/// `Registry::globals`).
pub trait GlobalRegistryItem {
    fn install(self, scope: &mut GlobalScope);
}

impl GlobalRegistryItem for Member {
    fn install(self, scope: &mut GlobalScope) {
        scope.engine.registry_mut().insert_global(self);
    }
}

impl GlobalRegistryItem for fn(&mut Engine) {
    fn install(self, scope: &mut GlobalScope) {
        self(scope.engine);
    }
}
