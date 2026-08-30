//! Travessia e mutação da árvore: a ponte que a fachada `.ts` já chamava e que
//! não existia.
//!
//! # O que isto repõe, e porque ninguém deu por ela faltar
//!
//! `crates/rts-dom/src/dom/travessia.rs` e `mutacao.rs` implementam `firstChild`,
//! `insertBefore`, `removeChild`, `cloneNode` e o resto há muito. O que foi
//! apagado com o motor antigo — no mesmo commit que levou o `scriptscan` e o
//! `timerscope`, e pela mesma razão: nomeavam `rts_engine` numa linha de `use` —
//! foi a PONTE, o `abi.rs`. A lógica ficou, a porta não.
//!
//! E a falta não apareceu, porque o outro lado da fronteira é TEXTO: o `dom.ts`
//! continuou a declarar `get firstChild()` a chamar `dom.firstChild(...)`, e
//! nenhum teste em Rust passa por aqui. A suite do `rts-dom` responde `718
//! passed` com metade da API do DOM sem porta nenhuma.
//!
//! É a mesma lição que `docs/ui/page-script-bridge.md` já tinha escrito para as
//! outras três peças, e esta é a quarta.
//!
//! # Como se viu
//!
//! Ao carregar o React 18.3.1 numa página: os dois bundles correm, `createRoot`
//! e `render` são chamados, o scheduler entrega o trabalho — e o React morre a
//! montar, porque montar é `insertBefore` e `firstChild`. O erro que aparecia
//! primeiro era `dom.firstChild is not a function`, que nomeia exatamente o
//! problema e leva ao sítio errado se lido como "falta implementar".
//!
//! # A convenção
//!
//! A mesma de `nodes.rs`, e por isso este módulo não a redefine: um nó é um
//! `i64` (`-1` para nenhum), o documento é um handle, e um `Option<NodeId>`
//! atravessa como `-1`. Ver `crate::value` para as conversões.

use rts_core::entry::Provided;

use crate::nodes::node;
use crate::value::{handle, int, integer, nothing, string, text};

/// Traduz uma resposta do DOM que pode ser nenhum nó.
fn maybe(found: Option<rts_dom::NodeId>) -> u64 {
    int(found.map(|id| id.to_abi()).unwrap_or(-1))
}

pub const MEMBERS: &[(&str, Provided)] = &[
    // ── travessia ──────────────────────────────────────────────────────────
    ("firstChild", first_child),
    ("lastChild", last_child),
    ("nextSibling", next_sibling),
    ("previousSibling", previous_sibling),
    ("parentNode", parent_node),
    ("firstElementChild", first_element_child),
    ("lastElementChild", last_element_child),
    ("nextElementSibling", next_element_sibling),
    ("previousElementSibling", previous_element_sibling),
    ("childNodesCount", child_nodes_count),
    ("childNodeAt", child_node_at),
    ("hasChildNodes", has_child_nodes),
    ("contains", contains),
    ("closest", closest),
    ("nodeName", node_name),
    ("nodeValue", node_value),
    // ── mutação ────────────────────────────────────────────────────────────
    ("insertBefore", insert_before),
    ("removeChild", remove_child),
    ("replaceChild", replace_child),
    ("prepend", prepend),
    ("replaceWith", replace_with),
    ("insertAdjacent", insert_adjacent),
    ("clearChildren", clear_children),
    ("cloneNode", clone_node),
    ("createComment", create_comment),
    ("normalize", normalize),
    ("setNodeValue", set_node_value),
    ("attrCount", attr_count),
    ("attrNameAt", attr_name_at),
    // ── estilo inline ──────────────────────────────────────────────────────
    ("inlineProperty", inline_property),
    ("setStyleProperty", set_style_property),
    ("removeStyleProperty", remove_style_property),
    ("cssText", css_text),
    ("setCssText", set_css_text),
    // ── consulta a partir de um NÓ, e não do documento ─────────────────────
    ("queryWithin", query_within),
    ("queryAllWithinCount", query_all_within_count),
    ("queryAllWithinAt", query_all_within_at),
    ("getByClassCount", get_by_class_count),
    ("getByClassAt", get_by_class_at),
    ("getByNameCount", get_by_name_count),
    ("getByNameAt", get_by_name_at),
    ("advance", advance),
];

