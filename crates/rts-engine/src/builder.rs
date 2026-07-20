//! O builder: a API fluente que as camadas (`rts-std`, `rts-node`, `rts-browser`,
//! módulos externos) usam para registrar a sua superfície no [`Registry`].

use crate::Sig;
use crate::abi::{AbiType, MemberFlags, MemberKind};
use crate::member::{FnPtr, Member, VarKind};
use crate::registry::{Class, Module, Registry};

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

    /// Começa a registrar um módulo importável `"<scheme>:<name>"`. Use
    /// [`Engine::ns`] para o atalho do esquema `rts`.
    pub fn module<'e>(&'e mut self, scheme: &str, name: &str) -> ModuleBuilder<'e> {
        ModuleBuilder {
            engine: self,
            scheme: scheme.to_string(),
            name: name.to_string(),
            doc: String::new(),
            members: Vec::new(),
            private: false,
            load_order: 0,
            aliases: Vec::new(),
        }
    }

    /// Começa a registrar um módulo importável a partir de UM spec-string:
    ///  - `"bigfloat"`            → esquema `rts` (`rts:bigfloat`);
    ///  - `"node:fs"`             → esquema explícito (`node:fs`);
    ///  - `"node:fs, fs"`         → canônico + ALIAS(es) bare (vírgula-separados),
    ///                              cada alias resolvível como `rts:<alias>`.
    /// O primeiro token (`scheme:name` ou bare `name`) é o canônico; os demais
    /// são aliases. Assim o register fn nomeia o módulo inteiramente como DADO,
    /// sem string de identidade duplicada na tabela de registro.
    pub fn ns<'e>(&'e mut self, spec: &str) -> ModuleBuilder<'e> {
        let mut parts = spec.split(',').map(str::trim);
        let primary = parts.next().unwrap_or("");
        let (scheme, name) = primary.split_once(':').unwrap_or(("rts", primary));
        let mut b = self.module(scheme, name);
        for alias in parts.filter(|a| !a.is_empty()) {
            b = b.alias(alias);
        }
        b
    }

    /// Começa a registrar uma classe global (`new <Name>()`).
    pub fn class<'e>(&'e mut self, name: &str) -> ClassBuilder<'e> {
        ClassBuilder {
            engine: self,
            name: name.to_string(),
            doc: String::new(),
            members: Vec::new(),
            instanceof_predicate: None,
        }
    }

    /// Começa a registrar membros de escopo global (bare, sem import).
    pub fn global<'e>(&'e mut self) -> GlobalBuilder<'e> {
        GlobalBuilder { engine: self }
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

/// Stem de símbolo de um módulo: `NS_<NAME>`.
fn module_stem(name: &str) -> String {
    format!("NS_{}", name.to_uppercase())
}
/// Stem de símbolo de uma classe: `GL_<NAME>`.
fn class_stem(name: &str) -> String {
    format!("GL_{}", name.to_uppercase())
}
/// Símbolo canônico `__RTS_FN_<STEM>_<MEMBER>`.
fn fn_symbol(stem: &str, member: &str) -> String {
    format!("__RTS_FN_{stem}_{}", member.to_uppercase())
}

/// Constrói um [`Member`] simples (fn/const) com símbolo derivado.
fn simple_member(stem: &str, name: &str, kind: MemberKind, sig: Sig, ptr: *const u8) -> Member {
    Member {
        name: name.to_string(),
        kind,
        symbol: fn_symbol(stem, name),
        sig,
        fn_ptr: FnPtr(ptr),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: String::new(),
        doc: String::new(),
        pure: false,
        intrinsic: None,
    }
}

/// Acumula os membros de um módulo; insere no registry em [`ModuleBuilder::done`].
pub struct ModuleBuilder<'e> {
    engine: &'e mut Engine,
    scheme: String,
    name: String,
    doc: String,
    members: Vec<Member>,
    private: bool,
    load_order: i32,
    /// Alternate bare names the module ALSO resolves under (registered in the
    /// `rts:` keyspace the import resolver strips to). Lets a scheme'd module
    /// (`e.module("node", "querystring")`) declare its back-compat / bare alias
    /// as DATA, so the codegen never hardcodes a `node`→`rts` mapping.
    aliases: Vec<String>,
}

