//! O DOCUMENTO: criar, liberar, alimentar com CSS e alcançar a raiz.
//!
//! Um documento vive no store do `rts-dom` e é nomeado por um handle `u64`. O
//! programa recebe esse handle de `parseHtml` e o passa de volta em cada
//! chamada — a mesma forma que a janela do `rts:egui` usa, e pela mesma razão:
//! o motor não conhece nem janela nem documento, então nenhum dos dois pode ser
//! uma variante do `Entry` do runtime.

use rts_core::entry::Provided;
use rts_dom::NodeId;

use crate::value::{handle, int, integer, nothing, string, text};

fn node(value: u64) -> Option<NodeId> {
    NodeId::from_abi(integer(value, -1))
}

pub const MEMBERS: &[(&str, Provided)] = &[
    ("parseHtml", parse_html),
    ("createDocument", create_document),
    ("free", free),
    ("rootId", root_id),
    ("documentElement", document_element),
    ("addStylesheet", add_stylesheet),
    ("mediaMatches", media_matches),
    ("setPrefersColorScheme", set_prefers_color_scheme),
    ("setPrefersReducedMotion", set_prefers_reduced_motion),
    ("dump", dump),
    ("nodeCount", node_count),
    ("releaseSubtree", release_subtree),
    ("getByTagCount", get_by_tag_count),
    ("getByTagAt", get_by_tag_at),
    ("runScript", run_script),
];

/// `parseHtml(source)` — o handle do documento (0 em falha).
extern "C" fn parse_html(_e: u64, _t: u64, source: u64, _a: u64, _b: u64, _c: u64) -> u64 {
    let dom = rts_dom::parse_html_to_dom(&text(source));
    int(rts_dom::store::insert(dom) as i64)
}

/// `createDocument()` — um documento VAZIO, com só a raiz `#document`.
///
/// Existe separado do `parseHtml("")` porque é o caminho de quem monta a árvore
/// por código (`createElement`/`appendChild`), e passar por um parser para
/// obter uma árvore vazia é uma indireção que só confunde quem lê a chamada.
extern "C" fn create_document(_e: u64, _t: u64, _a: u64, _b: u64, _c: u64, _d: u64) -> u64 {
    let dom = rts_dom::parse_html_to_dom("");
    int(rts_dom::store::insert(dom) as i64)
}

/// `free(doc)` — solta o documento. O handle fica inválido.
extern "C" fn free(_e: u64, _t: u64, doc: u64, _a: u64, _b: u64, _c: u64) -> u64 {
    crate::raizes::liberta_documento(handle(doc));
    rts_dom::store::remove(handle(doc));
    nothing()
}

/// `rootId(doc)` — o `NodeId` da raiz `#document`, versionado.
extern "C" fn root_id(_e: u64, _t: u64, doc: u64, _a: u64, _b: u64, _c: u64) -> u64 {
    let id = rts_dom::store::with_dom(handle(doc), |d| d.root_id().to_abi()).unwrap_or(-1);
    int(id)
}

/// `addStylesheet(doc, css)` — acrescenta regras de autor ao documento.
extern "C" fn document_element(_e: u64, _t: u64, doc: u64, _a: u64, _b: u64, _c: u64) -> u64 {
    let id = rts_dom::store::with_dom(handle(doc), |d| {
        d.document_element().map(|n| n.to_abi()).unwrap_or(-1)
    })
    .unwrap_or(-1);
    int(id)
}

extern "C" fn add_stylesheet(_e: u64, _t: u64, doc: u64, css: u64, _b: u64, _c: u64) -> u64 {
    let css = text(css);
    rts_dom::store::with_dom_mut(handle(doc), |d| d.add_stylesheet(&css));
    nothing()
}

/// `mediaMatches(doc, query)` — `window.matchMedia(query).matches` (lote P,
/// §5.P item 2). O MESMO avaliador que a cascade usa para `@media` — ver
/// `Dom::media_matches`.
extern "C" fn media_matches(_e: u64, _t: u64, doc: u64, query: u64, _b: u64, _c: u64) -> u64 {
    let query = text(query);
    let yes = rts_dom::store::with_dom(handle(doc), |d| d.media_matches(&query)).unwrap_or(false);
    int(yes as i64)
}