/// `dom.advance(doc, agora_ms)` — faz o tempo passar para as animações CSS e as
/// transições. Responde `1` se alguma continua a correr.
///
/// O frame da janela já chamava isto, de Rust para Rust; o que não havia era
/// forma de um programa o chamar — e sem ela não se consegue perguntar *"esta
/// página anima?"* sem abrir uma janela e olhar. Um teste que precisa de um
/// humano a olhar não é um teste.
extern "C" fn advance(_e: u64, _t: u64, doc: u64, agora: u64, _b: u64, _c: u64) -> u64 {
    let agora = integer(agora, 0) as f32;
    let a_correr = rts_dom::store::with_dom_mut(handle(doc), |d| d.advance(agora)).unwrap_or(false);
    int(if a_correr { 1 } else { 0 })
}

// ── consulta ───────────────────────────────────────────────────────────────
//
// `document.querySelector` já estava exposto; o que faltava era a mesma
// pergunta feita a partir de um NÓ — `el.querySelector(...)`, que é o que uma
// aplicação usa para procurar dentro de um componente. `rts-dom` responde às
// duas há muito (`consulta.rs`), e só a segunda não tinha porta.
//
// Contagem e acesso separados pela razão de sempre: um `Vec<NodeId>` não
// atravessa esta fronteira.

extern "C" fn query_within(_e: u64, _t: u64, doc: u64, n: u64, sel: u64, _c: u64) -> u64 {
    let Some(id) = node(n) else { return int(-1) };
    let sel = text(sel);
    maybe(rts_dom::store::with_dom(handle(doc), |d| d.query_within(id, &sel)).flatten())
}

extern "C" fn query_all_within_count(_e: u64, _t: u64, doc: u64, n: u64, sel: u64, _c: u64) -> u64 {
    let Some(id) = node(n) else { return int(0) };
    let sel = text(sel);
    let out = rts_dom::store::with_dom(handle(doc), |d| d.query_all_within(id, &sel).len())
        .unwrap_or(0);
    int(out as i64)
}

extern "C" fn query_all_within_at(_e: u64, _t: u64, doc: u64, n: u64, sel: u64, i: u64) -> u64 {
    let Some(id) = node(n) else { return int(-1) };
    let sel = text(sel);
    let i = integer(i, 0).max(0) as usize;
    maybe(
        rts_dom::store::with_dom(handle(doc), |d| d.query_all_within(id, &sel).get(i).copied())
            .flatten(),
    )
}

extern "C" fn get_by_class_count(_e: u64, _t: u64, doc: u64, nomes: u64, _b: u64, _c: u64) -> u64 {
    let nomes = text(nomes);
    let out = rts_dom::store::with_dom(handle(doc), |d| d.get_elements_by_class_name(&nomes).len())
        .unwrap_or(0);
    int(out as i64)
}

extern "C" fn get_by_class_at(_e: u64, _t: u64, doc: u64, nomes: u64, i: u64, _c: u64) -> u64 {
    let nomes = text(nomes);
    let i = integer(i, 0).max(0) as usize;
    maybe(
        rts_dom::store::with_dom(handle(doc), |d| {
            d.get_elements_by_class_name(&nomes).get(i).copied()
        })
        .flatten(),
    )
}

extern "C" fn get_by_name_count(_e: u64, _t: u64, doc: u64, nome: u64, _b: u64, _c: u64) -> u64 {
    let nome = text(nome);
    let out = rts_dom::store::with_dom(handle(doc), |d| d.get_elements_by_name(&nome).len())
        .unwrap_or(0);
    int(out as i64)
}

extern "C" fn get_by_name_at(_e: u64, _t: u64, doc: u64, nome: u64, i: u64, _c: u64) -> u64 {
    let nome = text(nome);
    let i = integer(i, 0).max(0) as usize;
    maybe(
        rts_dom::store::with_dom(handle(doc), |d| d.get_elements_by_name(&nome).get(i).copied())
            .flatten(),
    )
}

