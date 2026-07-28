//! O registry: a fonte única que o codegen lê para resolver módulos, classes e
//! globais. Builtins (via os crates de camada) e módulos externos (`.dll`/`.so`)
//! populam o mesmo registry; o codegen não distingue origem.

use std::collections::HashMap;

use crate::abi::MemberKind;
use crate::{FnPtr, Member};

/// Um módulo importável (`import { x } from "<scheme>:<name>"`).
#[derive(Debug, Clone)]
pub struct Module {
    /// Esquema de import: `"rts"`, `"node"`, `"plugin"`, custom.
    pub scheme: String,
    /// Nome do módulo, ex.: `"io"`, `"fs"`.
    pub name: String,
    /// Resumo (para `rts apis` / docs).
    pub doc: String,
    /// Membros: funções, constantes, variáveis.
    pub members: Vec<Member>,
    /// PRIVATE namespace: its symbols are usable by embedded stdlib TS (engine
    /// includes) but are NOT exposed to the final user program. Visibility
    /// enforcement is applied by the import resolver (a later increment); the
    /// flag is recorded here.
    pub private: bool,
    /// Load-order preference (higher loads earlier). Lets embedded stdlib that
    /// depends on another namespace order its load. Default 0.
    pub load_order: i32,
    /// A JS-visible NAMESPACE OBJECT this module publishes (`fs.readFile(...)`,
    /// reachable with no import), DISTINCT from the import specifier
    /// (`"node:fs"`). `None` = the module has no bare-namespace surface — the
    /// common case. See [`ModuleBuilder::namespace`]/[`ModuleScope::namespace`].
    pub namespace: Option<String>,
}

/// Uma classe global (`new Date()`, `d.getFullYear()`) — sem import.
#[derive(Debug, Clone)]
pub struct Class {
    /// Nome JS da classe, ex.: `"Date"`, `"String"`.
    pub name: String,
    /// Resumo.
    pub doc: String,
    /// Membros: construtores, métodos estáticos/instância, getters/setters,
    /// constantes.
    pub members: Vec<Member>,
    /// Símbolo runtime do predicado `x instanceof <Class>` (`fn(handle) -> i64`,
    /// 1/0). Deixa o codegen resolver instanceof de classes NÃO-primordiais pelo
    /// Registry, sem nomear a classe no motor. `None` = sem predicado próprio
    /// (primordiais nomeadas pelo motor, ou classes que nunca casam).
    pub instanceof_predicate: Option<String>,
    /// Nome da classe base (`#[rtse::class("Child", extends = "Parent")]`). O
    /// codegen sobe a cadeia p/ `x instanceof Parent` sobre um `Child`. Os MÉTODOS
    /// herdados NÃO vêm por cadeia (o modelo `Entry::Rtse` downcasta ao tipo
    /// concreto — um método do pai não opera sobre a box do filho): o filho compõe
    /// o pai num campo e ENCAMINHA os métodos (autoria), como o Rust idiomático.
    pub parent: Option<String>,
}

impl Class {
    /// Resolve um método de instância por nome (ou alias) e aridade da chamada,
    /// honrando overloads + variádicos (docs/specs/rts-engine-dispatch.md §4.3):
    /// aridade exata → aceita-via-variádico → first-by-name.
    pub fn resolve_instance_method(&self, name: &str, n_args: usize) -> Option<&Member> {
        let named =
            |m: &&Member| matches!(m.kind, MemberKind::InstanceMethod) && m.matches_name(name);
        self.members
            .iter()
            .find(|m| named(m) && m.sig.explicit_arity() == n_args)
            .or_else(|| {
                self.members.iter().find(|m| {
                    named(m) && m.variadic && n_args >= m.sig.explicit_arity().saturating_sub(1)
                })
            })
            .or_else(|| self.members.iter().find(named))
    }

    /// Getter de instância (`inst.prop`) por nome.
    pub fn instance_getter(&self, name: &str) -> Option<&Member> {
        self.members
            .iter()
            .find(|m| matches!(m.kind, MemberKind::InstanceGetter) && m.matches_name(name))
    }

