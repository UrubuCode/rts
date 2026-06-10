//! `collections` namespace — HashMap<string, i64> e Vec<i64> via
//! HandleTable do gc.
//!
//! Escopo intencionalmente minimo: valores sao sempre i64. Caller
//! interpreta como inteiro, handle (string, bigfloat, etc) ou bool
//! conforme o uso. Quando object literals (#53) chegarem, um
//! `MapAny` com valores polimorficos sera natural.

pub mod map;
pub mod vec;

// (stage 2c) Os 26 membros do namespace vivem em dois `#[rts_namespace(
// collections, part)]` impls — map.rs (17) e vec.rs (9). Aqui agregamos as
// duas tabelas `MEMBERS` numa unica `SPEC` via o helper const
// `rts_abi::concat_members`. Ordem: map primeiro, depois vec (preserva a
// ordem do antigo abi.rs p/ rts.d.ts byte-identico).
const COLLECTIONS_MEMBERS: [rts_abi::NamespaceMember; 26] =
    rts_abi::concat_members(map::MEMBERS, vec::MEMBERS);

/// Membros agregados do namespace `collections` (map + vec).
pub const MEMBERS: &[rts_abi::NamespaceMember] = &COLLECTIONS_MEMBERS;

/// Spec do namespace `collections`.
pub const SPEC: rts_abi::NamespaceSpec = rts_abi::NamespaceSpec {
    name: "collections",
    doc: "Handle-based HashMap and Vec backed by std::collections.",
    members: MEMBERS,
};

/// Registra o namespace `collections` no motor (Fase 2). Owner hand-written:
/// agrega os membros (formato builder) dos dois `part` (map + vec) — mesma
/// ordem do const `MEMBERS` (`concat_members`: map, depois vec).
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
