//! O registry: a fonte única que o codegen lê para resolver módulos, classes e
//! globais. Builtins (via os crates de camada) e módulos externos (`.dll`/`.so`)
//! populam o mesmo registry; o codegen não distingue origem.

use std::collections::HashMap;

use crate::{FnPtr, Member};
use crate::abi::MemberKind;

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
}

impl Class {
    /// Resolve um método de instância por nome (ou alias) e aridade da chamada,
    /// honrando overloads + variádicos. Mesma lógica de
    /// `GlobalClassSpec::resolve_instance_method` (RTS_ENGINE.md §4.3): aridade
    /// exata → aceita-via-variádico → first-by-name.
    pub fn resolve_instance_method(&self, name: &str, n_args: usize) -> Option<&Member> {
        let named = |m: &&Member| {
            matches!(m.kind, MemberKind::InstanceMethod) && m.matches_name(name)
        };
        self.members
            .iter()
            .find(|m| named(m) && m.sig.explicit_arity() == n_args)
            .or_else(|| {
                self.members.iter().find(|m| {
                    named(m)
                        && m.variadic
                        && n_args >= m.sig.explicit_arity().saturating_sub(1)
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
    /// Classes globais por nome JS.
    classes: HashMap<String, Class>,
    /// Membros de escopo global (bare, sem import): `NaN`, `isNaN`, vars globais.
    globals: HashMap<String, Member>,
    /// `symbol -> fn_ptr` de TODO membro com extern — substitui a lista
    /// `add_fn!` do JIT. O loader injeta isto em `JITBuilder::symbol`.
    jit_symbols: HashMap<String, FnPtr>,
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

    /// Resolve um membro de escopo global (bare ident).
    pub fn global(&self, name: &str) -> Option<&Member> {
        self.globals.get(name)
    }

    /// Resolve o ponteiro nativo de um símbolo (para registrar no JIT).
    pub fn jit_symbol(&self, symbol: &str) -> Option<FnPtr> {
        self.jit_symbols.get(symbol).copied()
    }

    /// Itera todos os `(symbol, fn_ptr)` — o loader do JIT injeta cada um.
    pub fn jit_symbols(&self) -> impl Iterator<Item = (&str, FnPtr)> {
        self.jit_symbols.iter().map(|(s, p)| (s.as_str(), *p))
    }

    /// Itera todos os módulos (para `builtin_module_keys`, geração de `.d.ts`, …).
    pub fn modules(&self) -> impl Iterator<Item = &Module> {
        self.modules.values()
    }

    /// Itera todas as classes.
    pub fn classes(&self) -> impl Iterator<Item = &Class> {
        self.classes.values()
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
        self.modules.insert(key, module);
    }

    pub(crate) fn insert_class(&mut self, class: Class) {
        for m in &class.members {
            if !m.fn_ptr.0.is_null() {
                self.jit_symbols.insert(m.symbol.clone(), m.fn_ptr);
            }
        }
        self.classes.insert(class.name.clone(), class);
    }

    pub(crate) fn insert_global(&mut self, member: Member) {
        self.jit_symbols.insert(member.symbol.clone(), member.fn_ptr);
        self.globals.insert(member.name.clone(), member);
    }
}