    /// Setter de instância (`inst.prop = v`) por nome. Getter sem setter = read-only.
    pub fn instance_setter(&self, name: &str) -> Option<&Member> {
        self.members
            .iter()
            .find(|m| matches!(m.kind, MemberKind::InstanceSetter) && m.matches_name(name))
    }
}

/// A tabela central. Montada em runtime a partir das chamadas do builder.
#[derive(Debug, Clone, Default)]
pub struct Registry {
    /// Módulos por chave `"<scheme>:<name>"`.
    modules: HashMap<String, Module>,
    /// Ordem de inserção das chaves de `modules` — `modules()` itera por aqui
    /// p/ saída DETERMINÍSTICA (HashMap::values() varia run-a-run → quebra o
    /// `rts.d.ts` byte-idêntico). Chave reinserida não duplica.
    module_order: Vec<String>,
    /// Namespace OBJECT name → module key (`"<scheme>:<name>"`). Populated when
    /// a module declares [`Module::namespace`]. Distinct from `modules`'s own
    /// key: a namespace is a bare, importless JS surface, a module key is an
    /// import specifier.
    namespaces: HashMap<String, String>,
    /// Classes globais por nome JS.
    classes: HashMap<String, Class>,
    /// Ordem de inserção das classes (mesma razão de `module_order`).
    class_order: Vec<String>,
    /// Membros de escopo global (bare, sem import): `NaN`, `isNaN`, vars globais.
    globals: HashMap<String, Member>,
    /// `symbol -> fn_ptr` de TODO membro com extern — substitui a lista
    /// `add_fn!` do JIT. O loader injeta isto em `JITBuilder::symbol`.
    jit_symbols: HashMap<String, FnPtr>,
    /// Embedded TypeScript stdlib sources (engine `include`s), compiled as a
    /// PRELUDE ahead of the user program by the new engine.
    pub(crate) includes: Vec<String>,
}

impl Registry {
    /// Registry vazio.
    pub fn new() -> Self {
        Self::default()
    }

    /// Resolve `"<scheme>:<name>"` (ou bare `name`, default scheme `rts`).
    pub fn module(&self, key: &str) -> Option<&Module> {
        if let Some(m) = self.modules.get(key) {
            return Some(m);
        }
        // bare name → tenta scheme rts
        if !key.contains(':') {
            return self.modules.get(&format!("rts:{key}"));
        }
        None
    }

    /// Resolve uma classe global por nome.
    pub fn class(&self, name: &str) -> Option<&Class> {
        self.classes.get(name)
    }

    /// Resolve a module by its published NAMESPACE OBJECT name (`fs` in
    /// `fs.readFile(...)`), distinct from its import specifier.
    pub fn namespace(&self, name: &str) -> Option<&Module> {
        self.modules.get(self.namespaces.get(name)?)
    }

    /// Resolve um membro de escopo global (bare ident).
    pub fn global(&self, name: &str) -> Option<&Member> {
        self.globals.get(name)
    }

    /// The embedded TS stdlib sources (engine `include`s), in registration order.
    pub fn includes(&self) -> &[String] {
        &self.includes
    }

    /// Resolve o ponteiro nativo de um símbolo (para registrar no JIT).
    pub fn jit_symbol(&self, symbol: &str) -> Option<FnPtr> {
        self.jit_symbols.get(symbol).copied()
    }

    /// Itera todos os `(symbol, fn_ptr)` — o loader do JIT injeta cada um.
    pub fn jit_symbols(&self) -> impl Iterator<Item = (&str, FnPtr)> {
        self.jit_symbols.iter().map(|(s, p)| (s.as_str(), *p))
    }

    /// Itera todos os módulos em ordem de inserção (determinístico).
    pub fn modules(&self) -> impl Iterator<Item = &Module> {
        self.module_order
            .iter()
            .filter_map(move |k| self.modules.get(k))
    }

    /// Itera todas as classes em ordem de inserção (determinístico).
    pub fn classes(&self) -> impl Iterator<Item = &Class> {
        self.class_order
            .iter()
            .filter_map(move |k| self.classes.get(k))
    }

    pub fn module_count(&self) -> usize {
        self.modules.len()
    }
    pub fn class_count(&self) -> usize {
        self.classes.len()
    }

