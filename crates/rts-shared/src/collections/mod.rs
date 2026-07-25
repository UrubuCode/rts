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

// DRAIN_MOTOR (2026-07-24): `register_mapset_class_spec` removido — nunca
// chamado (não wired em `registry_build.rs`). Map/Set real vive no `.ts`
// stdlib (ambient classes, ver CLAUDE.md doctrine); `instanceof Map|Set`/
// dispatch de método não passa por essa spec.

// NOTA (Fase 2): `register_array_class_spec` migrou p/ `rts-primitives/array.rs`
// (Array é primordial — só o motor pode citá-la diretamente). Os corpos
// `__RTS_FN_NS_COLLECTIONS_VEC_*` continuam aqui em `vec.rs`; só a spec
// (metadata pura, fn_ptr null) saiu. A facade re-exporta via
// `rts_primitives::array`.
