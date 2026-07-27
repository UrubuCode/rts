//! Module registration: the accumulator ([`ModuleAccum`]) shared by BOTH the
//! fluent, `.done()`-terminated [`ModuleBuilder`] and the closure-scoped
//! [`super::closure::ModuleScope`]. Neither depends on the other's internals —
//! both are thin front-ends pushing into the same accumulator and calling the
//! same [`ModuleAccum::commit`] exactly once.

use super::support::{fn_symbol, module_stem, simple_member, ts_of};
use crate::Sig;
use crate::abi::AbiType;
use crate::member::{Member, VarKind};
use crate::registry::Module;
use crate::{Engine, MemberFlags, MemberKind};

/// Accumulates one module's fields until commit. Not `pub` — reached only
/// through [`ModuleBuilder`] or [`super::closure::ModuleScope`].
#[derive(Default)]
pub(crate) struct ModuleAccum {
    pub(crate) scheme: String,
    pub(crate) name: String,
    pub(crate) doc: String,
    pub(crate) members: Vec<Member>,
    pub(crate) private: bool,
    pub(crate) load_order: i32,
    pub(crate) aliases: Vec<String>,
    pub(crate) namespace: Option<String>,
}

impl ModuleAccum {
    pub(crate) fn new(scheme: &str, name: &str) -> Self {
        Self {
            scheme: scheme.to_string(),
            name: name.to_string(),
            ..Default::default()
        }
    }

    /// Finaliza: insere o módulo (e os seus símbolos) no registry, mais uma
    /// entrada por alias na keyspace `rts:` que o resolver de import consulta.
    /// Consuming `self` makes a double-commit a type error, not a runtime bug.
    pub(crate) fn commit(self, engine: &mut Engine) {
        let module = Module {
            scheme: self.scheme,
            name: self.name,
            doc: self.doc,
            members: self.members,
            private: self.private,
            load_order: self.load_order,
            namespace: self.namespace,
        };
        for alias in &self.aliases {
            engine
                .registry_mut()
                .insert_module_at(format!("rts:{alias}"), module.clone());
        }
        engine.registry_mut().insert_module(module);
    }
}

/// Acumula os membros de um módulo; insere no registry em [`ModuleBuilder::done`].
pub struct ModuleBuilder<'e> {
    engine: &'e mut Engine,
    accum: ModuleAccum,
}

impl<'e> ModuleBuilder<'e> {
    pub(crate) fn new(engine: &'e mut Engine, scheme: &str, name: &str) -> Self {
        Self {
            engine,
            accum: ModuleAccum::new(scheme, name),
        }
    }

    /// Resumo do módulo.
    pub fn doc(mut self, doc: &str) -> Self {
        self.accum.doc = doc.to_string();
        self
    }

    /// Mark this namespace PRIVATE: its symbols are usable by embedded stdlib TS
    /// (engine includes) but are NOT exposed to the final user program.
    pub fn private(mut self) -> Self {
        self.accum.private = true;
        self
    }

    /// Load-order preference for this namespace (higher loads earlier).
    pub fn order(mut self, n: i32) -> Self {
        self.accum.load_order = n;
        self
    }

    /// Declare an alternate bare name the module ALSO resolves under.
    pub fn alias(mut self, name: &str) -> Self {
        self.accum.aliases.push(name.to_string());
        self
    }

    /// Publish a JS-visible NAMESPACE OBJECT for this module (`fs.readFile(...)`,
    /// reachable with no import) — DISTINCT from the module's import specifier.
    /// See [`crate::registry::Module::namespace`].
    pub fn namespace(mut self, name: &str) -> Self {
        self.accum.namespace = Some(name.to_string());
        self
    }

    /// Uma função: `ns.fn(args)`. `ptr` = o `extern "C"` nativo (`f as *const u8`).
    pub fn function(mut self, name: &str, ptr: *const u8, sig: Sig) -> Self {
        let stem = module_stem(&self.accum.name);
        self.accum
            .members
            .push(simple_member(&stem, name, MemberKind::Function, sig, ptr));
        self
    }

    /// Uma constante zero-arg `ns.K` (sem parênteses). `ptr` = getter `fn() -> ty`.
    pub fn constant(mut self, name: &str, ptr: *const u8, ty: AbiType) -> Self {
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

    /// Uma variável de módulo. `Const` → só getter (READONLY); `Mutable` → getter
    /// + setter.
    pub fn variable(
        mut self,
        name: &str,
        kind: VarKind,
        get: *const u8,
        set: Option<*const u8>,
        ty: AbiType,
    ) -> Self {
        push_variable(&mut self.accum.members, &self.accum.name, name, kind, get, set, ty);
        self
    }

    /// Escape hatch: empurra um [`Member`] totalmente construído.
    pub fn member(mut self, member: Member) -> Self {
        self.accum.members.push(member);
        self
    }

    pub fn done(self) {
        self.accum.commit(self.engine);
    }
}

/// Shared `variable()` body (fluent + closure scope call this).
pub(crate) fn push_variable(
    members: &mut Vec<Member>,
    module_name: &str,
    name: &str,
    kind: VarKind,
    get: *const u8,
    set: Option<*const u8>,
    ty: AbiType,
) {
    let stem = module_stem(module_name);
    let mut getter = Member {
        name: name.to_string(),
        kind: MemberKind::VarGetter,
        symbol: format!("{}_GET", fn_symbol(&stem, name)),
        sig: Sig::new(Vec::new(), ty),
        fn_ptr: crate::member::FnPtr(get),
        flags: if kind == VarKind::Mutable {
            MemberFlags::MUTABLE
        } else {
            MemberFlags::READONLY
        },
        aliases: Vec::new(),
        variadic: false,
        ts_signature: String::new(),
        doc: String::new(),
        ret_class: None,
        pure: false,
        
        emit: None,
    };
    getter.ts_signature = format!("{name}: {}", ts_of(ty));
    members.push(getter);
    if kind == VarKind::Mutable {
        if let Some(set) = set {
            members.push(Member {
                name: name.to_string(),
                kind: MemberKind::VarSetter,
                symbol: format!("{}_SET", fn_symbol(&stem, name)),
                sig: Sig::new(vec![ty], AbiType::Void),
                fn_ptr: crate::member::FnPtr(set),
                flags: MemberFlags::MUTABLE,
                aliases: Vec::new(),
                variadic: false,
                ts_signature: String::new(),
                doc: String::new(),
                ret_class: None,
                pure: false,
                
                emit: None,
            });
        }
    }
}