    // ---- inserção (usada pelo builder) ----

    pub(crate) fn insert_module(&mut self, module: Module) {
        for m in &module.members {
            // Membros `alias`/`external` carregam fn_ptr null — o símbolo é
            // resolvido pela ns dona; não sobrescrever com null.
            if !m.fn_ptr.0.is_null() {
                self.jit_symbols.insert(m.symbol.clone(), m.fn_ptr);
            }
        }
        let key = format!("{}:{}", module.scheme, module.name);
        if let Some(ns) = &module.namespace {
            self.namespaces.insert(ns.clone(), key.clone());
        }
        if self.modules.insert(key.clone(), module).is_none() {
            self.module_order.push(key); // 1ª inserção → entra na ordem
        }
    }

    /// Insere o módulo sob uma CHAVE explícita (entrada de ALIAS). Resolve como
    /// o canônico e harvesta os mesmos símbolos JIT, mas NÃO entra em
    /// `module_order` — assim `modules()` (usado por `rts.d.ts`/`apis`) não
    /// duplica o módulo. Ver `ModuleBuilder::alias`.
    pub(crate) fn insert_module_at(&mut self, key: String, module: Module) {
        for m in &module.members {
            if !m.fn_ptr.0.is_null() {
                self.jit_symbols.insert(m.symbol.clone(), m.fn_ptr);
            }
        }
        self.modules.insert(key, module);
    }

    /// Register a class's members. A SECOND call for the same class name MERGES
    /// (appends `members`, fills `doc`/`instanceof_predicate`/`parent` only where
    /// the existing entry has none) instead of replacing — this lets a class be
    /// composed from more than one register call (e.g. a `#[rtse::class]`-derived
    /// `register` for ctor/methods plus a hand-written call adding constant
    /// `Member`s, which the class macro's `impl` block cannot express since a
    /// Rust `const` item is not a `Fn`). Every pre-existing single-call class
    /// keeps identical behaviour (nothing to merge into on the first call).
    pub(crate) fn insert_class(&mut self, class: Class) {
        for m in &class.members {
            if !m.fn_ptr.0.is_null() {
                self.jit_symbols.insert(m.symbol.clone(), m.fn_ptr);
            }
        }
        let name = class.name.clone();
        if let Some(existing) = self.classes.get_mut(&name) {
            // DEDUPLICATE BY SYMBOL. A member's symbol IS its identity — the same
            // symbol is the same function, so a second registration of it is a
            // repeat, not an overload.
            //
            // This matters because several classes REOPEN themselves: `register(e)`
            // runs the macro, the caller reads the members back out, then reopens
            // `.class(X)` and re-emits them alongside hand-written residuals
            // (`headers`, `url`, `url::search_params`, `date` all do this — see
            // `register_headers_class_spec`). That pattern was written against
            // REPLACE semantics, where re-emitting was how you kept the macro's
            // members. Under a naive merge it doubles every one of them, and a
            // duplicated constructor turns a resolvable `new Headers(x)` into an
            // ambiguous overload the lowering must bail on.
            //
            // It surfaced ONLY in AOT: the JIT prunes the prelude, so a program
            // that never touches `Response` never lowers its ctor, while AOT skips
            // the prune and compiles everything.
            let mut seen: std::collections::HashSet<String> = existing
                .members
                .iter()
                .map(|m| m.symbol.clone())
                .collect();
            existing
                .members
                .extend(class.members.into_iter().filter(|m| seen.insert(m.symbol.clone())));
            if existing.doc.is_empty() {
                existing.doc = class.doc;
            }
            if existing.instanceof_predicate.is_none() {
                existing.instanceof_predicate = class.instanceof_predicate;
            }
            if existing.parent.is_none() {
                existing.parent = class.parent;
            }
        } else {
            self.classes.insert(name.clone(), class);
            self.class_order.push(name);
        }
    }

    pub(crate) fn insert_global(&mut self, member: Member) {
        self.jit_symbols
            .insert(member.symbol.clone(), member.fn_ptr);
        self.globals.insert(member.name.clone(), member);
    }
}
