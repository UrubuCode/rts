//! O DOCUMENTO: criar, liberar, alimentar com CSS e alcançar a raiz.
//!
//! Um documento vive no store do `rts-dom` e é nomeado por um handle `u64`. O
//! programa recebe esse handle de `parseHtml` e o passa de volta em cada
//! chamada — a mesma forma que a janela do `rts:egui` usa, e pela mesma razão:
//! o motor não conhece nem janela nem documento, então nenhum dos dois pode ser
//! uma variante do `Entry` do runtime.

use rts_core::entry::Provided;

use crate::value::{handle, int, nothing, string, text};

pub const MEMBERS: &[(&str, Provided)] = &[
    ("parseHtml", parse_html),
    ("createDocument", create_document),
    ("free", free),
    ("rootId", root_id),
    ("documentElement", document_element),
    ("addStylesheet", add_stylesheet),
    ("dump", dump),
    ("nodeCount", node_count),
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

/// `dump(doc)` — a árvore como texto indentado. Diagnóstico, não formato.
extern "C" fn dump(_e: u64, _t: u64, doc: u64, _a: u64, _b: u64, _c: u64) -> u64 {
    let out = rts_dom::store::with_dom(handle(doc), |d| d.dump()).unwrap_or_default();
    string(&out)
}

/// `nodeCount(doc)` — quantos nós a arena tem. Inclui os desanexados: é uma
/// medida de MEMÓRIA, não da árvore visível.
extern "C" fn node_count(_e: u64, _t: u64, doc: u64, _a: u64, _b: u64, _c: u64) -> u64 {
    let n = rts_dom::store::with_dom(handle(doc), |d| d.nodes.len()).unwrap_or(0);
    int(n as i64)
}
