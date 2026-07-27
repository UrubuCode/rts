//! Class registration: [`ClassBuilder`], the fluent `.done()`-terminated
//! accumulator for a global class (`new Date()`, `d.getFullYear()`).

use super::support::{class_stem, simple_member};
use crate::Sig;
use crate::member::Member;
use crate::registry::Class;
use crate::{Engine, MemberKind};

/// Acumula os membros de uma classe global.
pub struct ClassBuilder<'e> {
    engine: &'e mut Engine,
    name: String,
    doc: String,
    members: Vec<Member>,
    instanceof_predicate: Option<String>,
    parent: Option<String>,
}

impl<'e> ClassBuilder<'e> {
    pub(crate) fn new(engine: &'e mut Engine, name: &str) -> Self {
        Self {
            engine,
            name: name.to_string(),
            doc: String::new(),
            members: Vec::new(),
            instanceof_predicate: None,
            parent: None,
        }
    }

    pub fn doc(mut self, doc: &str) -> Self {
        self.doc = doc.to_string();
        self
    }

    /// Declara o símbolo runtime do predicado `x instanceof <Class>`
    /// (`fn(handle) -> i64`). Usado por classes NÃO-primordiais para que o
    /// codegen resolva instanceof pelo Registry sem nomear a classe.
    pub fn instanceof_predicate(mut self, symbol: &str) -> Self {
        self.instanceof_predicate = Some(symbol.to_string());
        self
    }

    /// Construtor: `new Class(args)`. `ptr` = `fn(args) -> Handle`.
    pub fn constructor(mut self, ptr: *const u8, sig: Sig) -> Self {
        let stem = class_stem(&self.name);
        self.members.push(simple_member(
            &stem,
            "new",
            MemberKind::Constructor,
            sig,
            ptr,
        ));
        self
    }

    /// Método de instância. `sig` inclui o `Handle` do receiver no slot 0.
    pub fn method(mut self, name: &str, ptr: *const u8, sig: Sig) -> Self {
        let stem = class_stem(&self.name);
        self.members.push(simple_member(
            &stem,
            name,
            MemberKind::InstanceMethod,
            sig,
            ptr,
        ));
        self
    }

    /// Método estático: `Class.fn(args)`.
    pub fn static_method(mut self, name: &str, ptr: *const u8, sig: Sig) -> Self {
        let stem = class_stem(&self.name);
        self.members.push(simple_member(
            &stem,
            name,
            MemberKind::StaticMethod,
            sig,
            ptr,
        ));
        self
    }

    /// Getter de instância (`inst.prop`, sem parênteses). `sig` = `(Handle) -> ty`.
    pub fn getter(mut self, name: &str, ptr: *const u8, sig: Sig) -> Self {
        let stem = class_stem(&self.name);
        self.members.push(simple_member(
            &stem,
            name,
            MemberKind::InstanceGetter,
            sig,
            ptr,
        ));
        self
    }

    /// Setter de instância (`inst.prop = v`). `sig` = `(Handle, ty) -> Void`.
    pub fn setter(mut self, name: &str, ptr: *const u8, sig: Sig) -> Self {
        let stem = class_stem(&self.name);
        self.members.push(simple_member(
            &stem,
            name,
            MemberKind::InstanceSetter,
            sig,
            ptr,
        ));
        self
    }

    pub fn member(mut self, member: Member) -> Self {
        self.members.push(member);
        self
    }

    /// Declara a classe base: `#[rtse::class("Child", extends = "Parent")]`.
    pub fn extends(mut self, parent: &str) -> Self {
        self.parent = Some(parent.to_string());
        self
    }

    pub fn done(self) {
        self.engine.registry_mut().insert_class(Class {
            name: self.name,
            doc: self.doc,
            members: self.members,
            instanceof_predicate: self.instanceof_predicate,
            parent: self.parent,
        });
    }
}
