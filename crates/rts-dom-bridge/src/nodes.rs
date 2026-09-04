//! Os NÓS: consultar, ler, mutar e medir.
//!
//! Todo membro daqui recebe `(doc, node, …)` — o handle do documento e o
//! `NodeId` versionado. Um id de outra árvore não resolve (a geração não casa) e
//! a chamada vira um no-op ou `-1`, que é a guarda que impede aplicar estado ao
//! nó errado depois de um `parseHtml` novo.
//!
//! ## Listas cruzam como `count` + `at(i)`
//!
//! `querySelectorAll` responde uma contagem, e cada elemento sai por índice. É
//! duas travessias em vez de uma, e é o que evita este crate ter de construir um
//! array do runtime — a fachada em TS já monta o array do lado dela, onde o
//! array é natural.

use rts_core::entry::Provided;
use rts_dom::NodeId;

use crate::value::{handle, int, integer, nothing, num, string, text};

pub const MEMBERS: &[(&str, Provided)] = &[
    // consulta
    ("querySelector", query_selector),
    ("getById", get_by_id),
    ("querySelectorAllCount", query_all_count),
    ("querySelectorAllAt", query_all_at),
    ("matches", matches_selector),
    // leitura
    ("getText", get_text),
    ("getAttribute", get_attribute),
    ("hasAttr", has_attr),
    ("tagName", tag_name),
    ("nodeType", node_type),
    ("childCount", child_count),
    ("childAt", child_at),
    ("parentElement", parent_element),
    ("innerHtml", inner_html),
    ("outerHtml", outer_html),
    // mutação
    ("setText", set_text),
    ("setAttribute", set_attribute),
    ("setAttr", set_attribute),
    ("removeAttribute", remove_attribute),
    ("removeAttr", remove_attribute),
    ("createElement", create_element),
    ("createTextNode", create_text_node),
    ("appendChild", append_child),
    ("removeNode", remove_node),
    ("setInnerHtml", set_inner_html),
    // estilo e geometria
    ("computedProperty", computed_property),
    ("setStyle", set_style),
    ("boundingRect", bounding_rect),
    ("boundingRectAll", bounding_rect_all),
    // pixels (o `<canvas>` e o `<img>` compartilham o mesmo caminho)
    ("setPixels", set_pixels),
];

/// O `NodeId` de um argumento, ou `None` para a sentinela `-1`.
pub(crate) fn node(value: u64) -> Option<NodeId> {
    NodeId::from_abi(integer(value, -1))
}

/// `querySelector(doc, sel)` — o primeiro que casa, em ordem de documento.
extern "C" fn query_selector(_e: u64, _t: u64, doc: u64, sel: u64, _b: u64, _c: u64) -> u64 {
    let sel = text(sel);
    let id = rts_dom::store::with_dom(handle(doc), |d| {
        d.query(&sel).map(|n| n.to_abi()).unwrap_or(-1)
    })
    .unwrap_or(-1);
    int(id)
}

extern "C" fn get_by_id(_e: u64, _t: u64, doc: u64, id: u64, _b: u64, _c: u64) -> u64 {
    let id = text(id);
    let out = rts_dom::store::with_dom(handle(doc), |d| {
        d.get_element_by_id(&id).map(|n| n.to_abi()).unwrap_or(-1)
    })
    .unwrap_or(-1);
    int(out)
}

extern "C" fn query_all_count(_e: u64, _t: u64, doc: u64, sel: u64, _b: u64, _c: u64) -> u64 {
    let sel = text(sel);
    let n = rts_dom::store::with_dom(handle(doc), |d| d.query_all(&sel).len()).unwrap_or(0);
    int(n as i64)
}

extern "C" fn query_all_at(_e: u64, _t: u64, doc: u64, sel: u64, i: u64, _c: u64) -> u64 {
    let sel = text(sel);
    let i = integer(i, 0).max(0) as usize;
    let id = rts_dom::store::with_dom(handle(doc), |d| {
        d.query_all(&sel).get(i).map(|n| n.to_abi()).unwrap_or(-1)
    })
    .unwrap_or(-1);
    int(id)
}

extern "C" fn matches_selector(_e: u64, _t: u64, doc: u64, n: u64, sel: u64, _c: u64) -> u64 {
    let sel = text(sel);
    let Some(id) = node(n) else { return int(0) };
    let yes = rts_dom::store::with_dom(handle(doc), |d| d.matches_selector(id, &sel)).unwrap_or(false);
    int(yes as i64)
}

