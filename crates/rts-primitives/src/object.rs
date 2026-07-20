//! Spec da classe primordial `Object` (metadata pura — fn_ptr null; corpos
//! `__RTS_FN_GL_OBJECT_*` ficam em `rts-shared/collections/map.rs`). Object é
//! primordial: o motor PODE nomeá-lo direto, mas registrar a spec dá ao Registry
//! uma entrada `Object` de primeira classe, permitindo que os gates
//! `global_class_lookup("Object").is_some()` substituam os literais `"Object"`
//! espalhados na codegen (uniformidade). Os métodos de instância de
//! `Object.prototype` (hasOwnProperty/propertyIsEnumerable/isPrototypeOf) são
//! declarados; os estáticos bespoke (keys/assign/freeze/create — com lógica
//! AUTO/variádica/descriptor própria) seguem no caminho hardcoded da codegen
//! por ora (drain = follow-up dedicado). SEM construtor: `Object()`/`new Object()`
//! seguem o coerção-path hardcoded (o guard de ctor filtra specs sem ctor).

/// Registra a classe global `Object` no motor (Fase 2.7 — só metadata).
pub fn register_object_class_spec(e: &mut rts_engine::Engine) {
    use rts_engine::{AbiType, FnPtr, Member, MemberFlags, MemberKind, Sig};
    fn m(name: &str, sig: Sig, symbol: &str) -> Member {
        Member {
            name: name.to_string(),
            kind: MemberKind::InstanceMethod,
            sig,
            symbol: symbol.to_string(),
            fn_ptr: FnPtr(core::ptr::null::<u8>()),
            flags: MemberFlags::NONE,
            aliases: Vec::new(),
            variadic: false,
            ts_signature: String::new(),
            doc: String::new(),
            pure: false,
            intrinsic: None,
            emit: None,
        }
    }
    e.class("Object")
        .doc("Object — primordial. Spec só de métodos de instância Object.prototype; estáticos (keys/assign/freeze/...) seguem hardcoded na codegen.")
        .member(m(
            "hasOwnProperty",
            Sig::new(vec![AbiType::Handle, AbiType::StrPtr], AbiType::Bool),
            "__RTS_FN_GL_OBJECT_HAS_OWN_PROPERTY",
        ))
        .member(m(
            "propertyIsEnumerable",
            Sig::new(vec![AbiType::Handle, AbiType::StrPtr], AbiType::Bool),
            "__RTS_FN_GL_OBJECT_PROPERTY_IS_ENUMERABLE",
        ))
        .member(m(
            "isPrototypeOf",
            Sig::new(vec![AbiType::Handle, AbiType::Handle], AbiType::Bool),
            "__RTS_FN_GL_OBJECT_IS_PROTOTYPE_OF",
        ))
        .done();
}