// ── estilo inline ──────────────────────────────────────────────────────────
//
// `rts-dom/src/dom/estilo.rs` já respondia a tudo isto; era a ponte que não
// existia, como no resto deste ficheiro. É o que faz `node.style.color = "red"`
// — a forma como o React escreve estilo — poder chegar ao documento.

extern "C" fn inline_property(_e: u64, _t: u64, doc: u64, n: u64, name: u64, _c: u64) -> u64 {
    let Some(id) = node(n) else { return string("") };
    let name = text(name);
    let out = rts_dom::store::with_dom(handle(doc), |d| d.inline_property(id, &name))
        .unwrap_or_default();
    string(&out)
}

extern "C" fn set_style_property(_e: u64, _t: u64, doc: u64, n: u64, name: u64, value: u64) -> u64 {
    let Some(id) = node(n) else { return nothing() };
    let (name, value) = (text(name), text(value));
    rts_dom::store::with_dom_mut(handle(doc), |d| d.set_style_property(id, &name, &value));
    nothing()
}

extern "C" fn remove_style_property(_e: u64, _t: u64, doc: u64, n: u64, name: u64, _c: u64) -> u64 {
    let Some(id) = node(n) else { return nothing() };
    let name = text(name);
    rts_dom::store::with_dom_mut(handle(doc), |d| d.remove_style_property(id, &name));
    nothing()
}

extern "C" fn css_text(_e: u64, _t: u64, doc: u64, n: u64, _b: u64, _c: u64) -> u64 {
    let Some(id) = node(n) else { return string("") };
    let out = rts_dom::store::with_dom(handle(doc), |d| d.css_text(id)).unwrap_or_default();
    string(&out)
}

extern "C" fn set_css_text(_e: u64, _t: u64, doc: u64, n: u64, v: u64, _c: u64) -> u64 {
    let Some(id) = node(n) else { return nothing() };
    let v = text(v);
    rts_dom::store::with_dom_mut(handle(doc), |d| d.set_css_text(id, &v));
    nothing()
}

// ── travessia ──────────────────────────────────────────────────────────────

extern "C" fn first_child(_e: u64, _t: u64, doc: u64, n: u64, _b: u64, _c: u64) -> u64 {
    let Some(id) = node(n) else { return int(-1) };
    maybe(rts_dom::store::with_dom(handle(doc), |d| d.first_child(id)).flatten())
}

extern "C" fn last_child(_e: u64, _t: u64, doc: u64, n: u64, _b: u64, _c: u64) -> u64 {
    let Some(id) = node(n) else { return int(-1) };
    maybe(rts_dom::store::with_dom(handle(doc), |d| d.last_child(id)).flatten())
}

extern "C" fn next_sibling(_e: u64, _t: u64, doc: u64, n: u64, _b: u64, _c: u64) -> u64 {
    let Some(id) = node(n) else { return int(-1) };
    maybe(rts_dom::store::with_dom(handle(doc), |d| d.next_sibling(id)).flatten())
}

extern "C" fn previous_sibling(_e: u64, _t: u64, doc: u64, n: u64, _b: u64, _c: u64) -> u64 {
    let Some(id) = node(n) else { return int(-1) };
    maybe(rts_dom::store::with_dom(handle(doc), |d| d.previous_sibling(id)).flatten())
}

/// `parentNode` e não `parentElement`: o pai de um nó pode ser o documento ou um
/// fragmento, e `parentElement` responde nenhum nesse caso. São perguntas
/// diferentes e o `nodes.rs` já responde à outra.
extern "C" fn parent_node(_e: u64, _t: u64, doc: u64, n: u64, _b: u64, _c: u64) -> u64 {
    let Some(id) = node(n) else { return int(-1) };
    maybe(rts_dom::store::with_dom(handle(doc), |d| d.parent_of(id)).flatten())
}

extern "C" fn first_element_child(_e: u64, _t: u64, doc: u64, n: u64, _b: u64, _c: u64) -> u64 {
    let Some(id) = node(n) else { return int(-1) };
    maybe(rts_dom::store::with_dom(handle(doc), |d| d.first_element_child(id)).flatten())
}

extern "C" fn last_element_child(_e: u64, _t: u64, doc: u64, n: u64, _b: u64, _c: u64) -> u64 {
    let Some(id) = node(n) else { return int(-1) };
    maybe(rts_dom::store::with_dom(handle(doc), |d| d.last_element_child(id)).flatten())
}