extern "C" fn get_text(_e: u64, _t: u64, doc: u64, n: u64, _b: u64, _c: u64) -> u64 {
    let Some(id) = node(n) else { return string("") };
    let out = rts_dom::store::with_dom(handle(doc), |d| d.text_content(id).unwrap_or_default())
        .unwrap_or_default();
    string(&out)
}

extern "C" fn get_attribute(_e: u64, _t: u64, doc: u64, n: u64, name: u64, _c: u64) -> u64 {
    let name = text(name);
    let Some(id) = node(n) else { return string("") };
    let out = rts_dom::store::with_dom(handle(doc), |d| {
        d.get_attr(id, &name).unwrap_or_default().to_string()
    })
    .unwrap_or_default();
    string(&out)
}

extern "C" fn has_attr(_e: u64, _t: u64, doc: u64, n: u64, name: u64, _c: u64) -> u64 {
    let name = text(name);
    let Some(id) = node(n) else { return int(0) };
    let yes = rts_dom::store::with_dom(handle(doc), |d| d.has_attr(id, &name)).unwrap_or(false);
    int(yes as i64)
}

extern "C" fn tag_name(_e: u64, _t: u64, doc: u64, n: u64, _b: u64, _c: u64) -> u64 {
    let Some(id) = node(n) else { return string("") };
    let out = rts_dom::store::with_dom(handle(doc), |d| {
        d.tag_name(id).unwrap_or_default().to_string()
    })
    .unwrap_or_default();
    string(&out)
}

extern "C" fn node_type(_e: u64, _t: u64, doc: u64, n: u64, _b: u64, _c: u64) -> u64 {
    let Some(id) = node(n) else { return int(-1) };
    int(rts_dom::store::with_dom(handle(doc), |d| d.node_type(id)).unwrap_or(-1))
}

extern "C" fn child_count(_e: u64, _t: u64, doc: u64, n: u64, _b: u64, _c: u64) -> u64 {
    let Some(id) = node(n) else { return int(0) };
    let count = rts_dom::store::with_dom(handle(doc), |d| d.child_elements(id).len()).unwrap_or(0);
    int(count as i64)
}

extern "C" fn child_at(_e: u64, _t: u64, doc: u64, n: u64, i: u64, _c: u64) -> u64 {
    let Some(id) = node(n) else { return int(-1) };
    let i = integer(i, 0).max(0) as usize;
    let out = rts_dom::store::with_dom(handle(doc), |d| {
        d.child_elements(id).get(i).map(|c| c.to_abi()).unwrap_or(-1)
    })
    .unwrap_or(-1);
    int(out)
}

extern "C" fn parent_element(_e: u64, _t: u64, doc: u64, n: u64, _b: u64, _c: u64) -> u64 {
    let Some(id) = node(n) else { return int(-1) };
    let out = rts_dom::store::with_dom(handle(doc), |d| {
        d.parent_element(id).map(|p| p.to_abi()).unwrap_or(-1)
    })
    .unwrap_or(-1);
    int(out)
}

extern "C" fn inner_html(_e: u64, _t: u64, doc: u64, n: u64, _b: u64, _c: u64) -> u64 {
    let Some(id) = node(n) else { return string("") };
    let out = rts_dom::store::with_dom(handle(doc), |d| d.inner_html(id).unwrap_or_default())
        .unwrap_or_default();
    string(&out)
}

extern "C" fn outer_html(_e: u64, _t: u64, doc: u64, n: u64, _b: u64, _c: u64) -> u64 {
    let Some(id) = node(n) else { return string("") };
    let out = rts_dom::store::with_dom(handle(doc), |d| d.outer_html(id).unwrap_or_default())
        .unwrap_or_default();
    string(&out)
}

extern "C" fn set_text(_e: u64, _t: u64, doc: u64, n: u64, s: u64, _c: u64) -> u64 {
    let s = text(s);
    let Some(id) = node(n) else { return nothing() };
    rts_dom::store::with_dom_mut(handle(doc), |d| d.set_text(id, &s));
    nothing()
}

