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
            emit: None,
        }
    }
    // Membros LIMPOS (sem chave) compartilhados por Map E Set — o receiver é o
    // mesmo handle e os símbolos __RTS_FN_NS_COLLECTIONS_* tratam ambos.
    let members = || -> Vec<Member> {
        vec![
            m(
                "clear",
                Sig::new(vec![AbiType::Handle], AbiType::Void),
                "__RTS_FN_NS_COLLECTIONS_MAP_CLEAR",
            ),
            m(
                "size",
                Sig::new(vec![AbiType::Handle], AbiType::I64),
                "__RTS_FN_NS_COLLECTIONS_MAP_LEN",
            ),
            m(
                "keys",
                Sig::new(vec![AbiType::Handle], AbiType::Handle),
                "__RTS_FN_NS_COLLECTIONS_MAP_KEYS",
            ),
            m(
                "values",
                Sig::new(vec![AbiType::Handle], AbiType::Handle),
                "__RTS_FN_NS_COLLECTIONS_MAP_VALUES",
            ),
            m(
                "entries",
                Sig::new(vec![AbiType::Handle], AbiType::Handle),
                "__RTS_FN_NS_COLLECTIONS_MAP_ENTRIES_INSERTION",
            ),
            m(
                "union",
                Sig::new(vec![AbiType::Handle, AbiType::Handle], AbiType::Handle),
                "__RTS_FN_NS_COLLECTIONS_SET_UNION",
            ),
            m(
                "intersection",
                Sig::new(vec![AbiType::Handle, AbiType::Handle], AbiType::Handle),
                "__RTS_FN_NS_COLLECTIONS_SET_INTERSECTION",
            ),
            m(
                "difference",
                Sig::new(vec![AbiType::Handle, AbiType::Handle], AbiType::Handle),
                "__RTS_FN_NS_COLLECTIONS_SET_DIFFERENCE",
            ),
            m(
                "symmetricDifference",
                Sig::new(vec![AbiType::Handle, AbiType::Handle], AbiType::Handle),
                "__RTS_FN_NS_COLLECTIONS_SET_SYMMETRIC_DIFFERENCE",
            ),
            m(
                "isSubsetOf",
                Sig::new(vec![AbiType::Handle, AbiType::Handle], AbiType::Bool),
                "__RTS_FN_NS_COLLECTIONS_SET_IS_SUBSET",
            ),
            m(
                "isSupersetOf",
                Sig::new(vec![AbiType::Handle, AbiType::Handle], AbiType::Bool),
                "__RTS_FN_NS_COLLECTIONS_SET_IS_SUPERSET",
            ),
            m(
                "isDisjointFrom",
                Sig::new(vec![AbiType::Handle, AbiType::Handle], AbiType::Bool),
                "__RTS_FN_NS_COLLECTIONS_SET_IS_DISJOINT",
            ),
            // Métodos com CHAVE drenados (Grupo B-núcleo): key como handle único,
            // runtime AUTO deriva a key (set_stable_key/string content). set
            // (value frac-float bitcast) e forEach (callback) seguem no builtin.
            m(
                "add",
                Sig::new(vec![AbiType::Handle, AbiType::I64], AbiType::Handle),
                "__RTS_FN_NS_COLLECTIONS_SET_ADD",
            ),
            m(
                "has",
                Sig::new(vec![AbiType::Handle, AbiType::Handle], AbiType::Bool),
                "__RTS_FN_NS_COLLECTIONS_HAS_AUTO",
            ),
            m(
                "delete",
                Sig::new(vec![AbiType::Handle, AbiType::Handle], AbiType::Bool),
                "__RTS_FN_NS_COLLECTIONS_DELETE_AUTO",
            ),
            // forEach(cb): callback como I64 (fn-ptr/handle). A reificação COM
            // captura (lifted arrow) é feita pelo emissor genérico (ns_call.rs).
            m(
                "forEach",
                Sig::new(vec![AbiType::Handle, AbiType::I64], AbiType::Void),
                "__RTS_FN_NS_COLLECTIONS_MAP_FOR_EACH",
            ),
            {
                // get: retorno ambíguo (valor ou undefined-sentinel) → AMBIGUOUS_RET.
                let mut g = m(
                    "get",
                    Sig::new(vec![AbiType::Handle, AbiType::Handle], AbiType::I64),
                    "__RTS_FN_NS_COLLECTIONS_MAP_GET_AUTO_H",
                );
                g.flags = MemberFlags::AMBIGUOUS_RET;
                g
            },
        ]
    };
    // Map e Set são classes DISTINTAS no Registry (instanceof, dispatch), mas
    // partilham os símbolos runtime. Métodos com chave (set/get/has/delete/add/
    // forEach) seguem no builtin até terem runtime handle-based (Grupo B). O
    // `instanceof_predicate` resolve `x instanceof Map|Set` sem nomear no motor;
    // registrar "Set" também destrava o dispatch de método via Registry.
    for class_name in ["Map", "Set"] {
        let mut b = e
            .class(class_name)
            .doc("Map/Set — métodos sem chave (clear/size/keys/values/entries + set-ops).")
            .instanceof_predicate("__RTS_FN_NS_GC_IS_MAP_LIKE");
        for mem in members() {
            b = b.member(mem);
        }
        b.done();
    }
}

// NOTA (Fase 2): `register_array_class_spec` migrou p/ `rts-primitives/array.rs`
// (Array é primordial — só o motor pode citá-la diretamente). Os corpos
// `__RTS_FN_NS_COLLECTIONS_VEC_*` continuam aqui em `vec.rs`; só a spec
// (metadata pura, fn_ptr null) saiu. A facade re-exporta via
// `rts_primitives::array`.