/// `setPrefersColorScheme(doc, dark)` — o host declara a preferência do SO
/// (lote P item 1). `dark` é 0/1 (a ABI não distingue bool de int).
extern "C" fn set_prefers_color_scheme(_e: u64, _t: u64, doc: u64, dark: u64, _b: u64, _c: u64) -> u64 {
    let dark = integer(dark, 0) != 0;
    rts_dom::store::with_dom_mut(handle(doc), |d| d.set_prefers_color_scheme(dark));
    nothing()
}

/// `setPrefersReducedMotion(doc, reduce)` — o host declara a preferência.
extern "C" fn set_prefers_reduced_motion(_e: u64, _t: u64, doc: u64, reduce: u64, _b: u64, _c: u64) -> u64 {
    let reduce = integer(reduce, 0) != 0;
    rts_dom::store::with_dom_mut(handle(doc), |d| d.set_prefers_reduced_motion(reduce));
    nothing()
}

/// `dump(doc)` — a árvore como texto indentado. Diagnóstico, não formato.
extern "C" fn dump(_e: u64, _t: u64, doc: u64, _a: u64, _b: u64, _c: u64) -> u64 {
    let out = rts_dom::store::with_dom(handle(doc), |d| d.dump()).unwrap_or_default();
    string(&out)
}

/// `getByTagCount(doc, tag)` — número de elementos com a tag, em ordem documental.
extern "C" fn get_by_tag_count(_e: u64, _t: u64, doc: u64, tag: u64, _b: u64, _c: u64) -> u64 {
    let tag = text(tag);
    let count = rts_dom::store::with_dom(handle(doc), |d| d.query_all(&tag).len()).unwrap_or(0);
    int(count as i64)
}

/// `getByTagAt(doc, tag, index)` — i-ésimo elemento da tag, ou `-1`.
extern "C" fn get_by_tag_at(_e: u64, _t: u64, doc: u64, tag: u64, index: u64, _c: u64) -> u64 {
    let tag = text(tag);
    let index = integer(index, -1);
    let id = rts_dom::store::with_dom(handle(doc), |d| {
        if index < 0 {
            return -1;
        }
        d.query_all(&tag)
            .get(index as usize)
            .map(|node| node.to_abi())
            .unwrap_or(-1)
    })
    .unwrap_or(-1);
    int(id)
}

/// `runScript(doc, node, source)` — materializa um script externo no nó para que
/// a fachada TypeScript o execute na etapa seguinte.
extern "C" fn run_script(_e: u64, _t: u64, doc: u64, n: u64, source: u64, _c: u64) -> u64 {
    let Some(id) = node(n) else { return nothing() };
    let source = text(source);
    rts_dom::store::with_dom_mut(handle(doc), |d| d.set_text(id, &source));
    nothing()
}

/// `nodeCount(doc)` — quantos nós a arena tem. Inclui os desanexados SEM
/// wrapper (ainda não passaram por `releaseSubtree`): é uma medida de
/// MEMÓRIA, não da árvore visível. Deixa de CRESCER sem limite a partir do
/// lote M — um slot reciclado é reusado em vez de a arena crescer.
extern "C" fn node_count(_e: u64, _t: u64, doc: u64, _a: u64, _b: u64, _c: u64) -> u64 {
    let n = rts_dom::store::with_dom(handle(doc), |d| d.nodes.len()).unwrap_or(0);
    int(n as i64)
}

/// `releaseSubtree(doc, node)` — recicla `node` (já desanexado) e a sua
/// subárvore, devolvendo os índices à freelist (`dom/freelist.rs`, lote M).
/// A FACHADA decide quando é seguro chamar isto: só quando nenhum wrapper TS
/// aponta para `node` ou para um descendente dele — o Rust não tem como saber
/// isso sozinho, então nunca recicla por conta própria em `remove_node`.
extern "C" fn release_subtree(_e: u64, _t: u64, doc: u64, n: u64, _b: u64, _c: u64) -> u64 {
    let Some(id) = node(n) else { return nothing() };
    rts_dom::store::with_dom_mut(handle(doc), |d| d.release_subtree(id));
    nothing()
}