extern "C" fn set_attribute(_e: u64, _t: u64, doc: u64, n: u64, name: u64, v: u64) -> u64 {
    let (name, v) = (text(name), text(v));
    let Some(id) = node(n) else { return nothing() };
    rts_dom::store::with_dom_mut(handle(doc), |d| d.set_attr(id, &name, &v));
    nothing()
}

extern "C" fn remove_attribute(_e: u64, _t: u64, doc: u64, n: u64, name: u64, _c: u64) -> u64 {
    let name = text(name);
    let Some(id) = node(n) else { return nothing() };
    rts_dom::store::with_dom_mut(handle(doc), |d| d.remove_attr(id, &name));
    nothing()
}

extern "C" fn create_element(_e: u64, _t: u64, doc: u64, tag: u64, _b: u64, _c: u64) -> u64 {
    let tag = text(tag);
    let out = rts_dom::store::with_dom_mut(handle(doc), |d| d.create_element(&tag).to_abi())
        .unwrap_or(-1);
    int(out)
}

extern "C" fn create_text_node(_e: u64, _t: u64, doc: u64, s: u64, _b: u64, _c: u64) -> u64 {
    let s = text(s);
    let out = rts_dom::store::with_dom_mut(handle(doc), |d| d.create_text_node(&s).to_abi())
        .unwrap_or(-1);
    int(out)
}

extern "C" fn append_child(_e: u64, _t: u64, doc: u64, parent: u64, child: u64, _c: u64) -> u64 {
    let (Some(p), Some(c)) = (node(parent), node(child)) else { return nothing() };
    rts_dom::store::with_dom_mut(handle(doc), |d| d.append_child(p, c));
    nothing()
}

extern "C" fn remove_node(_e: u64, _t: u64, doc: u64, n: u64, _b: u64, _c: u64) -> u64 {
    let Some(id) = node(n) else { return nothing() };
    rts_dom::store::with_dom_mut(handle(doc), |d| d.remove_node(id));
    nothing()
}

extern "C" fn set_inner_html(_e: u64, _t: u64, doc: u64, n: u64, s: u64, _c: u64) -> u64 {
    let s = text(s);
    let Some(id) = node(n) else { return nothing() };
    rts_dom::store::with_dom_mut(handle(doc), |d| d.set_inner_html(id, &s));
    nothing()
}

/// `setStyle(doc, node, slot, val)` — aplica UM slot de estilo OPACO (invariante
/// 4 do roadmap: Rust nunca casa nome CSS) como override POR-NÓ deste nó, a 3ª e
/// mais forte camada da cascade (vence tag e `style=""` inline).
///
/// A mutação já existia — `Dom::set_node_style_slot` (`dom/estilo.rs`) aplica o
/// mesmo `apply_slot` que `apply_style_batch` (o motor por trás de
/// `setStyleBatch`, essa AINDA não registada aqui) aplica por tripla — só a
/// chave `setStyle` nunca foi. `Element.setStyle(slot, val)` (`dom.ts`) já
/// chamava `dom.setStyle(...)` sem que nenhum ficheiro deste crate a
/// expusesse: `el.setStyle(0, 255)` lançava `TypeError: dom.setStyle is not a
/// function`.
extern "C" fn set_style(_e: u64, _t: u64, doc: u64, n: u64, slot: u64, val: u64) -> u64 {
    let Some(id) = node(n) else { return nothing() };
    let (slot, val) = (integer(slot, 0), integer(val, 0));
    rts_dom::store::with_dom_mut(handle(doc), |d| d.set_node_style_slot(id, slot, val));
    nothing()
}

/// `computedProperty(doc, node, nome)` — o valor COMPUTADO de uma propriedade
/// CSS, no formato do browser (`getComputedStyle(el).color`).
extern "C" fn computed_property(_e: u64, _t: u64, doc: u64, n: u64, name: u64, _c: u64) -> u64 {
    let name = text(name);
    let Some(id) = node(n) else { return string("") };
    let out = rts_dom::store::with_dom(handle(doc), |d| d.computed_property(id, &name))
        .unwrap_or_default();
    string(&out)
}

/// `boundingRect(doc, node, componente)` — x, y, largura ou altura da caixa,
/// como o `getBoundingClientRect` do browser devolve.
///
/// Um componente por chamada, e não um objeto: montar um objeto exigiria que
/// este crate construísse um valor composto do runtime, e quem chama já está em
/// TypeScript, onde `{x: r(0), y: r(1)}` é uma linha.
extern "C" fn bounding_rect(_e: u64, _t: u64, doc: u64, n: u64, which: u64, _c: u64) -> u64 {
    let Some(id) = node(n) else { return num(0.0) };
    let which = integer(which, 0);
    let v = rts_dom::store::with_dom(handle(doc), |d| {
        d.bounding_component(id, which as i64)
    })
    .unwrap_or(0.0);
    num(v as f64)
}