extern "C" fn next_element_sibling(_e: u64, _t: u64, doc: u64, n: u64, _b: u64, _c: u64) -> u64 {
    let Some(id) = node(n) else { return int(-1) };
    maybe(rts_dom::store::with_dom(handle(doc), |d| d.next_element_sibling(id)).flatten())
}

extern "C" fn previous_element_sibling(_e: u64, _t: u64, doc: u64, n: u64, _b: u64, _c: u64) -> u64 {
    let Some(id) = node(n) else { return int(-1) };
    maybe(rts_dom::store::with_dom(handle(doc), |d| d.previous_element_sibling(id)).flatten())
}

/// A contagem e o acesso são separados porque um `Vec<NodeId>` não atravessa
/// esta fronteira — a mesma forma que `childCount`/`childAt` já usam para os
/// elementos, aqui para TODOS os nós (texto e comentários incluídos, que é o que
/// distingue `childNodes` de `children`).
extern "C" fn child_nodes_count(_e: u64, _t: u64, doc: u64, n: u64, _b: u64, _c: u64) -> u64 {
    let Some(id) = node(n) else { return int(0) };
    let out = rts_dom::store::with_dom(handle(doc), |d| d.child_nodes(id).len()).unwrap_or(0);
    int(out as i64)
}

extern "C" fn child_node_at(_e: u64, _t: u64, doc: u64, n: u64, i: u64, _c: u64) -> u64 {
    let Some(id) = node(n) else { return int(-1) };
    let i = integer(i, 0).max(0) as usize;
    maybe(rts_dom::store::with_dom(handle(doc), |d| d.child_nodes(id).get(i).copied()).flatten())
}

extern "C" fn has_child_nodes(_e: u64, _t: u64, doc: u64, n: u64, _b: u64, _c: u64) -> u64 {
    let Some(id) = node(n) else { return int(0) };
    let out = rts_dom::store::with_dom(handle(doc), |d| d.has_child_nodes(id)).unwrap_or(false);
    int(if out { 1 } else { 0 })
}

extern "C" fn contains(_e: u64, _t: u64, doc: u64, n: u64, other: u64, _c: u64) -> u64 {
    let (Some(id), Some(other)) = (node(n), node(other)) else { return int(0) };
    let out = rts_dom::store::with_dom(handle(doc), |d| d.contains(id, other)).unwrap_or(false);
    int(if out { 1 } else { 0 })
}

extern "C" fn closest(_e: u64, _t: u64, doc: u64, n: u64, sel: u64, _c: u64) -> u64 {
    let Some(id) = node(n) else { return int(-1) };
    let sel = text(sel);
    maybe(rts_dom::store::with_dom(handle(doc), |d| d.closest(id, &sel)).flatten())
}

extern "C" fn node_name(_e: u64, _t: u64, doc: u64, n: u64, _b: u64, _c: u64) -> u64 {
    let Some(id) = node(n) else { return string("") };
    let out = rts_dom::store::with_dom(handle(doc), |d| d.node_name(id).unwrap_or_default())
        .unwrap_or_default();
    string(&out)
}

extern "C" fn node_value(_e: u64, _t: u64, doc: u64, n: u64, _b: u64, _c: u64) -> u64 {
    let Some(id) = node(n) else { return string("") };
    let out = rts_dom::store::with_dom(handle(doc), |d| d.node_value(id).unwrap_or_default())
        .unwrap_or_default();
    string(&out)
}

// ── mutação ────────────────────────────────────────────────────────────────

/// `insertBefore(doc, pai, filho, referencia)` — `-1` na referência significa
/// "no fim", que é o que a especificação diz de `insertBefore(x, null)`.
extern "C" fn insert_before(_e: u64, _t: u64, doc: u64, parent: u64, child: u64, reference: u64) -> u64 {
    let (Some(p), Some(c)) = (node(parent), node(child)) else { return nothing() };
    let reference = node(reference);
    rts_dom::store::with_dom_mut(handle(doc), |d| d.insert_before(p, c, reference));
    nothing()
}