impl ModuleBuilder<'_> {
    /// Resumo do módulo.
    pub fn doc(mut self, doc: &str) -> Self {
        self.doc = doc.to_string();
        self
    }

    /// Mark this namespace PRIVATE: its symbols are usable by embedded stdlib TS
    /// (engine includes) but are NOT exposed to the final user program. (Visibility
    /// enforcement is applied by the import resolver — a later increment; the flag
    /// is recorded here.)
    pub fn private(mut self) -> Self {
        self.private = true;
        self
    }

    /// Load-order preference for this namespace (higher loads earlier). Lets
    /// embedded stdlib that depends on another namespace order its load. Default 0.
    pub fn order(mut self, n: i32) -> Self {
        self.load_order = n;
        self
    }

    /// Declare an alternate bare name the module ALSO resolves under. Use it so a
    /// scheme'd module names itself by its real specifier while still resolving
    /// through the import path — e.g. `e.module("node", "querystring")
    /// .alias("querystring")` gives canonical key `node:querystring` PLUS a
    /// resolvable `rts:querystring`, with no `node`→`rts` special-case in codegen.
    pub fn alias(mut self, name: &str) -> Self {
        self.aliases.push(name.to_string());
        self
    }

    /// Uma função: `ns.fn(args)`. `ptr` = o `extern "C"` nativo (`f as *const u8`).
    pub fn function(mut self, name: &str, ptr: *const u8, sig: Sig) -> Self {
        let stem = module_stem(&self.name);
        self.members
            .push(simple_member(&stem, name, MemberKind::Function, sig, ptr));
        self
    }

    /// Uma constante zero-arg `ns.K` (sem parênteses). `ptr` = getter `fn() -> ty`.
    pub fn constant(mut self, name: &str, ptr: *const u8, ty: AbiType) -> Self {
        let stem = module_stem(&self.name);
        self.members.push(simple_member(
            &stem,
            name,
            MemberKind::Constant,
            Sig::new(Vec::new(), ty),
            ptr,
        ));
        self
    }

    /// Uma variável de módulo. `Const` → só getter (READONLY); `Mutable` → getter
    /// + setter. `get` = `fn() -> ty`; `set` = `fn(ty)` (ignorado se `Const`).
    pub fn variable(
        mut self,
        name: &str,
        kind: VarKind,
        get: *const u8,
        set: Option<*const u8>,
        ty: AbiType,
    ) -> Self {
        let stem = module_stem(&self.name);
        let mut getter = Member {
            name: name.to_string(),
            kind: MemberKind::VarGetter,
            symbol: format!("{}_GET", fn_symbol(&stem, name)),
            sig: Sig::new(Vec::new(), ty),
            fn_ptr: FnPtr(get),
            flags: if kind == VarKind::Mutable {
                MemberFlags::MUTABLE
            } else {
                MemberFlags::READONLY
            },
            aliases: Vec::new(),
            variadic: false,
            ts_signature: String::new(),
            doc: String::new(),
            pure: false,
            intrinsic: None,
        };
        getter.ts_signature = format!("{name}: {}", ts_of(ty));
        self.members.push(getter);
        if kind == VarKind::Mutable {
            if let Some(set) = set {
                self.members.push(Member {
                    name: name.to_string(),
                    kind: MemberKind::VarSetter,
                    symbol: format!("{}_SET", fn_symbol(&stem, name)),
                    sig: Sig::new(vec![ty], AbiType::Void),
                    fn_ptr: FnPtr(set),
                    flags: MemberFlags::MUTABLE,
                    aliases: Vec::new(),
                    variadic: false,
                    ts_signature: String::new(),
                    doc: String::new(),
                    pure: false,
                    intrinsic: None,
                });
            }
        }
        self
    }

    /// Escape hatch: empurra um [`Member`] totalmente construído (símbolo/flags/
    /// aliases custom).
    pub fn member(mut self, member: Member) -> Self {
        self.members.push(member);
        self
    }

    /// Finaliza: insere o módulo (e os seus símbolos) no registry, mais uma
    /// entrada por alias na keyspace `rts:` que o resolver de import consulta.
    pub fn done(self) {
        let module = Module {
            scheme: self.scheme,
            name: self.name,
            doc: self.doc,
            members: self.members,
            private: self.private,
            load_order: self.load_order,
        };
        for alias in &self.aliases {
            self.engine
                .registry
                .insert_module_at(format!("rts:{alias}"), module.clone());
        }
        self.engine.registry.insert_module(module);
    }
}

/// Acumula os membros de uma classe global.
pub struct ClassBuilder<'e> {
    engine: &'e mut Engine,
    name: String,
    doc: String,
    members: Vec<Member>,
    instanceof_predicate: Option<String>,
}

impl ClassBuilder<'_> {
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

    pub fn done(self) {
        self.engine.registry.insert_class(Class {
            name: self.name,
            doc: self.doc,
            members: self.members,
            instanceof_predicate: self.instanceof_predicate,
        });
    }
}

/// Registra membros de escopo global (bare ident, sem import): `NaN`, `isNaN`,
/// variáveis globais. Cada chamada insere direto no registry.
pub struct GlobalBuilder<'e> {
    engine: &'e mut Engine,
}

impl GlobalBuilder<'_> {
    /// Função global: `isNaN(x)`.
    pub fn function(self, name: &str, ptr: *const u8, sig: Sig) -> Self {
        let m = simple_member("GLOBAL", name, MemberKind::Function, sig, ptr);
        self.engine.registry.insert_global(m);
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
        self.engine.registry.insert_global(m);
        self
    }
}

/// Tipo TS de um `AbiType` (para `ts_signature` derivado).
fn ts_of(ty: AbiType) -> &'static str {
    match ty {
        AbiType::Bool => "boolean",
        AbiType::StrPtr => "string",
        AbiType::Void => "void",
        _ => "number",
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
}