/// `boundingRectAll(doc, ids, saida)` — as caixas de MUITOS nós numa chamada.
///
/// `ids` é um `Float64Array` com um `NodeId` da ABI por posição; `saida` é um
/// `Float32Array` com espaço para `4 * ids.length` (x, y, w, h por nó, na mesma
/// ordem). Responde quantos nós foram escritos.
///
/// # Porque uma lista, quando `boundingRect` já responde uma componente
///
/// Porque cada `boundingRect` faz um `layout_document` inteiro (13,7 ms na
/// Wikipédia), e quem quer a árvore toda paga isso uma vez por COMPONENTE. Este
/// faz o layout uma vez para a lista inteira — medido no extrator de paridade
/// sobre a Wikipédia (16 813 elementos), 9m21s para 9,7s, com o dump a sair
/// byte a byte igual. A alternativa rejeitada foi guardar o
/// `DisplayList` no `Dom`: mais rápida ainda e com uma pergunta de invalidação
/// que ninguém responde hoje, e uma caixa que não reflete o DOM é uma medição
/// errada e não uma medição rápida.
///
/// # Porque BUFFERS e não um array
///
/// Pela mesma razão que `querySelectorAll` cruza como `count`+`at(i)`: este
/// crate não constrói valores compostos do runtime. Um buffer de números já é um
/// bloco de bytes dos dois lados, então nada é construído — e 16 mil elementos
/// cruzam a fronteira duas vezes em vez de 67 mil.
extern "C" fn bounding_rect_all(_e: u64, _t: u64, doc: u64, ids: u64, out: u64, _c: u64) -> u64 {
    let bytes = rts_core::entry::with_runtime(|c| rts_core::entry::bytes_of(c, ids));
    let Some(bytes) = bytes else { return int(0) };
    let mut nodes: Vec<NodeId> = Vec::with_capacity(bytes.len() / 8);
    for chunk in bytes.chunks_exact(8) {
        let v = f64::from_le_bytes(chunk.try_into().unwrap_or([0; 8]));
        // `-1` (nó nenhum) e um id de outra árvore caem os dois no mesmo sítio:
        // `resolve` falha e a caixa sai a zeros, como no singular.
        nodes.push(NodeId::from_abi(v as i64).unwrap_or(NodeId {
            generation: u32::MAX,
            idx: u32::MAX,
        }));
    }
    let comps = rts_dom::store::with_dom(handle(doc), |d| d.bounding_components_many(&nodes))
        .unwrap_or_default();
    let mut raw: Vec<u8> = Vec::with_capacity(comps.len() * 4);
    for v in &comps {
        raw.extend_from_slice(&v.to_le_bytes());
    }
    let written = rts_core::entry::with_runtime(|c| rts_core::entry::write_bytes(c, out, 0, &raw));
    int((written / 16) as i64)
}

/// `setPixels(doc, node, buffer, w, h)` — os PIXELS de um nó de imagem.
///
/// É por aqui que um `<canvas>` ganha conteúdo: o layout emite um
/// `DisplayItem::Image` e o backend sobe como textura. Os bytes são RGBA8, `w*h*4`
/// deles, num `Buffer` — o mesmo caminho que o `<img>` do mini-browser usa
/// depois de baixar e decodificar, e a razão de o canvas não precisar de um
/// segundo mecanismo.
extern "C" fn set_pixels(_e: u64, _t: u64, doc: u64, n: u64, buffer: u64, size: u64) -> u64 {
    let Some(id) = node(n) else { return nothing() };
    let (w, h) = {
        let packed = integer(size, 0);
        ((packed >> 16) as u32, (packed & 0xFFFF) as u32)
    };
    let bytes = rts_core::entry::with_runtime(|context| {
        rts_core::entry::bytes_of(context, buffer)
    });
    let Some(bytes) = bytes else { return nothing() };
    rts_dom::store::with_dom_mut(handle(doc), |d| d.set_pixel_data(id, bytes, w, h));
    nothing()
}
