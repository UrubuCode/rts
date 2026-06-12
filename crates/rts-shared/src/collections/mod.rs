//! `collections` namespace — HashMap<string, i64> e Vec<i64> via
//! HandleTable do gc.
//!
//! Escopo intencionalmente minimo: valores sao sempre i64. Caller
//! interpreta como inteiro, handle (string, bigfloat, etc) ou bool
//! conforme o uso. Quando object literals (#53) chegarem, um
//! `MapAny` com valores polimorficos sera natural.

pub mod map;
pub mod vec;

// (Fase 2) Os 26 membros do namespace sao hand-written em dois módulos —
// map.rs (17) e vec.rs (9). Cada um expõe `append_engine_members(&mut Vec<
// Member>)`; o `register()` abaixo agrega map primeiro, depois vec (preserva a
// ordem do antigo `concat_members` p/ rts.d.ts byte-identico). Sem `SPEC`/
// `MEMBERS` const — o registry do `rts-engine` é a fonte de verdade.

/// Registra o namespace `collections` no motor (Fase 2). Owner hand-written:
/// agrega os membros (formato builder) dos dois `part` (map + vec) na ordem
/// map→vec (preserva a ordem do antigo `concat_members`).
pub fn register(e: &mut rts_engine::Engine) {
    let mut members: Vec<rts_engine::Member> = Vec::new();
    map::append_engine_members(&mut members);
    vec::append_engine_members(&mut members);
    let mut b = e
        .ns("collections")
        .doc("Handle-based HashMap and Vec backed by std::collections.");
    for m in members {
        b = b.member(m);
    }
    b.done();
}

/// Registra a classe global `Map` (cobre Map E Set — o receiver é o mesmo handle
/// e os símbolos `__RTS_FN_NS_COLLECTIONS_*` tratam ambos). Só os métodos LIMPOS
/// (sem decomposição de chave): o codegen resolve `recv.method(args)` pelo
/// Registry e emite a call genérica, sem braço hardcoded. Os métodos com chave
/// (set/get/has/delete/add/forEach) seguem no builtin até terem runtime
/// handle-based. fn_ptr null: os símbolos resolvem via a registração do
/// namespace `collections` (jit add_fn!).
pub fn register_mapset_class_spec(e: &mut rts_engine::Engine) {
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
        }
    }
    e.class("Map")
        .doc("Map/Set — métodos sem chave (clear/size/keys/values/entries + set-ops).")
        .member(m("clear", Sig::new(vec![AbiType::Handle], AbiType::Void), "__RTS_FN_NS_COLLECTIONS_MAP_CLEAR"))
        .member(m("size", Sig::new(vec![AbiType::Handle], AbiType::I64), "__RTS_FN_NS_COLLECTIONS_MAP_LEN"))
        .member(m("keys", Sig::new(vec![AbiType::Handle], AbiType::Handle), "__RTS_FN_NS_COLLECTIONS_MAP_KEYS"))
        .member(m("values", Sig::new(vec![AbiType::Handle], AbiType::Handle), "__RTS_FN_NS_COLLECTIONS_MAP_VALUES"))
        .member(m("entries", Sig::new(vec![AbiType::Handle], AbiType::Handle), "__RTS_FN_NS_COLLECTIONS_MAP_ENTRIES_INSERTION"))
        .member(m("union", Sig::new(vec![AbiType::Handle, AbiType::Handle], AbiType::Handle), "__RTS_FN_NS_COLLECTIONS_SET_UNION"))
        .member(m("intersection", Sig::new(vec![AbiType::Handle, AbiType::Handle], AbiType::Handle), "__RTS_FN_NS_COLLECTIONS_SET_INTERSECTION"))
        .member(m("difference", Sig::new(vec![AbiType::Handle, AbiType::Handle], AbiType::Handle), "__RTS_FN_NS_COLLECTIONS_SET_DIFFERENCE"))
        .member(m("symmetricDifference", Sig::new(vec![AbiType::Handle, AbiType::Handle], AbiType::Handle), "__RTS_FN_NS_COLLECTIONS_SET_SYMMETRIC_DIFFERENCE"))
        .member(m("isSubsetOf", Sig::new(vec![AbiType::Handle, AbiType::Handle], AbiType::Bool), "__RTS_FN_NS_COLLECTIONS_SET_IS_SUBSET"))
        .member(m("isSupersetOf", Sig::new(vec![AbiType::Handle, AbiType::Handle], AbiType::Bool), "__RTS_FN_NS_COLLECTIONS_SET_IS_SUPERSET"))
        .member(m("isDisjointFrom", Sig::new(vec![AbiType::Handle, AbiType::Handle], AbiType::Bool), "__RTS_FN_NS_COLLECTIONS_SET_IS_DISJOINT"))
        .done();
}

/// Registra a classe global `Array` (receiver = handle de Vec). Só os métodos
/// LIMPOS recv-only (sem callback/variádico/arg-overload): pop/shift (retorno
/// ambíguo — elemento ou undefined), reverse/toReversed/values/keys/entries.
/// O codegen resolve `arr.method()` pelo Registry, sem braço hardcoded; métodos
/// ausentes (push/sort/map/...) caem de volta no array builtin. `.length`/`.size`
/// vão pelo handle-length genérico (ver rc_for_size em members.rs). fn_ptr null:
/// símbolos resolvem via a registração do namespace `collections`.
pub fn register_array_class_spec(e: &mut rts_engine::Engine) {
    use rts_engine::{AbiType, FnPtr, Member, MemberFlags, MemberKind, Sig};
    fn m(name: &str, sig: Sig, symbol: &str, flags: MemberFlags) -> Member {
        Member {
            name: name.to_string(),
            kind: MemberKind::InstanceMethod,
            sig,
            symbol: symbol.to_string(),
            fn_ptr: FnPtr(core::ptr::null::<u8>()),
            flags,
            aliases: Vec::new(),
            variadic: false,
            ts_signature: String::new(),
            doc: String::new(),
            pure: false,
            intrinsic: None,
        }
    }
    let h = || Sig::new(vec![AbiType::Handle], AbiType::Handle);
    let amb = || Sig::new(vec![AbiType::Handle], AbiType::I64);
    e.class("Array")
        .doc("Array — métodos recv-only sem callback (pop/shift/reverse/toReversed/values/keys/entries).")
        .member(m("pop", amb(), "__RTS_FN_NS_COLLECTIONS_VEC_POP", MemberFlags::AMBIGUOUS_RET))
        .member(m("shift", amb(), "__RTS_FN_NS_COLLECTIONS_VEC_SHIFT", MemberFlags::AMBIGUOUS_RET))
        .member(m("reverse", h(), "__RTS_FN_NS_COLLECTIONS_VEC_REVERSE", MemberFlags::NONE))
        .member(m("toReversed", h(), "__RTS_FN_NS_COLLECTIONS_VEC_TO_REVERSED", MemberFlags::NONE))
        .member(m("values", h(), "__RTS_FN_NS_COLLECTIONS_VEC_VALUES", MemberFlags::NONE))
        .member(m("keys", h(), "__RTS_FN_NS_COLLECTIONS_VEC_KEYS", MemberFlags::NONE))
        .member(m("entries", h(), "__RTS_FN_NS_COLLECTIONS_VEC_ENTRIES", MemberFlags::NONE))
        .done();
}