extern "C" fn remove_child(_e: u64, _t: u64, doc: u64, parent: u64, child: u64, _c: u64) -> u64 {
    let (Some(p), Some(c)) = (node(parent), node(child)) else { return nothing() };
    rts_dom::store::with_dom_mut(handle(doc), |d| d.remove_child(p, c));
    nothing()
}

extern "C" fn replace_child(_e: u64, _t: u64, doc: u64, parent: u64, new_child: u64, old_child: u64) -> u64 {
    let (Some(p), Some(novo), Some(velho)) = (node(parent), node(new_child), node(old_child)) else {
        return nothing();
    };
    rts_dom::store::with_dom_mut(handle(doc), |d| d.replace_child(p, novo, velho));
    nothing()
}

extern "C" fn prepend(_e: u64, _t: u64, doc: u64, parent: u64, child: u64, _c: u64) -> u64 {
    let (Some(p), Some(c)) = (node(parent), node(child)) else { return nothing() };
    rts_dom::store::with_dom_mut(handle(doc), |d| d.prepend_child(p, c));
    nothing()
}

extern "C" fn replace_with(_e: u64, _t: u64, doc: u64, n: u64, other: u64, _c: u64) -> u64 {
    let (Some(id), Some(other)) = (node(n), node(other)) else { return nothing() };
    rts_dom::store::with_dom_mut(handle(doc), |d| d.replace_with(id, other));
    nothing()
}

/// `after != 0` insere depois, `0` insere antes.
extern "C" fn insert_adjacent(_e: u64, _t: u64, doc: u64, n: u64, other: u64, after: u64) -> u64 {
    let (Some(id), Some(other)) = (node(n), node(other)) else { return nothing() };
    let after = integer(after, 0) != 0;
    rts_dom::store::with_dom_mut(handle(doc), |d| d.insert_adjacent(id, other, after));
    nothing()
}

extern "C" fn clear_children(_e: u64, _t: u64, doc: u64, n: u64, _b: u64, _c: u64) -> u64 {
    let Some(id) = node(n) else { return nothing() };
    rts_dom::store::with_dom_mut(handle(doc), |d| d.clear_children(id));
    nothing()
}

extern "C" fn clone_node(_e: u64, _t: u64, doc: u64, n: u64, deep: u64, _c: u64) -> u64 {
    let Some(id) = node(n) else { return int(-1) };
    let deep = integer(deep, 0) != 0;
    maybe(rts_dom::store::with_dom_mut(handle(doc), |d| d.clone_node(id, deep)).flatten())
}

extern "C" fn create_comment(_e: u64, _t: u64, doc: u64, s: u64, _b: u64, _c: u64) -> u64 {
    let s = text(s);
    let out = rts_dom::store::with_dom_mut(handle(doc), |d| d.create_comment(&s).to_abi())
        .unwrap_or(-1);
    int(out)
}

extern "C" fn normalize(_e: u64, _t: u64, doc: u64, n: u64, _b: u64, _c: u64) -> u64 {
    let Some(id) = node(n) else { return nothing() };
    rts_dom::store::with_dom_mut(handle(doc), |d| d.normalize(id));
    nothing()
}

extern "C" fn set_node_value(_e: u64, _t: u64, doc: u64, n: u64, v: u64, _c: u64) -> u64 {
    let Some(id) = node(n) else { return nothing() };
    let v = text(v);
    rts_dom::store::with_dom_mut(handle(doc), |d| d.set_node_value(id, &v));
    nothing()
}

extern "C" fn attr_count(_e: u64, _t: u64, doc: u64, n: u64, _b: u64, _c: u64) -> u64 {
    let Some(id) = node(n) else { return int(0) };
    let out = rts_dom::store::with_dom(handle(doc), |d| d.attr_names(id).len()).unwrap_or(0);
    int(out as i64)
}

extern "C" fn attr_name_at(_e: u64, _t: u64, doc: u64, n: u64, i: u64, _c: u64) -> u64 {
    let Some(id) = node(n) else { return string("") };
    let i = integer(i, 0).max(0) as usize;
    let out = rts_dom::store::with_dom(handle(doc), |d| {
        d.attr_names(id).get(i).cloned().unwrap_or_default()
    })
    .unwrap_or_default();
    string(&out)
}
