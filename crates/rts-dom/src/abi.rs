//! ABI headless do namespace `rts:dom` — parse/query/mutação de um DOM avulso,
//! SEM nenhuma janela. Cada `Dom` vive num store `thread_local` próprio (handle
//! `u64`), espelhando o padrão do `rts-egui` (`UiCtx` num thread_local) MAS sem
//! depender de UI: um consumidor TS pode parsear HTML, consultar e mutar a árvore
//! puramente em memória.
//!
//! DOUTRINA: o engine NUNCA nomeia `dom` — registra `register` via a tabela
//! `Registration` (`registry_build.rs`). Nenhum valor polimórfico cruza a borda:
//! strings entram como `StrPtr` da ABI; `NodeId` cruza VERSIONADO num `i64`
//! (`to_abi`/`from_abi`); a sentinela "nenhum" é `-1` (invariante 3 do roadmap).
//!
//! O `rts-egui` tem seu PRÓPRIO DOM no `UiCtx` (consome o tipo `Dom` deste crate
//! direto, sem passar pela ABI). Esta ABI é o caminho HEADLESS, paralelo.

use rts_engine::abi::str_abi;
use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

use crate::dom::{parse_html_to_dom, NodeId};

// Aloca uma string no pool de strings GC e devolve seu handle `u64` (o que o TS
// recebe como `string`). Mesmo padrão de `rts-shared::buffer` /
// `rts-primitives::string::rt`: a fn é definida em `rts-std`
// (collector::string_pool, símbolo `__RTS_FN_NS_GC_STRING_NEW`), resolvida via
// `rts_engine::gc_surface` (site único de declaração, link-time) — mantém a
// doutrina "rts-dom depende só de rts-engine".
use rts_engine::gc_surface::__RTS_FN_NS_GC_STRING_NEW;

/// Interna uma `&str` no pool de strings GC e devolve o handle (`0` p/ vazia é
/// válido — `gc.string_new` aceita len 0). É a forma de DEVOLVER string do Rust
/// pro TS pela ABI (a ABI proíbe `StrPtr` de RETORNO; só de arg estático).
fn intern(s: &str) -> u64 {
    unsafe { __RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64) }
}

// Handle de DOM avulso cruza como `u64` (mesma convenção do egui UiCtx); NodeId
// versionado cruza como `i64`. Aliases locais p/ legibilidade das `Sig`.
use AbiType::{I64, StrPtr, U64 as Handle};

// O store de `Dom`s vive em `crate::store` (fonte única da verdade, compartilhada
// com o renderer). Aliases locais curtos para as funções da ABI lerem/escreverem.
use crate::store::{insert, with_dom as with, with_dom_mut as with_mut};

/// Sentinela "nó não encontrado" — `-1` num `i64` (invariante 3: `u64::MAX` > 2^53
/// não é exato como `number` no TS). Igual à do `rts-egui`.
const NODE_NONE: i64 = -1;

// ── Funções extern "C" da ABI `dom.*` ──────────────────────────────────────────

/// `parseHtml(html)` → handle do DOM avulso (≥ 1). Parseia a string numa árvore
/// retida nova (geração própria). É o entry-point headless.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_PARSE_HTML(ptr: *const u8, len: i64) -> u64 {
    let html = unsafe { str_abi::from_abi(ptr, len) }.unwrap_or("");
    insert(parse_html_to_dom(html))
}

/// `createDocument()` → handle de um DOM vazio (só `#document`), para montar a
/// árvore por `createElement`/`appendChild` sem parsear HTML.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_CREATE_DOCUMENT() -> u64 {
    insert(parse_html_to_dom(""))
}

/// `free(domHandle)` — libera o DOM avulso (o handle fica inválido).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_FREE(h: u64) {
    crate::store::remove(h);
}

/// `querySelector(domHandle, selector)` → `NodeId` versionado (`i64` ≥ 0) ou `-1`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_QUERY_SELECTOR(h: u64, sel_ptr: *const u8, sel_len: i64) -> i64 {
    let sel = unsafe { str_abi::from_abi(sel_ptr, sel_len) }.unwrap_or("");
    with(h, |dom| dom.query(sel).map(|id| id.to_abi()).unwrap_or(NODE_NONE)).unwrap_or(NODE_NONE)
}

/// `setText(domHandle, node, text)` — `element.textContent = text`. `node` é o
/// `NodeId` VERSIONADO (i64); um id de árvore velha é rejeitado pelo `gen`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_SET_TEXT(h: u64, id: i64, ptr: *const u8, len: i64) {
    let Some(node) = NodeId::from_abi(id) else { return };
    let txt = unsafe { str_abi::from_abi(ptr, len) }.unwrap_or("").to_string();
    with_mut(h, |dom| dom.set_text(node, &txt));
}

/// `setAttr(domHandle, node, name, value)` — `element.setAttribute`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_SET_ATTR(
    h: u64,
    id: i64,
    name_ptr: *const u8,
    name_len: i64,
    val_ptr: *const u8,
    val_len: i64,
) {
    let Some(node) = NodeId::from_abi(id) else { return };
    let name = unsafe { str_abi::from_abi(name_ptr, name_len) }.unwrap_or("").to_string();
    let val = unsafe { str_abi::from_abi(val_ptr, val_len) }.unwrap_or("").to_string();
    with_mut(h, |dom| dom.set_attr(node, &name, &val));
}

/// `createElement(domHandle, tag)` → `NodeId` versionado (≥ 0) do elemento solto,
/// ou `-1` se o handle não existe. Ligue com `appendChild`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_CREATE_ELEMENT(h: u64, tag_ptr: *const u8, tag_len: i64) -> i64 {
    let tag = unsafe { str_abi::from_abi(tag_ptr, tag_len) }.unwrap_or("").to_string();
    with_mut(h, |dom| dom.create_element(&tag).to_abi()).unwrap_or(NODE_NONE)
}

/// `appendChild(domHandle, parent, child)` — `parent.appendChild(child)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_APPEND_CHILD(h: u64, parent: i64, child: i64) {
    let (Some(parent), Some(child)) = (NodeId::from_abi(parent), NodeId::from_abi(child)) else {
        return;
    };
    with_mut(h, |dom| dom.append_child(parent, child));
}

/// `removeNode(domHandle, node)` — `element.remove()`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_REMOVE_NODE(h: u64, id: i64) {
    let Some(node) = NodeId::from_abi(id) else { return };
    with_mut(h, |dom| dom.remove_node(node));
}

/// `createTextNode(domHandle, text)` → `NodeId` de um nó de texto solto (≥ 0), ou
/// `-1`. Ligue com `appendChild`/`insertBefore`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_CREATE_TEXT_NODE(h: u64, ptr: *const u8, len: i64) -> i64 {
    let text = unsafe { str_abi::from_abi(ptr, len) }.unwrap_or("").to_string();
    with_mut(h, |dom| dom.create_text_node(&text).to_abi()).unwrap_or(NODE_NONE)
}

/// `insertBefore(domHandle, parent, child, reference)` — `parent.insertBefore(
/// child, reference)`. `reference = -1` (ou não-filho) → anexa ao fim.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_INSERT_BEFORE(h: u64, parent: i64, child: i64, reference: i64) {
    let (Some(parent), Some(child)) = (NodeId::from_abi(parent), NodeId::from_abi(child)) else {
        return;
    };
    let reference = NodeId::from_abi(reference);
    with_mut(h, |dom| dom.insert_before(parent, child, reference));
}

// ── Navegação (parentNode / first|lastChild / next|previousSibling) ──────────────
// Cada um recebe um `NodeId` e devolve outro (`-1` quando não há). Extraia o
// retorno para uma const antes de comparar com -1 (limite do motor i64-cmp inline).

/// `parentNode(domHandle, node)` → `NodeId` do pai, ou `-1` (raiz / inválido).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_PARENT_NODE(h: u64, id: i64) -> i64 {
    let Some(node) = NodeId::from_abi(id) else { return NODE_NONE };
    with(h, |dom| dom.parent_of(node).map(|n| n.to_abi()).unwrap_or(NODE_NONE)).unwrap_or(NODE_NONE)
}

/// `firstChild(domHandle, node)` → 1º filho (qualquer tipo, inclui Text), ou `-1`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_FIRST_CHILD(h: u64, id: i64) -> i64 {
    let Some(node) = NodeId::from_abi(id) else { return NODE_NONE };
    with(h, |dom| dom.first_child(node).map(|n| n.to_abi()).unwrap_or(NODE_NONE)).unwrap_or(NODE_NONE)
}

/// `lastChild(domHandle, node)` → último filho, ou `-1`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_LAST_CHILD(h: u64, id: i64) -> i64 {
    let Some(node) = NodeId::from_abi(id) else { return NODE_NONE };
    with(h, |dom| dom.last_child(node).map(|n| n.to_abi()).unwrap_or(NODE_NONE)).unwrap_or(NODE_NONE)
}

/// `nextSibling(domHandle, node)` → próximo irmão, ou `-1` (é o último).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_NEXT_SIBLING(h: u64, id: i64) -> i64 {
    let Some(node) = NodeId::from_abi(id) else { return NODE_NONE };
    with(h, |dom| dom.next_sibling(node).map(|n| n.to_abi()).unwrap_or(NODE_NONE))
        .unwrap_or(NODE_NONE)
}

/// `previousSibling(domHandle, node)` → irmão anterior, ou `-1` (é o primeiro).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_PREVIOUS_SIBLING(h: u64, id: i64) -> i64 {
    let Some(node) = NodeId::from_abi(id) else { return NODE_NONE };
    with(h, |dom| dom.previous_sibling(node).map(|n| n.to_abi()).unwrap_or(NODE_NONE))
        .unwrap_or(NODE_NONE)
}

/// `childNodesCount(domHandle, node)` → nº de filhos TOTAL (inclui Text); par com
/// `childNodeAt` (igual a childCount/childAt, mas SEM filtrar elementos).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_CHILD_NODES_COUNT(h: u64, id: i64) -> i64 {
    let Some(node) = NodeId::from_abi(id) else { return 0 };
    with(h, |dom| dom.child_nodes(node).len() as i64).unwrap_or(0)
}

/// `childNodeAt(domHandle, node, index)` → o índice-ésimo filho (inclui Text), -1.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_CHILD_NODE_AT(h: u64, id: i64, index: i64) -> i64 {
    if index < 0 {
        return NODE_NONE;
    }
    let Some(node) = NodeId::from_abi(id) else { return NODE_NONE };
    with(h, |dom| {
        dom.child_nodes(node).get(index as usize).map(|c| c.to_abi()).unwrap_or(NODE_NONE)
    })
    .unwrap_or(NODE_NONE)
}

/// `nodeType(domHandle, node)` → código DOM: Element=1, Text=3, Comment=8,
/// Document=9; `-1` se inválido.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_NODE_TYPE(h: u64, id: i64) -> i64 {
    let Some(node) = NodeId::from_abi(id) else { return NODE_NONE };
    with(h, |dom| dom.node_type(node)).unwrap_or(NODE_NONE)
}

/// `nodeName(domHandle, node)` → nome DOM (tag p/ Element; `#text`/`#comment`/
/// `#document`), como handle de string. Vazio se inválido.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_NODE_NAME(h: u64, id: i64) -> u64 {
    let Some(node) = NodeId::from_abi(id) else { return intern("") };
    let name = with(h, |dom| dom.node_name(node).unwrap_or_default()).unwrap_or_default();
    intern(&name)
}

/// `rootId(domHandle)` → `NodeId` versionado da raiz `#document` (≥ 0), ou `-1`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_ROOT_ID(h: u64) -> i64 {
    with(h, |dom| dom.root_id().to_abi()).unwrap_or(NODE_NONE)
}

/// `dump(domHandle)` — imprime a árvore (devtools-style) no stderr, para inspeção.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_DUMP(h: u64) {
    if let Some(s) = with(h, |dom| dom.dump()) {
        eprint!("{s}");
    }
}

/// `dumpLayout(domHandle, viewportW)` — computa a DisplayList (layout no DOM, com a
/// largura de viewport dada) e imprime um JSON com cada item (tipo + x/y/w/h + cor),
/// para COMPARAR número-a-número com o render do navegador (o JSON do
/// `extrair-render.js`). Usa o medidor aproximado (headless, determinístico).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_DUMP_LAYOUT(h: u64, viewport_w: i64) {
    use crate::layout::{layout_document, ApproxMeasurer, DisplayItem, LayoutCtx};
    let vw = viewport_w.max(1) as f32;
    let json = with(h, |dom| {
        let ctx = LayoutCtx { viewport_w: vw, viewport_h: 800.0, measurer: &ApproxMeasurer };
        let list = layout_document(dom, &ctx);
        let mut s = String::from("{\n  \"viewport\": ");
        s.push_str(&(vw as i64).to_string());
        s.push_str(",\n  \"content_height\": ");
        s.push_str(&(list.content_height as i64).to_string());
        s.push_str(",\n  \"items\": [\n");
        let hx = |c: u32| format!("0x{:08X}", c);
        for (i, it) in list.items.iter().enumerate() {
            let line = match it {
                DisplayItem::SolidRect { rect, color, .. } => format!(
                    "    {{\"kind\":\"rect\",\"x\":{:.1},\"y\":{:.1},\"w\":{:.1},\"h\":{:.1},\"bg\":\"{}\"}}",
                    rect.x, rect.y, rect.w, rect.h, hx(*color)
                ),
                DisplayItem::Border { rect, width, color, .. } => format!(
                    "    {{\"kind\":\"border\",\"x\":{:.1},\"y\":{:.1},\"w\":{:.1},\"h\":{:.1},\"width\":{:.1},\"color\":\"{}\"}}",
                    rect.x, rect.y, rect.w, rect.h, width, hx(*color)
                ),
                DisplayItem::Text { x, y, text, color, size, .. } => format!(
                    "    {{\"kind\":\"text\",\"x\":{:.1},\"y\":{:.1},\"size\":{:.1},\"color\":\"{}\",\"text\":{:?}}}",
                    x, y, size, hx(*color), text
                ),
                DisplayItem::BeginClip { rect, .. } => format!(
                    "    {{\"kind\":\"beginClip\",\"x\":{:.1},\"y\":{:.1},\"w\":{:.1},\"h\":{:.1}}}",
                    rect.x, rect.y, rect.w, rect.h
                ),
                DisplayItem::Shadow { rect, blur, color, .. } => format!(
                    "    {{\"kind\":\"shadow\",\"x\":{:.1},\"y\":{:.1},\"w\":{:.1},\"h\":{:.1},\"blur\":{:.1},\"color\":\"{}\"}}",
                    rect.x, rect.y, rect.w, rect.h, blur, hx(*color)
                ),
                DisplayItem::GradientRect { rect, c0, c1, .. } => format!(
                    "    {{\"kind\":\"gradient\",\"x\":{:.1},\"y\":{:.1},\"w\":{:.1},\"h\":{:.1},\"c0\":\"{}\",\"c1\":\"{}\"}}",
                    rect.x, rect.y, rect.w, rect.h, hx(*c0), hx(*c1)
                ),
                DisplayItem::Image { rect, img_w, img_h, .. } => format!(
                    "    {{\"kind\":\"image\",\"x\":{:.1},\"y\":{:.1},\"w\":{:.1},\"h\":{:.1},\"imgW\":{},\"imgH\":{}}}",
                    rect.x, rect.y, rect.w, rect.h, img_w, img_h
                ),
                DisplayItem::EndClip => "    {\"kind\":\"endClip\"}".to_string(),
            };
            s.push_str(&line);
            if i + 1 < list.items.len() {
                s.push(',');
            }
            s.push('\n');
        }
        s.push_str("  ]\n}");
        s
    });
    if let Some(j) = json {
        println!("{j}");
    }
}

/// `dumpTree(domHandle, viewportW)` — imprime a ÁRVORE de elementos com o rect de
/// CADA nó (`node_rects`) + tag/id/class, indentada, para COMPARAR a geometria
/// nó-a-nó com o `getBoundingClientRect` do Chrome (a ferramenta de diagnóstico da
/// paridade de layout). Só elementos com rect (os que o layout posicionou).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_DUMP_TREE(h: u64, viewport_w: i64) {
    use crate::layout::{layout_document, ApproxMeasurer, LayoutCtx};
    let vw = viewport_w.max(1) as f32;
    let out = with(h, |dom| {
        let ctx = LayoutCtx { viewport_w: vw, viewport_h: 800.0, measurer: &ApproxMeasurer };
        let list = layout_document(dom, &ctx);
        let mut s = String::new();
        fn walk(dom: &crate::Dom, idx: usize, depth: usize, rects: &std::collections::HashMap<usize, crate::layout::Rect>, s: &mut String) {
            let node = dom.node(idx);
            if let crate::dom::NodeKind::Element { tag } = &node.kind {
                let id = node.attr("id").unwrap_or("");
                let cls = node.attr("class").unwrap_or("");
                let r = rects.get(&idx);
                let ind = "  ".repeat(depth);
                match r {
                    Some(r) => s.push_str(&format!(
                        "{ind}<{tag}{}{}> x={:.0} y={:.0} w={:.0} h={:.0}\n",
                        if id.is_empty() { String::new() } else { format!(" #{id}") },
                        if cls.is_empty() { String::new() } else { format!(" .{}", cls.split_whitespace().next().unwrap_or("")) },
                        r.x, r.y, r.w, r.h,
                    )),
                    None => s.push_str(&format!(
                        "{ind}<{tag}{}{}> (sem rect)\n",
                        if id.is_empty() { String::new() } else { format!(" #{id}") },
                        if cls.is_empty() { String::new() } else { format!(" .{}", cls.split_whitespace().next().unwrap_or("")) },
                    )),
                }
            }
            for &c in &dom.node(idx).children {
                let is_el = matches!(dom.node(c).kind, crate::dom::NodeKind::Element { .. });
                walk(dom, c, if is_el { depth + 1 } else { depth }, rects, s);
            }
        }
        walk(dom, dom.root, 0, &list.node_rects, &mut s);
        s
    });
    if let Some(o) = out {
        println!("{o}");
    }
}

// ── Geometria: getBoundingClientRect (x/y/w/h por nó) ───────────────────────────
// O `element.getBoundingClientRect()` lê o LAYOUT que o motor já calcula (o
// `node_rects` da DisplayList). A ABI dá 1 i64 por chamada; cada componente vem
// como pontos × 1000 (3 casas decimais, preserva subpixel tipo 302.1 → 302100), e
// a fachada `.ts` monta `{x, y, width, height, top, left, right, bottom}` dividindo
// por 1000. Para não recomputar o layout 4× (x/y/w/h), guardamos a última
// DisplayList por (handle, viewport) num cache thread-local.

thread_local! {
    /// Cache da última DisplayList computada para geometria — evita rodar o layout
    /// 4× quando a fachada lê x/y/w/h em sequência. Chave: (domHandle, viewportW,
    /// render_revision) — a revisão invalida o cache quando o DOM/estilo MUDAM
    /// (antes, mutar e reler boundingComponent devolvia rects STALE).
    static GEOM_CACHE: std::cell::RefCell<Option<(u64, i64, u64, crate::layout::DisplayList)>> =
        const { std::cell::RefCell::new(None) };
}

/// Componente do border-box de um nó (`which`: 0=x 1=y 2=width 3=height), em
/// pontos × 1000. `-1` se o nó não tem rect (texto/inline/display:none/inválido) —
/// distinto de 0 (um rect legítimo de tamanho 0 dá 0, não -1). A fachada divide /1000.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_BOUNDING_COMPONENT(h: u64, id: i64, viewport_w: i64, which: i64) -> i64 {
    use crate::layout::{ApproxMeasurer, LayoutCtx};
    let Some(node) = NodeId::from_abi(id) else { return -1 };
    let vw = viewport_w.max(1);
    // Resolve o NodeIdx cru do nó nesta árvore.
    let idx = match with(h, |dom| dom.resolve(node)) {
        Some(Some(i)) => i,
        _ => return -1,
    };
    let rev = match with(h, |dom| dom.render_revision()) {
        Some(r) => r,
        None => return -1,
    };
    GEOM_CACHE.with(|cache| {
        let mut c = cache.borrow_mut();
        // (re)computa o layout se o cache não bate com (handle, viewport, revisão).
        let need = !matches!(&*c, Some((ch, cv, cr, _)) if *ch == h && *cv == vw && *cr == rev);
        if need {
            let list = with(h, |dom| {
                let ctx = LayoutCtx { viewport_w: vw as f32, viewport_h: 800.0, measurer: &ApproxMeasurer };
                crate::layout::layout_document(dom, &ctx)
            });
            match list {
                Some(l) => *c = Some((h, vw, rev, l)),
                None => return -1,
            }
        }
        let rect = match &*c {
            Some((_, _, _, l)) => l.node_rects.get(&idx).copied(),
            None => None,
        };
        match rect {
            Some(r) => {
                let v = match which {
                    0 => r.x,
                    1 => r.y,
                    2 => r.w,
                    3 => r.h,
                    _ => return -1,
                };
                (v * 1000.0) as i64
            }
            None => -1,
        }
    })
}

// ── Formulário: input editável (mini-browser) ───────────────────────────────────
// O egui continua BURRO: ele já entrega ao TS a posição do clique e os caracteres
// digitados no frame (via os primitivos de input). Estes ABIs deixam o TS: (1)
// descobrir QUAL input está sob o cursor (hit-test no layout), (2) dar/tirar o FOCO,
// (3) alimentar texto/backspace no input focado. Toda a lógica de edição vive no
// rts-dom; o layout emite o texto+cursor na DisplayList que o egui pinta.

/// `inputAt(dom, viewportW, x, y)` → o `NodeId` do `<input>`/`<textarea>` cujo
/// border-box contém a coord `(x, y)` (coords de conteúdo da página, × 1). `-1` se
/// nenhum. Usa o layout cacheado (mesmo GEOM_CACHE do getBoundingClientRect).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_INPUT_AT(h: u64, viewport_w: i64, x: i64, y: i64) -> i64 {
    use crate::layout::{ApproxMeasurer, LayoutCtx};
    let vw = viewport_w.max(1);
    let (px, py) = (x as f32, y as f32);
    let rev = match with(h, |dom| dom.render_revision()) {
        Some(r) => r,
        None => return NODE_NONE,
    };
    GEOM_CACHE.with(|cache| {
        let mut c = cache.borrow_mut();
        let need = !matches!(&*c, Some((ch, cv, cr, _)) if *ch == h && *cv == vw && *cr == rev);
        if need {
            let list = with(h, |dom| {
                let ctx = LayoutCtx { viewport_w: vw as f32, viewport_h: 800.0, measurer: &ApproxMeasurer };
                crate::layout::layout_document(dom, &ctx)
            });
            match list {
                Some(l) => *c = Some((h, vw, rev, l)),
                None => return NODE_NONE,
            }
        }
        // Percorre os rects; devolve o input que contém o ponto (o último no z-order
        // vence se sobrepostos — inputs raramente se sobrepõem).
        let hit = match &*c {
            Some((_, _, _, l)) => {
                let mut found = NODE_NONE;
                for (idx, r) in &l.node_rects {
                    if px >= r.x && px <= r.x + r.w && py >= r.y && py <= r.y + r.h {
                        let is_input = with(h, |dom| dom.is_text_input_idx(*idx)).unwrap_or(false);
                        if is_input {
                            found = with(h, |dom| dom.id_of_idx(*idx).to_abi()).unwrap_or(NODE_NONE);
                        }
                    }
                }
                found
            }
            None => NODE_NONE,
        };
        hit
    })
}

/// `focusInput(dom, node)` → dá o foco a `node` (recebe teclas); `node == -1` tira
/// o foco de todos. O caller (loop TS) chama após um clique (via `inputAt`).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_FOCUS_INPUT(h: u64, id: i64) {
    let node = NodeId::from_abi(id);
    with_mut(h, |dom| {
        let idx = node.and_then(|n| dom.resolve(n));
        dom.focus_input(idx);
    });
}

/// `setImage(dom, node, bufferHandle, off, w, h)` → associa a um `<img>` os pixels
/// RGBA já decodificados (o browser baixa via fetchBytes + decodifica via imgdec e
/// chama isto). O layout então emite a imagem. `off` = offset dos pixels no buffer.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_SET_IMAGE(
    h: u64, id: i64, buffer_handle: u64, off: i64, w: i64, hgt: i64,
) {
    let Some(node) = NodeId::from_abi(id) else {
        return;
    };
    with_mut(h, |dom| {
        dom.set_image(node, buffer_handle, off.max(0) as u32, w.max(0) as u32, hgt.max(0) as u32);
    });
}

/// `hasImage(dom, node)` → 1 se o nó tem imagem setada (diagnóstico), 0 senão.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_HAS_IMAGE(h: u64, id: i64) -> i64 {
    let Some(node) = NodeId::from_abi(id) else { return 0 };
    with(h, |dom| {
        dom.resolve(node).and_then(|idx| dom.image_of(idx)).is_some() as i64
    })
    .unwrap_or(0)
}

/// `focusedInput(dom)` → o `NodeId` do input focado (-1 se nenhum).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_FOCUSED_INPUT(h: u64) -> i64 {
    with(h, |dom| dom.focused_input().map(|i| dom.id_of_idx(i).to_abi()).unwrap_or(NODE_NONE))
        .unwrap_or(NODE_NONE)
}

/// `inputFeedText(dom, text)` → anexa `text` ao input focado (os caracteres do
/// frame). `1` se algo mudou (pede repaint), `0` senão.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_INPUT_FEED_TEXT(h: u64, t_ptr: *const u8, t_len: i64) -> i64 {
    let t = unsafe { str_abi::from_abi(t_ptr, t_len) }.unwrap_or("").to_string();
    with_mut(h, |dom| dom.input_feed_text(&t) as i64).unwrap_or(0)
}

/// `inputBackspace(dom)` → apaga o último char do input focado. `1` se mudou.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_INPUT_BACKSPACE(h: u64) -> i64 {
    with_mut(h, |dom| dom.input_backspace() as i64).unwrap_or(0)
}

/// `inputValue(dom, node)` → o texto corrente do input (value digitado ou atributo)
/// como STRING (handle do pool GC). `""` se não for input.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_INPUT_VALUE(h: u64, id: i64) -> u64 {
    let Some(node) = NodeId::from_abi(id) else { return intern("") };
    let v = with(h, |dom| {
        dom.resolve(node).map(|idx| dom.input_value(idx)).unwrap_or_default()
    })
    .unwrap_or_default();
    intern(&v)
}

// ── Leitura de conteúdo: STRING de volta pro TS (handle do pool GC) ──────────────
// A ABI proíbe `StrPtr` de RETORNO (só de arg). A forma de devolver string é
// internar no pool GC (`intern`) e retornar o handle `u64`; no register() esse
// retorno é `AbiType::Handle` com `ts_signature` terminando em `: string`, que é
// como o motor sabe que o handle é uma string (mesmo contrato de
// `string.trim`/`to_upper`). Nó inexistente / não-elemento ⇒ string vazia "".

/// `getText(domHandle, node)` → `node.textContent` como STRING (handle do pool
/// GC). Concatena o texto de todos os descendentes. Nó inválido ⇒ `""`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_GET_TEXT(h: u64, id: i64) -> u64 {
    let Some(node) = NodeId::from_abi(id) else { return intern("") };
    let txt = with(h, |dom| dom.text_content(node).unwrap_or_default()).unwrap_or_default();
    intern(&txt)
}

/// `innerHtml(domHandle, node)` → `element.innerHTML` como STRING (handle do pool
/// GC): o HTML serializado dos FILHOS do nó. Nó inválido ⇒ `""`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_INNER_HTML(h: u64, id: i64) -> u64 {
    let Some(node) = NodeId::from_abi(id) else { return intern("") };
    let s = with(h, |dom| dom.inner_html(node).unwrap_or_default()).unwrap_or_default();
    intern(&s)
}

/// `outerHtml(domHandle, node)` → `element.outerHTML` (inclui o próprio elemento).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_OUTER_HTML(h: u64, id: i64) -> u64 {
    let Some(node) = NodeId::from_abi(id) else { return intern("") };
    let s = with(h, |dom| dom.outer_html(node).unwrap_or_default()).unwrap_or_default();
    intern(&s)
}

/// `setInnerHtml(domHandle, node, html)` → `element.innerHTML = html`: parseia o
/// HTML e SUBSTITUI os filhos do nó pela nova subárvore. Nó inválido / não-elemento
/// ⇒ no-op.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_SET_INNER_HTML(h: u64, id: i64, html_ptr: *const u8, html_len: i64) {
    let Some(node) = NodeId::from_abi(id) else { return };
    let html = unsafe { str_abi::from_abi(html_ptr, html_len) }.unwrap_or("").to_string();
    with_mut(h, |dom| dom.set_inner_html(node, &html));
}

// ── Traversal por elemento — #1757 ──────────────────────────────────────────────

/// Macro de navegação que devolve um `NodeId` (ou `-1`). Reduz boilerplate.
macro_rules! nav_fn {
    ($fn:ident, $method:ident) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn $fn(h: u64, id: i64) -> i64 {
            let Some(node) = NodeId::from_abi(id) else { return NODE_NONE };
            with(h, |dom| dom.$method(node).map(|n| n.to_abi()).unwrap_or(NODE_NONE)).unwrap_or(NODE_NONE)
        }
    };
}
nav_fn!(__RTS_FN_NS_DOM_FIRST_ELEMENT_CHILD, first_element_child);
nav_fn!(__RTS_FN_NS_DOM_LAST_ELEMENT_CHILD, last_element_child);
nav_fn!(__RTS_FN_NS_DOM_NEXT_ELEMENT_SIBLING, next_element_sibling);
nav_fn!(__RTS_FN_NS_DOM_PREVIOUS_ELEMENT_SIBLING, previous_element_sibling);
nav_fn!(__RTS_FN_NS_DOM_PARENT_ELEMENT, parent_element);

/// `closest(domHandle, node, selector)` → o NodeId do ancestral (ou o próprio) que
/// casa o seletor simples, ou `-1`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_CLOSEST(h: u64, id: i64, sel_ptr: *const u8, sel_len: i64) -> i64 {
    let Some(node) = NodeId::from_abi(id) else { return NODE_NONE };
    let sel = unsafe { str_abi::from_abi(sel_ptr, sel_len) }.unwrap_or("").to_string();
    with(h, |dom| dom.closest(node, &sel).map(|n| n.to_abi()).unwrap_or(NODE_NONE)).unwrap_or(NODE_NONE)
}

/// `matches(domHandle, node, selector)` → 1 se o nó casa o seletor, 0 senão.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_MATCHES(h: u64, id: i64, sel_ptr: *const u8, sel_len: i64) -> i64 {
    let Some(node) = NodeId::from_abi(id) else { return 0 };
    let sel = unsafe { str_abi::from_abi(sel_ptr, sel_len) }.unwrap_or("").to_string();
    with(h, |dom| dom.matches_selector(node, &sel) as i64).unwrap_or(0)
}

// ── Node utils — #1762 ──────────────────────────────────────────────────────────

/// `contains(domHandle, node, other)` → 1 se `node` contém `other` (ou é ele), 0 senão.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_CONTAINS(h: u64, id: i64, other: i64) -> i64 {
    let (Some(node), Some(o)) = (NodeId::from_abi(id), NodeId::from_abi(other)) else { return 0 };
    with(h, |dom| dom.contains(node, o) as i64).unwrap_or(0)
}

/// `hasChildNodes(domHandle, node)` → 1 se tem ao menos um filho, 0 senão.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_HAS_CHILD_NODES(h: u64, id: i64) -> i64 {
    let Some(node) = NodeId::from_abi(id) else { return 0 };
    with(h, |dom| dom.has_child_nodes(node) as i64).unwrap_or(0)
}

/// `nodeValue(domHandle, node)` → o texto cru de um nó Text/Comment como STRING; ""
/// para Element/Document (nodeValue null).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_NODE_VALUE(h: u64, id: i64) -> u64 {
    let Some(node) = NodeId::from_abi(id) else { return intern("") };
    let v = with(h, |dom| dom.node_value(node).unwrap_or_default()).unwrap_or_default();
    intern(&v)
}

/// `setNodeValue(domHandle, node, value)` → substitui o texto de um nó Text/Comment.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_SET_NODE_VALUE(h: u64, id: i64, v_ptr: *const u8, v_len: i64) {
    let Some(node) = NodeId::from_abi(id) else { return };
    let v = unsafe { str_abi::from_abi(v_ptr, v_len) }.unwrap_or("").to_string();
    with_mut(h, |dom| dom.set_node_value(node, &v));
}

/// `createComment(domHandle, text)` → NodeId de um nó de comentário solto.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_CREATE_COMMENT(h: u64, t_ptr: *const u8, t_len: i64) -> i64 {
    let text = unsafe { str_abi::from_abi(t_ptr, t_len) }.unwrap_or("").to_string();
    with_mut(h, |dom| dom.create_comment(&text).to_abi()).unwrap_or(NODE_NONE)
}

/// `normalize(domHandle, node)` → funde nós de texto adjacentes + remove vazios.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_NORMALIZE(h: u64, id: i64) {
    let Some(node) = NodeId::from_abi(id) else { return };
    with_mut(h, |dom| dom.normalize(node));
}

// ── Atributos extra — #1761 ─────────────────────────────────────────────────────

/// `removeAttr(domHandle, node, name)` → remove o atributo (no-op se ausente).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_REMOVE_ATTR(h: u64, id: i64, n_ptr: *const u8, n_len: i64) {
    let Some(node) = NodeId::from_abi(id) else { return };
    let name = unsafe { str_abi::from_abi(n_ptr, n_len) }.unwrap_or("").to_string();
    with_mut(h, |dom| dom.remove_attr(node, &name));
}

/// `hasAttr(domHandle, node, name)` → 1 se o atributo está PRESENTE (mesmo vazio),
/// 0 senão. Corrige atributos booleanos (`hidden`/`disabled`, valor `""`).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_HAS_ATTR(h: u64, id: i64, n_ptr: *const u8, n_len: i64) -> i64 {
    let Some(node) = NodeId::from_abi(id) else { return 0 };
    let name = unsafe { str_abi::from_abi(n_ptr, n_len) }.unwrap_or("").to_string();
    with(h, |dom| dom.has_attr(node, &name) as i64).unwrap_or(0)
}

/// `attrCount(domHandle, node)` → nº de atributos (para getAttributeNames/attributes).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_ATTR_COUNT(h: u64, id: i64) -> i64 {
    let Some(node) = NodeId::from_abi(id) else { return 0 };
    with(h, |dom| dom.attr_names(node).len() as i64).unwrap_or(0)
}

/// `attrNameAt(domHandle, node, i)` → nome do i-ésimo atributo como STRING.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_ATTR_NAME_AT(h: u64, id: i64, i: i64) -> u64 {
    let Some(node) = NodeId::from_abi(id) else { return intern("") };
    let name = with(h, |dom| dom.attr_names(node).get(i as usize).cloned().unwrap_or_default()).unwrap_or_default();
    intern(&name)
}

/// `attrValueAt(domHandle, node, i)` → valor do i-ésimo atributo como STRING.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_ATTR_VALUE_AT(h: u64, id: i64, i: i64) -> u64 {
    let Some(node) = NodeId::from_abi(id) else { return intern("") };
    let val = with(h, |dom| dom.attr_value_at(node, i as usize).unwrap_or_default()).unwrap_or_default();
    intern(&val)
}

// ── Query extra — #1758: getElementsBy* + querySelector por subárvore ────────────
// Mesmo padrão count+at do querySelectorAll (re-roda a coleção por índice).

/// `getByClassCount`/`getByClassAt` — elementos com a classe (HTMLCollection).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_GET_BY_CLASS_COUNT(h: u64, n_ptr: *const u8, n_len: i64) -> i64 {
    let name = unsafe { str_abi::from_abi(n_ptr, n_len) }.unwrap_or("");
    with(h, |dom| dom.get_elements_by_class_name(name).len() as i64).unwrap_or(0)
}
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_GET_BY_CLASS_AT(h: u64, n_ptr: *const u8, n_len: i64, i: i64) -> i64 {
    if i < 0 { return NODE_NONE; }
    let name = unsafe { str_abi::from_abi(n_ptr, n_len) }.unwrap_or("");
    with(h, |dom| dom.get_elements_by_class_name(name).get(i as usize).map(|n| n.to_abi()).unwrap_or(NODE_NONE)).unwrap_or(NODE_NONE)
}

/// `getByTagCount`/`getByTagAt` — elementos da tag (`*` = todos).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_GET_BY_TAG_COUNT(h: u64, n_ptr: *const u8, n_len: i64) -> i64 {
    let name = unsafe { str_abi::from_abi(n_ptr, n_len) }.unwrap_or("");
    with(h, |dom| dom.get_elements_by_tag_name(name).len() as i64).unwrap_or(0)
}
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_GET_BY_TAG_AT(h: u64, n_ptr: *const u8, n_len: i64, i: i64) -> i64 {
    if i < 0 { return NODE_NONE; }
    let name = unsafe { str_abi::from_abi(n_ptr, n_len) }.unwrap_or("");
    with(h, |dom| dom.get_elements_by_tag_name(name).get(i as usize).map(|n| n.to_abi()).unwrap_or(NODE_NONE)).unwrap_or(NODE_NONE)
}

/// `getByNameCount`/`getByNameAt` — elementos com atributo `name`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_GET_BY_NAME_COUNT(h: u64, n_ptr: *const u8, n_len: i64) -> i64 {
    let name = unsafe { str_abi::from_abi(n_ptr, n_len) }.unwrap_or("");
    with(h, |dom| dom.get_elements_by_name(name).len() as i64).unwrap_or(0)
}
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_GET_BY_NAME_AT(h: u64, n_ptr: *const u8, n_len: i64, i: i64) -> i64 {
    if i < 0 { return NODE_NONE; }
    let name = unsafe { str_abi::from_abi(n_ptr, n_len) }.unwrap_or("");
    with(h, |dom| dom.get_elements_by_name(name).get(i as usize).map(|n| n.to_abi()).unwrap_or(NODE_NONE)).unwrap_or(NODE_NONE)
}

/// `queryWithin(domHandle, root, selector)` → 1º descendente de `root` que casa, ou -1.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_QUERY_WITHIN(h: u64, id: i64, sel_ptr: *const u8, sel_len: i64) -> i64 {
    let Some(node) = NodeId::from_abi(id) else { return NODE_NONE };
    let sel = unsafe { str_abi::from_abi(sel_ptr, sel_len) }.unwrap_or("").to_string();
    with(h, |dom| dom.query_within(node, &sel).map(|n| n.to_abi()).unwrap_or(NODE_NONE)).unwrap_or(NODE_NONE)
}

/// `queryAllWithinCount`/`At` — descendentes de `root` que casam (subárvore).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_QUERY_ALL_WITHIN_COUNT(h: u64, id: i64, sel_ptr: *const u8, sel_len: i64) -> i64 {
    let Some(node) = NodeId::from_abi(id) else { return 0 };
    let sel = unsafe { str_abi::from_abi(sel_ptr, sel_len) }.unwrap_or("").to_string();
    with(h, |dom| dom.query_all_within(node, &sel).len() as i64).unwrap_or(0)
}
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_QUERY_ALL_WITHIN_AT(h: u64, id: i64, sel_ptr: *const u8, sel_len: i64, i: i64) -> i64 {
    if i < 0 { return NODE_NONE; }
    let Some(node) = NodeId::from_abi(id) else { return NODE_NONE };
    let sel = unsafe { str_abi::from_abi(sel_ptr, sel_len) }.unwrap_or("").to_string();
    with(h, |dom| dom.query_all_within(node, &sel).get(i as usize).map(|n| n.to_abi()).unwrap_or(NODE_NONE)).unwrap_or(NODE_NONE)
}

// ── Mutação rica — #1756 ─────────────────────────────────────────────────────────

/// `cloneNode(domHandle, node, deep)` → NodeId do clone solto (deep!=0 = com filhos).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_CLONE_NODE(h: u64, id: i64, deep: i64) -> i64 {
    let Some(node) = NodeId::from_abi(id) else { return NODE_NONE };
    with_mut(h, |dom| dom.clone_node(node, deep != 0).map(|n| n.to_abi()).unwrap_or(NODE_NONE)).unwrap_or(NODE_NONE)
}

/// `prepend(domHandle, parent, child)` → insere child no início dos filhos.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_PREPEND(h: u64, parent: i64, child: i64) {
    let (Some(p), Some(c)) = (NodeId::from_abi(parent), NodeId::from_abi(child)) else { return };
    with_mut(h, |dom| dom.prepend_child(p, c));
}

/// `insertAdjacent(domHandle, node, other, after)` → other como irmão antes/depois.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_INSERT_ADJACENT(h: u64, node: i64, other: i64, after: i64) {
    let (Some(n), Some(o)) = (NodeId::from_abi(node), NodeId::from_abi(other)) else { return };
    with_mut(h, |dom| dom.insert_adjacent(n, o, after != 0));
}

/// `replaceWith(domHandle, node, other)` → substitui node por other.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_REPLACE_WITH(h: u64, node: i64, other: i64) {
    let (Some(n), Some(o)) = (NodeId::from_abi(node), NodeId::from_abi(other)) else { return };
    with_mut(h, |dom| dom.replace_with(n, o));
}

/// `replaceChild(domHandle, parent, new, old)` → substitui old por new.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_REPLACE_CHILD(h: u64, parent: i64, new_child: i64, old_child: i64) {
    let (Some(p), Some(nw), Some(od)) = (NodeId::from_abi(parent), NodeId::from_abi(new_child), NodeId::from_abi(old_child)) else { return };
    with_mut(h, |dom| dom.replace_child(p, nw, od));
}

/// `removeChild(domHandle, parent, child)` → remove child se for filho de parent.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_REMOVE_CHILD(h: u64, parent: i64, child: i64) {
    let (Some(p), Some(c)) = (NodeId::from_abi(parent), NodeId::from_abi(child)) else { return };
    with_mut(h, |dom| dom.remove_child(p, c));
}

/// `clearChildren(domHandle, parent)` → remove todos os filhos (base de replaceChildren).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_CLEAR_CHILDREN(h: u64, parent: i64) {
    let Some(p) = NodeId::from_abi(parent) else { return };
    with_mut(h, |dom| dom.clear_children(p));
}

// ── element.style + getComputedStyle — #1759 ────────────────────────────────────

/// `computedProperty(domHandle, node, name)` → valor COMPUTADO da prop (após
/// cascade) como STRING, formato do browser. "" se ausente.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_COMPUTED_PROPERTY(h: u64, id: i64, n_ptr: *const u8, n_len: i64) -> u64 {
    let Some(node) = NodeId::from_abi(id) else { return intern("") };
    let name = unsafe { str_abi::from_abi(n_ptr, n_len) }.unwrap_or("").to_string();
    let v = with(h, |dom| dom.computed_property(node, &name)).unwrap_or_default();
    intern(&v)
}

/// `inlineProperty(domHandle, node, name)` → valor INLINE da prop (só `style=""`).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_INLINE_PROPERTY(h: u64, id: i64, n_ptr: *const u8, n_len: i64) -> u64 {
    let Some(node) = NodeId::from_abi(id) else { return intern("") };
    let name = unsafe { str_abi::from_abi(n_ptr, n_len) }.unwrap_or("").to_string();
    let v = with(h, |dom| dom.inline_property(node, &name)).unwrap_or_default();
    intern(&v)
}

/// `cssText(domHandle, node)` → o `style=""` cru.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_CSS_TEXT(h: u64, id: i64) -> u64 {
    let Some(node) = NodeId::from_abi(id) else { return intern("") };
    let v = with(h, |dom| dom.css_text(node)).unwrap_or_default();
    intern(&v)
}

/// `setCssText(domHandle, node, text)` → substitui o `style=""` inteiro.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_SET_CSS_TEXT(h: u64, id: i64, t_ptr: *const u8, t_len: i64) {
    let Some(node) = NodeId::from_abi(id) else { return };
    let text = unsafe { str_abi::from_abi(t_ptr, t_len) }.unwrap_or("").to_string();
    with_mut(h, |dom| dom.set_css_text(node, &text));
}

/// `addStylesheet(domHandle, css)` → injeta uma folha de estilo de AUTOR na página,
/// pelo MESMO caminho do `<style>` inline (acumula no stylesheet, regras posteriores
/// desempatam por cima). Usado pela camada TS de carregamento de recursos para ligar
/// CSS externo (`<link rel=stylesheet>`, `@import`) à cascade — o Rust não conhece a
/// tag `<link>` nem lê o arquivo; o TS resolve/baixa e chama isto com o CSS pronto.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_ADD_STYLESHEET(h: u64, css_ptr: *const u8, css_len: i64) {
    let css = unsafe { str_abi::from_abi(css_ptr, css_len) }.unwrap_or("").to_string();
    with_mut(h, |dom| dom.add_stylesheet(&css));
}

/// `runScript(domHandle, node, code)` → materializa o fonte de um `<script src>`
/// carregado como TEXTO do nó (acessível por `textContent`). NÃO executa: o motor
/// novo não tem eval in-process com acesso ao DOM (ver a nota em `__loadScriptAt`).
/// Carregar ≠ executar — quando o eval in-process existir, este primitivo evolui para
/// disparar a execução de fato.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_RUN_SCRIPT(h: u64, id: i64, c_ptr: *const u8, c_len: i64) {
    let Some(node) = NodeId::from_abi(id) else { return };
    let code = unsafe { str_abi::from_abi(c_ptr, c_len) }.unwrap_or("").to_string();
    with_mut(h, |dom| dom.set_text(node, &code));
}

/// `setStyleProperty(domHandle, node, name, value)` → define UMA prop no `style=""`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_SET_STYLE_PROPERTY(
    h: u64, id: i64, n_ptr: *const u8, n_len: i64, v_ptr: *const u8, v_len: i64,
) {
    let Some(node) = NodeId::from_abi(id) else { return };
    let name = unsafe { str_abi::from_abi(n_ptr, n_len) }.unwrap_or("").to_string();
    let value = unsafe { str_abi::from_abi(v_ptr, v_len) }.unwrap_or("").to_string();
    with_mut(h, |dom| dom.set_style_property(node, &name, &value));
}

/// `removeStyleProperty(domHandle, node, name)` → remove a prop do `style=""`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_REMOVE_STYLE_PROPERTY(h: u64, id: i64, n_ptr: *const u8, n_len: i64) {
    let Some(node) = NodeId::from_abi(id) else { return };
    let name = unsafe { str_abi::from_abi(n_ptr, n_len) }.unwrap_or("").to_string();
    with_mut(h, |dom| dom.remove_style_property(node, &name));
}

// ── Eventos (#1760) — polling + bubbling ─────────────────────────────────────────
// `poll_event` devolve (NodeId, tipo). Como a ABI retorna um escalar por chamada, o
// `pollEvent` avança a fila e GUARDA o tipo num thread_local; `pollEventType` lê
// esse tipo logo após. O loop TS: `n = pollEvent(); if (n>=0) { t = pollEventType(); ... }`.
thread_local! {
    static LAST_EVENT_TYPE: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
    static LAST_RAW_EVENT_TYPE: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
}

/// `addListener(domHandle, node, type)` → registra que o nó escuta o tipo.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_ADD_LISTENER(h: u64, id: i64, t_ptr: *const u8, t_len: i64) {
    let Some(node) = NodeId::from_abi(id) else { return };
    let t = unsafe { str_abi::from_abi(t_ptr, t_len) }.unwrap_or("").to_string();
    with_mut(h, |dom| dom.add_event_listener(node, &t));
}

/// `addListenerCb(domHandle, node, type, cb)` → registra o tipo E o callback
/// (word/handle i64 da Function, guardado OPACO — quem invoca é a fachada TS).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_ADD_LISTENER_CB(
    h: u64,
    id: i64,
    t_ptr: *const u8,
    t_len: i64,
    cb: i64,
) {
    let Some(node) = NodeId::from_abi(id) else { return };
    let t = unsafe { str_abi::from_abi(t_ptr, t_len) }.unwrap_or("").to_string();
    with_mut(h, |dom| dom.add_event_listener_cb(node, &t, cb));
}

/// `dispatchCollect(domHandle, target, type, bubbles)` → dispara COLETANDO os
/// callbacks (alvo → bubbling) no scratch do Dom; devolve quantos coletou. A
/// fachada TS lê com `dispatchCbAt`/`dispatchCbNode` e COPIA antes de invocar.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_DISPATCH_COLLECT(
    h: u64,
    id: i64,
    t_ptr: *const u8,
    t_len: i64,
    bubbles: i64,
) -> i64 {
    let Some(node) = NodeId::from_abi(id) else { return 0 };
    let t = unsafe { str_abi::from_abi(t_ptr, t_len) }.unwrap_or("").to_string();
    with_mut(h, |dom| dom.dispatch_event_collect(node, &t, bubbles != 0)).unwrap_or(0)
}

/// `dispatchCbAt(domHandle, i)` → o i-ésimo callback-word coletado (0 fora do range).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_DISPATCH_CB_AT(h: u64, i: i64) -> i64 {
    with(h, |dom| {
        dom.last_dispatch_at(i.max(0) as usize)
            .map(|(_, cb)| cb)
            .unwrap_or(0)
    })
    .unwrap_or(0)
}

/// `dispatchCbNode(domHandle, i)` → o NodeId do nó que escuta no i-ésimo par
/// coletado (-1 fora do range) — vira o `currentTarget` do handler na fachada.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_DISPATCH_CB_NODE(h: u64, i: i64) -> i64 {
    with(h, |dom| {
        dom.last_dispatch_at(i.max(0) as usize)
            .map(|(n, _)| n.to_abi())
            .unwrap_or(NODE_NONE)
    })
    .unwrap_or(NODE_NONE)
}

/// `removeListener(domHandle, node, type)` → para de escutar o tipo.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_REMOVE_LISTENER(h: u64, id: i64, t_ptr: *const u8, t_len: i64) {
    let Some(node) = NodeId::from_abi(id) else { return };
    let t = unsafe { str_abi::from_abi(t_ptr, t_len) }.unwrap_or("").to_string();
    with_mut(h, |dom| dom.remove_event_listener(node, &t));
}

/// `hasListener(domHandle, node, type)` → 1 se o nó escuta o tipo, 0 senão.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_HAS_LISTENER(h: u64, id: i64, t_ptr: *const u8, t_len: i64) -> i64 {
    let Some(node) = NodeId::from_abi(id) else { return 0 };
    let t = unsafe { str_abi::from_abi(t_ptr, t_len) }.unwrap_or("").to_string();
    with(h, |dom| dom.has_listener(node, &t) as i64).unwrap_or(0)
}

/// `dispatchEvent(domHandle, target, type, bubbles)` → dispara; `bubbles!=0` sobe
/// pelos ancestrais. Devolve quantos listeners foram enfileirados.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_DISPATCH_EVENT(h: u64, id: i64, t_ptr: *const u8, t_len: i64, bubbles: i64) -> i64 {
    let Some(node) = NodeId::from_abi(id) else { return 0 };
    let t = unsafe { str_abi::from_abi(t_ptr, t_len) }.unwrap_or("").to_string();
    with_mut(h, |dom| dom.dispatch_event(node, &t, bubbles != 0)).unwrap_or(0)
}

/// `pollEvent(domHandle)` → NodeId do próximo evento pendente (ou -1 se a fila está
/// vazia). GUARDA o tipo p/ `pollEventType` ler em seguida.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_POLL_EVENT(h: u64) -> i64 {
    match with_mut(h, |dom| dom.poll_event()) {
        Some(Some((node, t))) => {
            LAST_EVENT_TYPE.with(|c| *c.borrow_mut() = t);
            node.to_abi()
        }
        _ => {
            LAST_EVENT_TYPE.with(|c| c.borrow_mut().clear());
            NODE_NONE
        }
    }
}

/// `pollEventType(domHandle)` → o tipo do evento entregue no último `pollEvent` (""
/// se nenhum). Ler imediatamente após `pollEvent`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_POLL_EVENT_TYPE(_h: u64) -> u64 {
    let t = LAST_EVENT_TYPE.with(|c| c.borrow().clone());
    intern(&t)
}

/// `setHovered(domHandle, node)` → informa o nó sob o cursor (`-1` = nenhum) — o
/// estado do `:hover` vivo. O backend real chama por frame via hit-test; este
/// membro cobre testes headless e backends alternativos.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_SET_HOVERED(h: u64, id: i64) {
    with_mut(h, |dom| {
        let idx = NodeId::from_abi(id).and_then(|n| dom.resolve(n));
        dom.set_hovered(idx);
    });
}

/// `pushRawEvent(domHandle, node, type)` → empurra um evento CRU na fila do
/// backend (o mesmo caminho do hit-test do mouse) — para eventos sintéticos e
/// testes headless do ciclo completo.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_PUSH_RAW_EVENT(h: u64, id: i64, t_ptr: *const u8, t_len: i64) {
    let Some(node) = NodeId::from_abi(id) else { return };
    let t = unsafe { str_abi::from_abi(t_ptr, t_len) }.unwrap_or("").to_string();
    with_mut(h, |dom| {
        if let Some(idx) = dom.resolve(node) {
            dom.push_raw_event(idx, &t);
        }
    });
}

/// `pollRawEvent(domHandle)` → NodeId do próximo evento CRU do backend (hit-test do
/// mouse), ou -1. GUARDA o tipo p/ `pollRawEventType` ler em seguida. A fachada TS
/// (`pumpEventCallbacks`) drena e faz o dispatch completo (bubbling + callbacks).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_POLL_RAW_EVENT(h: u64) -> i64 {
    match with_mut(h, |dom| dom.poll_raw_event()) {
        Some(Some((node, t))) => {
            LAST_RAW_EVENT_TYPE.with(|c| *c.borrow_mut() = t);
            node.to_abi()
        }
        _ => {
            LAST_RAW_EVENT_TYPE.with(|c| c.borrow_mut().clear());
            NODE_NONE
        }
    }
}

/// `pollRawEventType(domHandle)` → o tipo do evento entregue no último
/// `pollRawEvent` ("" se nenhum). Ler imediatamente após.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_POLL_RAW_EVENT_TYPE(_h: u64) -> u64 {
    let t = LAST_RAW_EVENT_TYPE.with(|c| c.borrow().clone());
    intern(&t)
}

/// `advance(domHandle, nowMs)` → avança as animações para o instante `nowMs` (o LOOP
/// INTERNO ao DOM; #1776). Devolve 1 se há animação ATIVA (o backend deve repintar o
/// próximo frame), 0 se tudo estático. O egui só chama isto passando o tempo do frame.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_ADVANCE(h: u64, now_ms: f64) -> i64 {
    with_mut(h, |dom| dom.advance(now_ms as f32) as i64).unwrap_or(0)
}

/// `getAttribute(domHandle, node, name)` → valor do atributo como STRING (handle
/// do pool GC). Atributo ausente / nó inválido ⇒ `""` (a fachada TS converte ""
/// para `null` se quiser semântica de browser).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_GET_ATTRIBUTE(
    h: u64,
    id: i64,
    name_ptr: *const u8,
    name_len: i64,
) -> u64 {
    let Some(node) = NodeId::from_abi(id) else { return intern("") };
    let name = unsafe { str_abi::from_abi(name_ptr, name_len) }.unwrap_or("");
    let val = with(h, |dom| dom.get_attr(node, name).unwrap_or("").to_string())
        .unwrap_or_default();
    intern(&val)
}

/// `tagName(domHandle, node)` → nome da tag em minúsculas como STRING (handle do
/// pool GC). Nó inválido / não-elemento (Document/Text) ⇒ `""`. (A fachada TS faz
/// o upper-case que o browser devolve.)
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_TAG_NAME(h: u64, id: i64) -> u64 {
    let Some(node) = NodeId::from_abi(id) else { return intern("") };
    let tag = with(h, |dom| dom.tag_name(node).unwrap_or("").to_string()).unwrap_or_default();
    intern(&tag)
}

// ── Coleções de nós: par count + at (evita array-return do Rust) ─────────────────
// `query_all`/`child_elements` retornam `Vec<NodeId>`. Devolver um ARRAY pela ABI
// exigiria materializar um `Entry::Vec` do pool GC + a convenção de array-de-i64,
// acoplando o crate a detalhes do runtime de coleções. ESCOLHA: expor o par
// `…Count(…) -> i64` + `…At(…, index) -> i64` (NodeId), e a fachada TS monta o
// array iterando `for (i=0; i<count; i++) at(i)`. Simples, sem dep nova, e o custo
// (re-rodar a query por elemento) é aceitável p/ os tamanhos de DOM em jogo; se
// virar gargalo, um snapshot por handle entra depois. `-1` = índice fora / inválido.

/// `querySelectorAllCount(domHandle, selector)` → quantos nós casam o seletor
/// simples (`tag`/`#id`/`.class`), em ordem de documento. `0` se nada casa ou o
/// handle é inválido.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_QUERY_SELECTOR_ALL_COUNT(
    h: u64,
    sel_ptr: *const u8,
    sel_len: i64,
) -> i64 {
    let sel = unsafe { str_abi::from_abi(sel_ptr, sel_len) }.unwrap_or("");
    with(h, |dom| dom.query_all(sel).len() as i64).unwrap_or(0)
}

/// `querySelectorAllAt(domHandle, selector, index)` → o `NodeId` do `index`-ésimo
/// nó que casa o seletor (ordem de documento), ou `-1` se fora do intervalo /
/// handle inválido. Pareie com `querySelectorAllCount` para iterar.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_QUERY_SELECTOR_ALL_AT(
    h: u64,
    sel_ptr: *const u8,
    sel_len: i64,
    index: i64,
) -> i64 {
    if index < 0 {
        return NODE_NONE;
    }
    let sel = unsafe { str_abi::from_abi(sel_ptr, sel_len) }.unwrap_or("");
    with(h, |dom| {
        dom.query_all(sel).get(index as usize).map(|id| id.to_abi()).unwrap_or(NODE_NONE)
    })
    .unwrap_or(NODE_NONE)
}

/// `childCount(domHandle, node)` → quantos filhos ELEMENTO o nó tem (exclui nós de
/// texto, como `element.children`). `0` se o nó é inválido.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_CHILD_COUNT(h: u64, id: i64) -> i64 {
    let Some(node) = NodeId::from_abi(id) else { return 0 };
    with(h, |dom| dom.child_elements(node).len() as i64).unwrap_or(0)
}

/// `childAt(domHandle, node, index)` → o `NodeId` do `index`-ésimo filho ELEMENTO,
/// ou `-1` se fora do intervalo / nó inválido. Pareie com `childCount`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_CHILD_AT(h: u64, id: i64, index: i64) -> i64 {
    if index < 0 {
        return NODE_NONE;
    }
    let Some(node) = NodeId::from_abi(id) else { return NODE_NONE };
    with(h, |dom| {
        dom.child_elements(node).get(index as usize).map(|c| c.to_abi()).unwrap_or(NODE_NONE)
    })
    .unwrap_or(NODE_NONE)
}

/// `nodeStyleSlot(domHandle, node, slot)` → valor (`i64`) do SLOT de estilo do nó,
/// com a cascade COMPLETA resolvida (defineStyle < `<style>` autor < `style=""`
/// inline < override por-nó), ou `-1` se não-setado/inválido.
/// É como o LAYOUT (em TS) lê o estilo computado de cada nó. Slots: 0=color 1=bg
/// 2=font_size 3=padding 4=margin 5=border_width 6=border_color 7=corner_radius.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_NODE_STYLE_SLOT(h: u64, id: i64, slot: i64) -> i64 {
    let Some(node) = NodeId::from_abi(id) else { return -1 };
    with(h, |dom| dom.computed_style(node).map(|s| s.slot_value(slot)).unwrap_or(-1)).unwrap_or(-1)
}

/// `displayOf(domHandle, node)` → o código de `display` do nó (0=vertical 1=wrap
/// 2=horizontal 3=grid), ou `-1` se a tag não é bloco (inline/desconhecida). O
/// LAYOUT (em TS) usa isso para decidir o eixo de empilhamento.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_DISPLAY_OF(h: u64, id: i64) -> i64 {
    let Some(node) = NodeId::from_abi(id) else { return -1 };
    with(h, |dom| dom.display_of(node)).unwrap_or(-1)
}

// ── defineStyle / defineBlock / defineInline (estado de estilo/layout no rts-dom) ─
// Hoje há cópias destes na ABI do `rts-egui`; o ESTADO (style::STYLES,
// block::BLOCKS/INLINES) migrou pra cá, então a fachada DOM headless também os
// expõe. ADITIVO (não remove os do egui neste passo). Tag vazia ⇒ no-op.

/// `defineStyle(tag, slot, val)` — registra UM slot de estilo OPACO (invariante 4)
/// de uma TAG: 0=color 1=bg 2=font_size 3=padding 4=margin 5=border_width
/// 6=border_color 7=corner_radius (cores como `u32` `0xRRGGBBAA`). O TS mapeia
/// nome-CSS→slot; o Rust nunca casa string CSS. Acumula por tag.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_DEFINE_STYLE(
    tag_ptr: *const u8,
    tag_len: i64,
    slot: i64,
    val: i64,
) {
    let tag = unsafe { str_abi::from_abi(tag_ptr, tag_len) }.unwrap_or("");
    if tag.is_empty() {
        return;
    }
    crate::style::define_style(tag, slot, val);
}

/// `setStyle(domHandle, node, slot, val)` — aplica UM slot de estilo OPACO a UM
/// NÓ (override por-nó, vence tag e `style=""` inline). Mesmos slots do
/// `defineStyle`. Para muitos nós/props use `setStyleBatch` (invariante 6).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_SET_STYLE(h: u64, id: i64, slot: i64, val: i64) {
    let Some(node) = NodeId::from_abi(id) else { return };
    with_mut(h, |dom| dom.set_node_style_slot(node, slot, val));
}

/// `setStyleBatch(domHandle, bufferHandle, count)` — aplica `count` triplas
/// `(nodeId, slot, val)` de uma vez (invariante 6: estilizar N nós por frame não
/// pode ser N×5 FFIs). O buffer é um `Entry::Buffer` (do namespace `buffer`) com
/// `count*3` inteiros i64 LITTLE-ENDIAN consecutivos (`[id0,slot0,val0, id1,…]`).
/// Lê via a HandleTable do engine (sem dep de `rts-shared` — camada). Triplas com
/// id inválido são ignoradas.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_SET_STYLE_BATCH(h: u64, buffer: u64, count: i64) {
    if count <= 0 {
        return;
    }
    let want = (count as usize) * 3; // i64s esperados
    // Lê o buffer GC como i64 little-endian (8 bytes cada), sem copiar além do
    // necessário. `with_entry` empresta o `Vec<u8>` do `Entry::Buffer`.
    let triples: Option<Vec<i64>> = rts_engine::heap::handles::with_entry(buffer, |e| {
        let bytes = match e {
            Some(rts_engine::heap::handles::Entry::Buffer(b)) => b,
            _ => return None,
        };
        let n = want.min(bytes.len() / 8); // não lê além do buffer
        let mut out = Vec::with_capacity(n);
        for k in 0..n {
            let mut le = [0u8; 8];
            le.copy_from_slice(&bytes[k * 8..k * 8 + 8]);
            out.push(i64::from_le_bytes(le));
        }
        Some(out)
    });
    if let Some(triples) = triples {
        with_mut(h, |dom| dom.apply_style_batch(&triples));
    }
}

/// `defineBlock(tag, display, indent, prefix, flags)` — registra como uma TAG faz
/// layout. `display` 0=vertical 1=wrap 2=horizontal 3=grid; `indent` recuo em
/// pontos (ou tamanho de fonte quando `flags` tem HEADING); `prefix` 0=none
/// 1=bullet 2=number; `flags` bitmask MONO=1|PRESERVE_WS=2|HEADING=4|BOLD=8|
/// ITALIC=16. Nenhum nome de tag é hardcodado no Rust.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_DEFINE_BLOCK(
    tag_ptr: *const u8,
    tag_len: i64,
    display: i64,
    indent: f64,
    prefix: i64,
    flags: i64,
) {
    let tag = unsafe { str_abi::from_abi(tag_ptr, tag_len) }.unwrap_or("");
    if tag.is_empty() {
        return;
    }
    crate::block::define(
        tag,
        crate::block::BlockDef { display, indent: indent as f32, prefix, flags },
    );
}

/// `defineInline(tag, flags)` — registra o estilo INLINE de uma tag (`<b>`/`<i>`/
/// `<code>`…): `flags` bitmask BOLD=8|ITALIC=16|MONO=1. Uma tag inline só liga os
/// bits de estilo e desce nos filhos (transparente). Nenhum nome de tag no Rust.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_DOM_DEFINE_INLINE(tag_ptr: *const u8, tag_len: i64, flags: i64) {
    let tag = unsafe { str_abi::from_abi(tag_ptr, tag_len) }.unwrap_or("");
    if tag.is_empty() {
        return;
    }
    crate::block::define_inline(tag, flags);
}

// ── Registro no Engine (via tabela Registration; o engine não nomeia "dom") ──────

/// Helper de declaração de membro (mesmo shape do `rts-egui::func` / `io::func`).
fn func(name: &str, symbol: &str, sig: Sig, ts: &str, doc: &str, fp: *const u8) -> Member {
    Member {
        name: name.to_string(),
        kind: MemberKind::Function,
        sig,
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: doc.to_string(),
        ret_class: None,
        pure: false,
        emit: None,
    }
}

/// Monta o namespace `dom` no Engine (chamado pela tabela `Registration` do
/// codegen). HEADLESS: nenhum membro recebe handle de janela.
pub fn register(e: &mut Engine) {
    e.ns("dom")
        .doc("Headless retained DOM: parse HTML, query (tag/#id/.class), mutate. No window/render.")
        .member(func(
            "parseHtml",
            "__RTS_FN_NS_DOM_PARSE_HTML",
            Sig::new(vec![StrPtr], Handle),
            "parseHtml(html: string): number",
            "Parses HTML into a fresh retained DOM; returns its handle (>= 1).",
            __RTS_FN_NS_DOM_PARSE_HTML as *const u8,
        ))
        .member(func(
            "createDocument",
            "__RTS_FN_NS_DOM_CREATE_DOCUMENT",
            Sig::new(vec![], Handle),
            "createDocument(): number",
            "Empty DOM (just #document) to build via createElement/appendChild; returns its handle.",
            __RTS_FN_NS_DOM_CREATE_DOCUMENT as *const u8,
        ))
        .member(func(
            "free",
            "__RTS_FN_NS_DOM_FREE",
            Sig::new(vec![Handle], AbiType::Void),
            "free(dom: number): void",
            "Frees a standalone DOM (its handle becomes invalid).",
            __RTS_FN_NS_DOM_FREE as *const u8,
        ))
        .member(func(
            "querySelector",
            "__RTS_FN_NS_DOM_QUERY_SELECTOR",
            Sig::new(vec![Handle, StrPtr], I64),
            "querySelector(dom: number, selector: string): number",
            "First node matching a simple selector (tag / #id / .class); NodeId (>= 0) or -1. Extract to a const before comparing.",
            __RTS_FN_NS_DOM_QUERY_SELECTOR as *const u8,
        ))
        .member(func(
            "setText",
            "__RTS_FN_NS_DOM_SET_TEXT",
            Sig::new(vec![Handle, I64, StrPtr], AbiType::Void),
            "setText(dom: number, node: number, text: string): void",
            "Replaces a node's content with a single text node (element.textContent = text).",
            __RTS_FN_NS_DOM_SET_TEXT as *const u8,
        ))
        .member(func(
            "setAttr",
            "__RTS_FN_NS_DOM_SET_ATTR",
            Sig::new(vec![Handle, I64, StrPtr, StrPtr], AbiType::Void),
            "setAttr(dom: number, node: number, name: string, value: string): void",
            "Sets/updates an attribute on a node (element.setAttribute).",
            __RTS_FN_NS_DOM_SET_ATTR as *const u8,
        ))
        .member(func(
            "createElement",
            "__RTS_FN_NS_DOM_CREATE_ELEMENT",
            Sig::new(vec![Handle, StrPtr], I64),
            "createElement(dom: number, tag: string): number",
            "Creates a detached element; returns its NodeId >= 0, or -1 if the DOM handle is invalid.",
            __RTS_FN_NS_DOM_CREATE_ELEMENT as *const u8,
        ))
        .member(func(
            "appendChild",
            "__RTS_FN_NS_DOM_APPEND_CHILD",
            Sig::new(vec![Handle, I64, I64], AbiType::Void),
            "appendChild(dom: number, parent: number, child: number): void",
            "Moves child to the end of parent's children (parent.appendChild).",
            __RTS_FN_NS_DOM_APPEND_CHILD as *const u8,
        ))
        .member(func(
            "removeNode",
            "__RTS_FN_NS_DOM_REMOVE_NODE",
            Sig::new(vec![Handle, I64], AbiType::Void),
            "removeNode(dom: number, node: number): void",
            "Detaches a node from its parent (element.remove).",
            __RTS_FN_NS_DOM_REMOVE_NODE as *const u8,
        ))
        .member(func(
            "createTextNode",
            "__RTS_FN_NS_DOM_CREATE_TEXT_NODE",
            Sig::new(vec![Handle, StrPtr], I64),
            "createTextNode(dom: number, text: string): number",
            "Creates a detached Text node, returns its NodeId (document.createTextNode).",
            __RTS_FN_NS_DOM_CREATE_TEXT_NODE as *const u8,
        ))
        .member(func(
            "insertBefore",
            "__RTS_FN_NS_DOM_INSERT_BEFORE",
            Sig::new(vec![Handle, I64, I64, I64], AbiType::Void),
            "insertBefore(dom: number, parent: number, child: number, reference: number): void",
            "Inserts child before reference in parent's children; reference -1 = append (parent.insertBefore).",
            __RTS_FN_NS_DOM_INSERT_BEFORE as *const u8,
        ))
        // ── Navegação (parentNode / first|lastChild / next|previousSibling) ──────
        .member(func(
            "parentNode",
            "__RTS_FN_NS_DOM_PARENT_NODE",
            Sig::new(vec![Handle, I64], I64),
            "parentNode(dom: number, node: number): number",
            "NodeId of the parent, or -1 for the root / invalid. Extract to a const before comparing.",
            __RTS_FN_NS_DOM_PARENT_NODE as *const u8,
        ))
        .member(func(
            "firstChild",
            "__RTS_FN_NS_DOM_FIRST_CHILD",
            Sig::new(vec![Handle, I64], I64),
            "firstChild(dom: number, node: number): number",
            "NodeId of the first child (any type, incl. Text), or -1.",
            __RTS_FN_NS_DOM_FIRST_CHILD as *const u8,
        ))
        .member(func(
            "lastChild",
            "__RTS_FN_NS_DOM_LAST_CHILD",
            Sig::new(vec![Handle, I64], I64),
            "lastChild(dom: number, node: number): number",
            "NodeId of the last child, or -1.",
            __RTS_FN_NS_DOM_LAST_CHILD as *const u8,
        ))
        .member(func(
            "nextSibling",
            "__RTS_FN_NS_DOM_NEXT_SIBLING",
            Sig::new(vec![Handle, I64], I64),
            "nextSibling(dom: number, node: number): number",
            "NodeId of the next sibling, or -1 if last.",
            __RTS_FN_NS_DOM_NEXT_SIBLING as *const u8,
        ))
        .member(func(
            "previousSibling",
            "__RTS_FN_NS_DOM_PREVIOUS_SIBLING",
            Sig::new(vec![Handle, I64], I64),
            "previousSibling(dom: number, node: number): number",
            "NodeId of the previous sibling, or -1 if first.",
            __RTS_FN_NS_DOM_PREVIOUS_SIBLING as *const u8,
        ))
        .member(func(
            "childNodesCount",
            "__RTS_FN_NS_DOM_CHILD_NODES_COUNT",
            Sig::new(vec![Handle, I64], I64),
            "childNodesCount(dom: number, node: number): number",
            "Total child count (incl. Text nodes) — pair with childNodeAt (node.childNodes.length).",
            __RTS_FN_NS_DOM_CHILD_NODES_COUNT as *const u8,
        ))
        .member(func(
            "childNodeAt",
            "__RTS_FN_NS_DOM_CHILD_NODE_AT",
            Sig::new(vec![Handle, I64, I64], I64),
            "childNodeAt(dom: number, node: number, index: number): number",
            "The index-th child (incl. Text), or -1 (node.childNodes[index]).",
            __RTS_FN_NS_DOM_CHILD_NODE_AT as *const u8,
        ))
        .member(func(
            "nodeType",
            "__RTS_FN_NS_DOM_NODE_TYPE",
            Sig::new(vec![Handle, I64], I64),
            "nodeType(dom: number, node: number): number",
            "DOM nodeType code: Element=1, Text=3, Comment=8, Document=9; -1 if invalid.",
            __RTS_FN_NS_DOM_NODE_TYPE as *const u8,
        ))
        .member(func(
            "nodeName",
            "__RTS_FN_NS_DOM_NODE_NAME",
            // `AbiType::Handle` LITERAL (não o alias U64) + ts `: string` → o motor
            // reboxa como TAG_STR (string usável no TS); o alias U64 reboxaria como
            // inteiro cru (bug "dados de ponteiro").
            Sig::new(vec![Handle, I64], AbiType::Handle),
            "nodeName(dom: number, node: number): string",
            "DOM nodeName: tag for Element; #text/#comment/#document otherwise.",
            __RTS_FN_NS_DOM_NODE_NAME as *const u8,
        ))
        .member(func(
            "rootId",
            "__RTS_FN_NS_DOM_ROOT_ID",
            Sig::new(vec![Handle], I64),
            "rootId(dom: number): number",
            "The versioned NodeId of the #document root (>= 0), or -1 if invalid.",
            __RTS_FN_NS_DOM_ROOT_ID as *const u8,
        ))
        .member(func(
            "dump",
            "__RTS_FN_NS_DOM_DUMP",
            Sig::new(vec![Handle], AbiType::Void),
            "dump(dom: number): void",
            "Prints the retained DOM tree to stderr, devtools-style (debug).",
            __RTS_FN_NS_DOM_DUMP as *const u8,
        ))
        .member(func(
            "dumpLayout",
            "__RTS_FN_NS_DOM_DUMP_LAYOUT",
            Sig::new(vec![Handle, I64], AbiType::Void),
            "dumpLayout(dom: number, viewportW: number): void",
            "Computes the layout DisplayList at the given viewport width and prints it as JSON (x/y/w/h + colors), to compare with the browser render.",
            __RTS_FN_NS_DOM_DUMP_LAYOUT as *const u8,
        ))
        .member(func(
            "dumpTree",
            "__RTS_FN_NS_DOM_DUMP_TREE",
            Sig::new(vec![Handle, I64], AbiType::Void),
            "dumpTree(dom: number, viewportW: number): void",
            "Prints the element tree with each node's rect (tag/id/class + x/y/w/h), indented, to compare geometry node-by-node with the browser's getBoundingClientRect.",
            __RTS_FN_NS_DOM_DUMP_TREE as *const u8,
        ))
        .member(func(
            "boundingComponent",
            "__RTS_FN_NS_DOM_BOUNDING_COMPONENT",
            Sig::new(vec![Handle, I64, I64, I64], I64),
            "boundingComponent(dom: number, node: number, viewportW: number, which: number): number",
            "One component of a node's border-box (which: 0=x 1=y 2=width 3=height) in points×1000, or -1 if the node has no box. Basis of element.getBoundingClientRect(); the facade divides by 1000.",
            __RTS_FN_NS_DOM_BOUNDING_COMPONENT as *const u8,
        ))
        // ── Leitura de conteúdo (retorna STRING: handle do pool GC) ──────────────
        // Retorno `Handle` + ts_signature `: string` = string dinâmica (mesmo
        // contrato de string.trim/to_upper; o motor reconhece pelo `: string`).
        .member(func(
            "getText",
            "__RTS_FN_NS_DOM_GET_TEXT",
            // Retorno `AbiType::Handle` (NÃO o alias `U64` de `dom`): só `Handle` +
            // `: string` faz o motor reboxar como `TAG_STR` (string usável no TS).
            // `U64` reboxaria como inteiro cru (era o bug: `Number.toUpperCase`).
            Sig::new(vec![Handle, I64], AbiType::Handle),
            "getText(dom: number, node: number): string",
            "node.textContent: concatenated text of all descendants. Empty string if the node is invalid.",
            __RTS_FN_NS_DOM_GET_TEXT as *const u8,
        ))
        .member(func(
            "innerHtml",
            "__RTS_FN_NS_DOM_INNER_HTML",
            Sig::new(vec![Handle, I64], AbiType::Handle),
            "innerHtml(dom: number, node: number): string",
            "element.innerHTML (get): serialized HTML of the node's children. Empty string if invalid.",
            __RTS_FN_NS_DOM_INNER_HTML as *const u8,
        ))
        .member(func(
            "outerHtml",
            "__RTS_FN_NS_DOM_OUTER_HTML",
            Sig::new(vec![Handle, I64], AbiType::Handle),
            "outerHtml(dom: number, node: number): string",
            "element.outerHTML (get): serialized HTML including the element itself.",
            __RTS_FN_NS_DOM_OUTER_HTML as *const u8,
        ))
        .member(func(
            "setInnerHtml",
            "__RTS_FN_NS_DOM_SET_INNER_HTML",
            Sig::new(vec![Handle, I64, StrPtr], AbiType::Void),
            "setInnerHtml(dom: number, node: number, html: string): void",
            "element.innerHTML = html (set): parses the HTML and replaces the node's children.",
            __RTS_FN_NS_DOM_SET_INNER_HTML as *const u8,
        ))
        // ── Traversal por elemento — #1757 ──────────────────────────────────────
        .member(func(
            "firstElementChild", "__RTS_FN_NS_DOM_FIRST_ELEMENT_CHILD",
            Sig::new(vec![Handle, I64], I64),
            "firstElementChild(dom: number, node: number): number",
            "element.firstElementChild: first child that is an Element (-1 if none).",
            __RTS_FN_NS_DOM_FIRST_ELEMENT_CHILD as *const u8,
        ))
        .member(func(
            "lastElementChild", "__RTS_FN_NS_DOM_LAST_ELEMENT_CHILD",
            Sig::new(vec![Handle, I64], I64),
            "lastElementChild(dom: number, node: number): number",
            "element.lastElementChild: last child Element (-1 if none).",
            __RTS_FN_NS_DOM_LAST_ELEMENT_CHILD as *const u8,
        ))
        .member(func(
            "nextElementSibling", "__RTS_FN_NS_DOM_NEXT_ELEMENT_SIBLING",
            Sig::new(vec![Handle, I64], I64),
            "nextElementSibling(dom: number, node: number): number",
            "element.nextElementSibling: next sibling Element, skipping text (-1 if none).",
            __RTS_FN_NS_DOM_NEXT_ELEMENT_SIBLING as *const u8,
        ))
        .member(func(
            "previousElementSibling", "__RTS_FN_NS_DOM_PREVIOUS_ELEMENT_SIBLING",
            Sig::new(vec![Handle, I64], I64),
            "previousElementSibling(dom: number, node: number): number",
            "element.previousElementSibling: previous sibling Element (-1 if none).",
            __RTS_FN_NS_DOM_PREVIOUS_ELEMENT_SIBLING as *const u8,
        ))
        .member(func(
            "parentElement", "__RTS_FN_NS_DOM_PARENT_ELEMENT",
            Sig::new(vec![Handle, I64], I64),
            "parentElement(dom: number, node: number): number",
            "node.parentElement: the parent if it is an Element (not the Document) (-1 otherwise).",
            __RTS_FN_NS_DOM_PARENT_ELEMENT as *const u8,
        ))
        .member(func(
            "closest", "__RTS_FN_NS_DOM_CLOSEST",
            Sig::new(vec![Handle, I64, StrPtr], I64),
            "closest(dom: number, node: number, selector: string): number",
            "element.closest(sel): nearest ancestor (incl. self) matching the simple selector (-1 if none).",
            __RTS_FN_NS_DOM_CLOSEST as *const u8,
        ))
        .member(func(
            "matches", "__RTS_FN_NS_DOM_MATCHES",
            Sig::new(vec![Handle, I64, StrPtr], I64),
            "matches(dom: number, node: number, selector: string): number",
            "element.matches(sel): 1 if the node matches the simple selector, 0 otherwise.",
            __RTS_FN_NS_DOM_MATCHES as *const u8,
        ))
        // ── Node utils — #1762 ──────────────────────────────────────────────────
        .member(func(
            "contains", "__RTS_FN_NS_DOM_CONTAINS",
            Sig::new(vec![Handle, I64, I64], I64),
            "contains(dom: number, node: number, other: number): number",
            "node.contains(other): 1 if other is node or a descendant, 0 otherwise.",
            __RTS_FN_NS_DOM_CONTAINS as *const u8,
        ))
        .member(func(
            "hasChildNodes", "__RTS_FN_NS_DOM_HAS_CHILD_NODES",
            Sig::new(vec![Handle, I64], I64),
            "hasChildNodes(dom: number, node: number): number",
            "node.hasChildNodes(): 1 if it has any child, 0 otherwise.",
            __RTS_FN_NS_DOM_HAS_CHILD_NODES as *const u8,
        ))
        .member(func(
            "nodeValue", "__RTS_FN_NS_DOM_NODE_VALUE",
            Sig::new(vec![Handle, I64], AbiType::Handle),
            "nodeValue(dom: number, node: number): string",
            "node.nodeValue: raw text of a Text/Comment node ('' for Element/Document).",
            __RTS_FN_NS_DOM_NODE_VALUE as *const u8,
        ))
        .member(func(
            "setNodeValue", "__RTS_FN_NS_DOM_SET_NODE_VALUE",
            Sig::new(vec![Handle, I64, StrPtr], AbiType::Void),
            "setNodeValue(dom: number, node: number, value: string): void",
            "node.nodeValue = value: replaces the text of a Text/Comment node.",
            __RTS_FN_NS_DOM_SET_NODE_VALUE as *const u8,
        ))
        .member(func(
            "createComment", "__RTS_FN_NS_DOM_CREATE_COMMENT",
            Sig::new(vec![Handle, StrPtr], I64),
            "createComment(dom: number, text: string): number",
            "document.createComment(text): a detached comment node.",
            __RTS_FN_NS_DOM_CREATE_COMMENT as *const u8,
        ))
        .member(func(
            "normalize", "__RTS_FN_NS_DOM_NORMALIZE",
            Sig::new(vec![Handle, I64], AbiType::Void),
            "normalize(dom: number, node: number): void",
            "node.normalize(): merge adjacent text nodes and drop empty ones, recursively.",
            __RTS_FN_NS_DOM_NORMALIZE as *const u8,
        ))
        // ── Atributos extra — #1761 ─────────────────────────────────────────────
        .member(func(
            "removeAttr", "__RTS_FN_NS_DOM_REMOVE_ATTR",
            Sig::new(vec![Handle, I64, StrPtr], AbiType::Void),
            "removeAttr(dom: number, node: number, name: string): void",
            "element.removeAttribute(name).",
            __RTS_FN_NS_DOM_REMOVE_ATTR as *const u8,
        ))
        .member(func(
            "hasAttr", "__RTS_FN_NS_DOM_HAS_ATTR",
            Sig::new(vec![Handle, I64, StrPtr], I64),
            "hasAttr(dom: number, node: number, name: string): number",
            "element.hasAttribute(name): 1 if present (even empty value), 0 otherwise.",
            __RTS_FN_NS_DOM_HAS_ATTR as *const u8,
        ))
        .member(func(
            "attrCount", "__RTS_FN_NS_DOM_ATTR_COUNT",
            Sig::new(vec![Handle, I64], I64),
            "attrCount(dom: number, node: number): number",
            "number of attributes (for getAttributeNames/attributes).",
            __RTS_FN_NS_DOM_ATTR_COUNT as *const u8,
        ))
        .member(func(
            "attrNameAt", "__RTS_FN_NS_DOM_ATTR_NAME_AT",
            Sig::new(vec![Handle, I64, I64], AbiType::Handle),
            "attrNameAt(dom: number, node: number, i: number): string",
            "name of the i-th attribute.",
            __RTS_FN_NS_DOM_ATTR_NAME_AT as *const u8,
        ))
        .member(func(
            "attrValueAt", "__RTS_FN_NS_DOM_ATTR_VALUE_AT",
            Sig::new(vec![Handle, I64, I64], AbiType::Handle),
            "attrValueAt(dom: number, node: number, i: number): string",
            "value of the i-th attribute.",
            __RTS_FN_NS_DOM_ATTR_VALUE_AT as *const u8,
        ))
        // ── Query extra — #1758 ─────────────────────────────────────────────────
        .member(func(
            "getByClassCount", "__RTS_FN_NS_DOM_GET_BY_CLASS_COUNT",
            Sig::new(vec![Handle, StrPtr], I64),
            "getByClassCount(dom: number, name: string): number",
            "count of getElementsByClassName.",
            __RTS_FN_NS_DOM_GET_BY_CLASS_COUNT as *const u8,
        ))
        .member(func(
            "getByClassAt", "__RTS_FN_NS_DOM_GET_BY_CLASS_AT",
            Sig::new(vec![Handle, StrPtr, I64], I64),
            "getByClassAt(dom: number, name: string, i: number): number",
            "i-th element of getElementsByClassName.",
            __RTS_FN_NS_DOM_GET_BY_CLASS_AT as *const u8,
        ))
        .member(func(
            "getByTagCount", "__RTS_FN_NS_DOM_GET_BY_TAG_COUNT",
            Sig::new(vec![Handle, StrPtr], I64),
            "getByTagCount(dom: number, tag: string): number",
            "count of getElementsByTagName ('*' = all).",
            __RTS_FN_NS_DOM_GET_BY_TAG_COUNT as *const u8,
        ))
        .member(func(
            "getByTagAt", "__RTS_FN_NS_DOM_GET_BY_TAG_AT",
            Sig::new(vec![Handle, StrPtr, I64], I64),
            "getByTagAt(dom: number, tag: string, i: number): number",
            "i-th element of getElementsByTagName.",
            __RTS_FN_NS_DOM_GET_BY_TAG_AT as *const u8,
        ))
        .member(func(
            "getByNameCount", "__RTS_FN_NS_DOM_GET_BY_NAME_COUNT",
            Sig::new(vec![Handle, StrPtr], I64),
            "getByNameCount(dom: number, name: string): number",
            "count of getElementsByName.",
            __RTS_FN_NS_DOM_GET_BY_NAME_COUNT as *const u8,
        ))
        .member(func(
            "getByNameAt", "__RTS_FN_NS_DOM_GET_BY_NAME_AT",
            Sig::new(vec![Handle, StrPtr, I64], I64),
            "getByNameAt(dom: number, name: string, i: number): number",
            "i-th element of getElementsByName.",
            __RTS_FN_NS_DOM_GET_BY_NAME_AT as *const u8,
        ))
        .member(func(
            "queryWithin", "__RTS_FN_NS_DOM_QUERY_WITHIN",
            Sig::new(vec![Handle, I64, StrPtr], I64),
            "queryWithin(dom: number, root: number, selector: string): number",
            "element.querySelector restricted to the subtree (-1 if none).",
            __RTS_FN_NS_DOM_QUERY_WITHIN as *const u8,
        ))
        .member(func(
            "queryAllWithinCount", "__RTS_FN_NS_DOM_QUERY_ALL_WITHIN_COUNT",
            Sig::new(vec![Handle, I64, StrPtr], I64),
            "queryAllWithinCount(dom: number, root: number, selector: string): number",
            "count of element.querySelectorAll in the subtree.",
            __RTS_FN_NS_DOM_QUERY_ALL_WITHIN_COUNT as *const u8,
        ))
        .member(func(
            "queryAllWithinAt", "__RTS_FN_NS_DOM_QUERY_ALL_WITHIN_AT",
            Sig::new(vec![Handle, I64, StrPtr, I64], I64),
            "queryAllWithinAt(dom: number, root: number, selector: string, i: number): number",
            "i-th element of element.querySelectorAll in the subtree.",
            __RTS_FN_NS_DOM_QUERY_ALL_WITHIN_AT as *const u8,
        ))
        // ── Mutação rica — #1756 ────────────────────────────────────────────────
        .member(func(
            "cloneNode", "__RTS_FN_NS_DOM_CLONE_NODE",
            Sig::new(vec![Handle, I64, I64], I64),
            "cloneNode(dom: number, node: number, deep: number): number",
            "node.cloneNode(deep): detached clone (deep!=0 = with children).",
            __RTS_FN_NS_DOM_CLONE_NODE as *const u8,
        ))
        .member(func(
            "prepend", "__RTS_FN_NS_DOM_PREPEND",
            Sig::new(vec![Handle, I64, I64], AbiType::Void),
            "prepend(dom: number, parent: number, child: number): void",
            "parent.prepend(child): insert at the start.",
            __RTS_FN_NS_DOM_PREPEND as *const u8,
        ))
        .member(func(
            "insertAdjacent", "__RTS_FN_NS_DOM_INSERT_ADJACENT",
            Sig::new(vec![Handle, I64, I64, I64], AbiType::Void),
            "insertAdjacent(dom: number, node: number, other: number, after: number): void",
            "node.before(other)/after(other): insert as sibling (after!=0 = after).",
            __RTS_FN_NS_DOM_INSERT_ADJACENT as *const u8,
        ))
        .member(func(
            "replaceWith", "__RTS_FN_NS_DOM_REPLACE_WITH",
            Sig::new(vec![Handle, I64, I64], AbiType::Void),
            "replaceWith(dom: number, node: number, other: number): void",
            "node.replaceWith(other).",
            __RTS_FN_NS_DOM_REPLACE_WITH as *const u8,
        ))
        .member(func(
            "replaceChild", "__RTS_FN_NS_DOM_REPLACE_CHILD",
            Sig::new(vec![Handle, I64, I64, I64], AbiType::Void),
            "replaceChild(dom: number, parent: number, newChild: number, oldChild: number): void",
            "parent.replaceChild(new, old).",
            __RTS_FN_NS_DOM_REPLACE_CHILD as *const u8,
        ))
        .member(func(
            "removeChild", "__RTS_FN_NS_DOM_REMOVE_CHILD",
            Sig::new(vec![Handle, I64, I64], AbiType::Void),
            "removeChild(dom: number, parent: number, child: number): void",
            "parent.removeChild(child).",
            __RTS_FN_NS_DOM_REMOVE_CHILD as *const u8,
        ))
        .member(func(
            "clearChildren", "__RTS_FN_NS_DOM_CLEAR_CHILDREN",
            Sig::new(vec![Handle, I64], AbiType::Void),
            "clearChildren(dom: number, parent: number): void",
            "parent.replaceChildren() with no args: remove all children.",
            __RTS_FN_NS_DOM_CLEAR_CHILDREN as *const u8,
        ))
        // ── element.style + getComputedStyle — #1759 ────────────────────────────
        .member(func(
            "computedProperty", "__RTS_FN_NS_DOM_COMPUTED_PROPERTY",
            Sig::new(vec![Handle, I64, StrPtr], AbiType::Handle),
            "computedProperty(dom: number, node: number, name: string): string",
            "getComputedStyle(el).<name>: computed value after the cascade.",
            __RTS_FN_NS_DOM_COMPUTED_PROPERTY as *const u8,
        ))
        .member(func(
            "inlineProperty", "__RTS_FN_NS_DOM_INLINE_PROPERTY",
            Sig::new(vec![Handle, I64, StrPtr], AbiType::Handle),
            "inlineProperty(dom: number, node: number, name: string): string",
            "el.style.getPropertyValue(name): inline value from style='' only.",
            __RTS_FN_NS_DOM_INLINE_PROPERTY as *const u8,
        ))
        .member(func(
            "cssText", "__RTS_FN_NS_DOM_CSS_TEXT",
            Sig::new(vec![Handle, I64], AbiType::Handle),
            "cssText(dom: number, node: number): string",
            "el.style.cssText (get): the raw style='' string.",
            __RTS_FN_NS_DOM_CSS_TEXT as *const u8,
        ))
        .member(func(
            "setCssText", "__RTS_FN_NS_DOM_SET_CSS_TEXT",
            Sig::new(vec![Handle, I64, StrPtr], AbiType::Void),
            "setCssText(dom: number, node: number, text: string): void",
            "el.style.cssText = text (set): replace the whole style='' string.",
            __RTS_FN_NS_DOM_SET_CSS_TEXT as *const u8,
        ))
        .member(func(
            "addStylesheet", "__RTS_FN_NS_DOM_ADD_STYLESHEET",
            Sig::new(vec![Handle, StrPtr], AbiType::Void),
            "addStylesheet(dom: number, css: string): void",
            "inject an author stylesheet (same path as inline <style>): external CSS \
             from <link rel=stylesheet>/@import is loaded in TS and fed to the cascade.",
            __RTS_FN_NS_DOM_ADD_STYLESHEET as *const u8,
        ))
        .member(func(
            "runScript", "__RTS_FN_NS_DOM_RUN_SCRIPT",
            Sig::new(vec![Handle, I64, StrPtr], AbiType::Void),
            "runScript(dom: number, node: number, code: string): void",
            "materialize a loaded <script src> source as the node's text (load, not \
             execute — the new engine has no in-process eval with DOM access yet).",
            __RTS_FN_NS_DOM_RUN_SCRIPT as *const u8,
        ))
        .member(func(
            "setStyleProperty", "__RTS_FN_NS_DOM_SET_STYLE_PROPERTY",
            Sig::new(vec![Handle, I64, StrPtr, StrPtr], AbiType::Void),
            "setStyleProperty(dom: number, node: number, name: string, value: string): void",
            "el.style.setProperty(name, value): set one inline property.",
            __RTS_FN_NS_DOM_SET_STYLE_PROPERTY as *const u8,
        ))
        .member(func(
            "removeStyleProperty", "__RTS_FN_NS_DOM_REMOVE_STYLE_PROPERTY",
            Sig::new(vec![Handle, I64, StrPtr], AbiType::Void),
            "removeStyleProperty(dom: number, node: number, name: string): void",
            "el.style.removeProperty(name).",
            __RTS_FN_NS_DOM_REMOVE_STYLE_PROPERTY as *const u8,
        ))
        // ── Eventos (#1760) ─────────────────────────────────────────────────────
        .member(func(
            "addListener", "__RTS_FN_NS_DOM_ADD_LISTENER",
            Sig::new(vec![Handle, I64, StrPtr], AbiType::Void),
            "addListener(dom: number, node: number, type: string): void",
            "element.addEventListener(type): register the node as listening for type.",
            __RTS_FN_NS_DOM_ADD_LISTENER as *const u8,
        ))
        .member(func(
            "addListenerCb", "__RTS_FN_NS_DOM_ADD_LISTENER_CB",
            Sig::new(vec![Handle, I64, StrPtr, I64], AbiType::Void),
            "addListenerCb(dom: number, node: number, type: string, cb: number): void",
            "element.addEventListener(type, fn): register type AND the callback \
             (Function word/handle, stored opaque — the TS facade invokes it).",
            __RTS_FN_NS_DOM_ADD_LISTENER_CB as *const u8,
        ))
        .member(func(
            "dispatchCollect", "__RTS_FN_NS_DOM_DISPATCH_COLLECT",
            Sig::new(vec![Handle, I64, StrPtr, I64], I64),
            "dispatchCollect(dom: number, target: number, type: string, bubbles: number): number",
            "dispatch collecting callbacks (target then bubbling) into the Dom \
             scratch; returns how many. Read with dispatchCbAt/dispatchCbNode and \
             COPY before invoking (a callback may re-dispatch).",
            __RTS_FN_NS_DOM_DISPATCH_COLLECT as *const u8,
        ))
        .member(func(
            "dispatchCbAt", "__RTS_FN_NS_DOM_DISPATCH_CB_AT",
            Sig::new(vec![Handle, I64], I64),
            "dispatchCbAt(dom: number, i: number): number",
            "i-th collected callback word (0 if out of range).",
            __RTS_FN_NS_DOM_DISPATCH_CB_AT as *const u8,
        ))
        .member(func(
            "dispatchCbNode", "__RTS_FN_NS_DOM_DISPATCH_CB_NODE",
            Sig::new(vec![Handle, I64], I64),
            "dispatchCbNode(dom: number, i: number): number",
            "NodeId of the listening node in the i-th collected pair (-1 if out of \
             range) — the handler's currentTarget.",
            __RTS_FN_NS_DOM_DISPATCH_CB_NODE as *const u8,
        ))
        .member(func(
            "removeListener", "__RTS_FN_NS_DOM_REMOVE_LISTENER",
            Sig::new(vec![Handle, I64, StrPtr], AbiType::Void),
            "removeListener(dom: number, node: number, type: string): void",
            "element.removeEventListener(type).",
            __RTS_FN_NS_DOM_REMOVE_LISTENER as *const u8,
        ))
        .member(func(
            "hasListener", "__RTS_FN_NS_DOM_HAS_LISTENER",
            Sig::new(vec![Handle, I64, StrPtr], I64),
            "hasListener(dom: number, node: number, type: string): number",
            "1 if the node listens for the type, 0 otherwise.",
            __RTS_FN_NS_DOM_HAS_LISTENER as *const u8,
        ))
        .member(func(
            "dispatchEvent", "__RTS_FN_NS_DOM_DISPATCH_EVENT",
            Sig::new(vec![Handle, I64, StrPtr, I64], I64),
            "dispatchEvent(dom: number, target: number, type: string, bubbles: number): number",
            "element.dispatchEvent(type, bubbles): fire; bubbles!=0 propagates to ancestors.",
            __RTS_FN_NS_DOM_DISPATCH_EVENT as *const u8,
        ))
        .member(func(
            "pollEvent", "__RTS_FN_NS_DOM_POLL_EVENT",
            Sig::new(vec![Handle], I64),
            "pollEvent(dom: number): number",
            "next pending event's NodeId (-1 if none); stores type for pollEventType.",
            __RTS_FN_NS_DOM_POLL_EVENT as *const u8,
        ))
        .member(func(
            "setHovered", "__RTS_FN_NS_DOM_SET_HOVERED",
            Sig::new(vec![Handle, I64], AbiType::Void),
            "setHovered(dom: number, node: number): void",
            "set the node under the cursor (-1 = none) — live :hover state; the \
             real backend feeds this per frame via hit-test.",
            __RTS_FN_NS_DOM_SET_HOVERED as *const u8,
        ))
        .member(func(
            "pushRawEvent", "__RTS_FN_NS_DOM_PUSH_RAW_EVENT",
            Sig::new(vec![Handle, I64, StrPtr], AbiType::Void),
            "pushRawEvent(dom: number, node: number, type: string): void",
            "push a raw backend-style event (same path as the mouse hit-test) — \
             synthetic events / headless tests of the full cycle.",
            __RTS_FN_NS_DOM_PUSH_RAW_EVENT as *const u8,
        ))
        .member(func(
            "pollRawEvent", "__RTS_FN_NS_DOM_POLL_RAW_EVENT",
            Sig::new(vec![Handle], I64),
            "pollRawEvent(dom: number): number",
            "next backend-origin raw event's NodeId (mouse hit-test; -1 if none); \
             stores type for pollRawEventType. pumpEventCallbacks drains this.",
            __RTS_FN_NS_DOM_POLL_RAW_EVENT as *const u8,
        ))
        .member(func(
            "pollRawEventType", "__RTS_FN_NS_DOM_POLL_RAW_EVENT_TYPE",
            Sig::new(vec![Handle], AbiType::Handle),
            "pollRawEventType(dom: number): string",
            "type of the raw event delivered by the last pollRawEvent ('' if none).",
            __RTS_FN_NS_DOM_POLL_RAW_EVENT_TYPE as *const u8,
        ))
        .member(func(
            "pollEventType", "__RTS_FN_NS_DOM_POLL_EVENT_TYPE",
            Sig::new(vec![Handle], AbiType::Handle),
            "pollEventType(dom: number): string",
            "type of the event delivered by the last pollEvent ('' if none).",
            __RTS_FN_NS_DOM_POLL_EVENT_TYPE as *const u8,
        ))
        .member(func(
            "advance", "__RTS_FN_NS_DOM_ADVANCE",
            Sig::new(vec![Handle, AbiType::F64], I64),
            "advance(dom: number, nowMs: number): number",
            "advance animations to nowMs (DOM-internal loop, #1776); 1 if active (repaint), 0 if static.",
            __RTS_FN_NS_DOM_ADVANCE as *const u8,
        ))
        // ── Formulário: input editável (mini-browser) ───────────────────────────
        .member(func(
            "inputAt", "__RTS_FN_NS_DOM_INPUT_AT",
            Sig::new(vec![Handle, I64, I64, I64], I64),
            "inputAt(dom: number, viewportW: number, x: number, y: number): number",
            "NodeId of the <input>/<textarea> whose box contains (x,y); -1 if none.",
            __RTS_FN_NS_DOM_INPUT_AT as *const u8,
        ))
        .member(func(
            "focusInput", "__RTS_FN_NS_DOM_FOCUS_INPUT",
            Sig::new(vec![Handle, I64], AbiType::Void),
            "focusInput(dom: number, node: number): void",
            "give keyboard focus to node (receives typed text); node=-1 clears focus.",
            __RTS_FN_NS_DOM_FOCUS_INPUT as *const u8,
        ))
        .member(func(
            "setImage", "__RTS_FN_NS_DOM_SET_IMAGE",
            Sig::new(vec![Handle, I64, AbiType::U64, I64, I64, I64], AbiType::Void),
            "setImage(dom: number, node: number, bufferHandle: number, off: number, w: number, h: number): void",
            "attach decoded RGBA pixels to an <img> node so the layout paints it.",
            __RTS_FN_NS_DOM_SET_IMAGE as *const u8,
        ))
        .member(func(
            "hasImage", "__RTS_FN_NS_DOM_HAS_IMAGE",
            Sig::new(vec![Handle, I64], I64),
            "hasImage(dom: number, node: number): number",
            "1 if the node has an image set (diagnostic), 0 otherwise.",
            __RTS_FN_NS_DOM_HAS_IMAGE as *const u8,
        ))
        .member(func(
            "focusedInput", "__RTS_FN_NS_DOM_FOCUSED_INPUT",
            Sig::new(vec![Handle], I64),
            "focusedInput(dom: number): number",
            "NodeId of the currently focused input (-1 if none).",
            __RTS_FN_NS_DOM_FOCUSED_INPUT as *const u8,
        ))
        .member(func(
            "inputFeedText", "__RTS_FN_NS_DOM_INPUT_FEED_TEXT",
            Sig::new(vec![Handle, StrPtr], I64),
            "inputFeedText(dom: number, text: string): number",
            "append text to the focused input; 1 if changed (repaint), 0 otherwise.",
            __RTS_FN_NS_DOM_INPUT_FEED_TEXT as *const u8,
        ))
        .member(func(
            "inputBackspace", "__RTS_FN_NS_DOM_INPUT_BACKSPACE",
            Sig::new(vec![Handle], I64),
            "inputBackspace(dom: number): number",
            "delete the last char of the focused input; 1 if changed, 0 otherwise.",
            __RTS_FN_NS_DOM_INPUT_BACKSPACE as *const u8,
        ))
        .member(func(
            "inputValue", "__RTS_FN_NS_DOM_INPUT_VALUE",
            Sig::new(vec![Handle, I64], AbiType::Handle),
            "inputValue(dom: number, node: number): string",
            "current text of the input (typed value or value= attribute); '' if not an input.",
            __RTS_FN_NS_DOM_INPUT_VALUE as *const u8,
        ))
        .member(func(
            "getAttribute",
            "__RTS_FN_NS_DOM_GET_ATTRIBUTE",
            Sig::new(vec![Handle, I64, StrPtr], AbiType::Handle),
            "getAttribute(dom: number, node: number, name: string): string",
            "element.getAttribute(name). Empty string if the attribute is missing or the node is invalid (the TS facade may map '' to null).",
            __RTS_FN_NS_DOM_GET_ATTRIBUTE as *const u8,
        ))
        .member(func(
            "tagName",
            "__RTS_FN_NS_DOM_TAG_NAME",
            Sig::new(vec![Handle, I64], AbiType::Handle),
            "tagName(dom: number, node: number): string",
            "Lowercase tag name of an element. Empty string if the node is invalid or not an element (the TS facade upper-cases it like the browser).",
            __RTS_FN_NS_DOM_TAG_NAME as *const u8,
        ))
        // ── Coleções de nós: par count + at (sem array-return do Rust) ───────────
        .member(func(
            "querySelectorAllCount",
            "__RTS_FN_NS_DOM_QUERY_SELECTOR_ALL_COUNT",
            Sig::new(vec![Handle, StrPtr], I64),
            "querySelectorAllCount(dom: number, selector: string): number",
            "How many nodes match a simple selector (tag / #id / .class). Pair with querySelectorAllAt to iterate (the TS facade builds the array).",
            __RTS_FN_NS_DOM_QUERY_SELECTOR_ALL_COUNT as *const u8,
        ))
        .member(func(
            "querySelectorAllAt",
            "__RTS_FN_NS_DOM_QUERY_SELECTOR_ALL_AT",
            Sig::new(vec![Handle, StrPtr, I64], I64),
            "querySelectorAllAt(dom: number, selector: string, index: number): number",
            "The NodeId of the index-th node matching the selector (document order), or -1 if out of range. Extract to a const before comparing.",
            __RTS_FN_NS_DOM_QUERY_SELECTOR_ALL_AT as *const u8,
        ))
        .member(func(
            "childCount",
            "__RTS_FN_NS_DOM_CHILD_COUNT",
            Sig::new(vec![Handle, I64], I64),
            "childCount(dom: number, node: number): number",
            "Number of element children of a node (excludes text nodes, like element.children). Pair with childAt.",
            __RTS_FN_NS_DOM_CHILD_COUNT as *const u8,
        ))
        .member(func(
            "childAt",
            "__RTS_FN_NS_DOM_CHILD_AT",
            Sig::new(vec![Handle, I64, I64], I64),
            "childAt(dom: number, node: number, index: number): number",
            "The NodeId of the index-th element child, or -1 if out of range. Extract to a const before comparing.",
            __RTS_FN_NS_DOM_CHILD_AT as *const u8,
        ))
        // ── Estilo computado por nó (o LAYOUT em TS lê isto) ─────────────────────
        .member(func(
            "nodeStyleSlot",
            "__RTS_FN_NS_DOM_NODE_STYLE_SLOT",
            Sig::new(vec![Handle, I64, I64], I64),
            "nodeStyleSlot(dom: number, node: number, slot: number): number",
            "Computed style slot value of a node (tag-style + inline), or -1 if unset. Slots 0=color 1=bg 2=font_size 3=padding 4=margin 5=border_width 6=border_color 7=corner_radius. The TS layout engine reads this.",
            __RTS_FN_NS_DOM_NODE_STYLE_SLOT as *const u8,
        ))
        .member(func(
            "displayOf",
            "__RTS_FN_NS_DOM_DISPLAY_OF",
            Sig::new(vec![Handle, I64], I64),
            "displayOf(dom: number, node: number): number",
            "Display code of a node (0=vertical 1=wrap 2=horizontal 3=grid), or -1 if not a block tag. The TS layout engine reads this to choose the stacking axis.",
            __RTS_FN_NS_DOM_DISPLAY_OF as *const u8,
        ))
        // ── defineStyle / defineBlock / defineInline (estilo/layout por-tag) ─────
        .member(func(
            "defineStyle",
            "__RTS_FN_NS_DOM_DEFINE_STYLE",
            Sig::new(vec![StrPtr, I64, I64], AbiType::Void),
            "defineStyle(tag: string, slot: number, val: number): void",
            "Registers one opaque style slot for a tag (0=color 1=bg 2=font_size 3=padding 4=margin 5=border_width 6=border_color 7=corner_radius 8=width; colors as 0xRRGGBBAA u32; width as encoded Dimension). The TS maps CSS-name->slot; Rust never matches a CSS string. Accumulates per tag.",
            __RTS_FN_NS_DOM_DEFINE_STYLE as *const u8,
        ))
        .member(func(
            "setStyle",
            "__RTS_FN_NS_DOM_SET_STYLE",
            Sig::new(vec![Handle, I64, I64, I64], AbiType::Void),
            "setStyle(dom: number, node: number, slot: number, val: number): void",
            "Applies one opaque style slot to a single NODE (per-node override; beats tag and inline). Same slots as defineStyle. For many nodes/props use setStyleBatch.",
            __RTS_FN_NS_DOM_SET_STYLE as *const u8,
        ))
        .member(func(
            "setStyleBatch",
            "__RTS_FN_NS_DOM_SET_STYLE_BATCH",
            Sig::new(vec![Handle, Handle, I64], AbiType::Void),
            "setStyleBatch(dom: number, buffer: number, count: number): void",
            "Applies count (nodeId,slot,val) triples at once from a buffer handle (count*3 little-endian i64s). The batch form is mandatory for styling many nodes per frame (invariant 6): N nodes would otherwise be N*5 FFI calls.",
            __RTS_FN_NS_DOM_SET_STYLE_BATCH as *const u8,
        ))
        .member(func(
            "defineBlock",
            "__RTS_FN_NS_DOM_DEFINE_BLOCK",
            Sig::new(vec![StrPtr, I64, AbiType::F64, I64, I64], AbiType::Void),
            "defineBlock(tag: string, display: number, indent: number, prefix: number, flags: number): void",
            "Registers how a tag lays out (display/indent/prefix/flags). No tag is hardcoded in Rust.",
            __RTS_FN_NS_DOM_DEFINE_BLOCK as *const u8,
        ))
        .member(func(
            "defineInline",
            "__RTS_FN_NS_DOM_DEFINE_INLINE",
            Sig::new(vec![StrPtr, I64], AbiType::Void),
            "defineInline(tag: string, flags: number): void",
            "Registers an inline tag's style (flags: BOLD=8|ITALIC=16|MONO=1). No tag is hardcoded in Rust.",
            __RTS_FN_NS_DOM_DEFINE_INLINE as *const u8,
        ))
        .done();
}

#[cfg(test)]
mod tests {
    use super::*;

    // These tests used to carry a `#[cfg(test)]` `#[no_mangle]` STUB of
    // `__RTS_FN_NS_GC_STRING_NEW` backed by a thread-local pool, because the real
    // definition lived in `rts-std`, which is not linked into this crate's test
    // binary (`rts-dom` depends only on `rts-engine`).
    //
    // That premise died when the string pool moved down into
    // `rts-engine::heap::string_pool`: the real symbol IS linked here now, so the
    // stub became a SECOND definition of it and the test binary failed to link
    // (LNK2005). Deleting it beats fixing it — these tests now exercise the same
    // string pool the runtime does, instead of a mock free to drift from it.
    /// Read back a string handle returned by a DOM member (getText /
    /// getAttribute / tagName).
    fn gc_str(handle: u64) -> String {
        rts_engine::heap::handles::read_string_handle(handle).unwrap_or_default()
    }

    #[test]
    fn get_text_devolve_conteudo_como_string() {
        let html = "<div id='x'>Olá <b>mundo</b></div>";
        let h = __RTS_FN_NS_DOM_PARSE_HTML(html.as_ptr(), html.len() as i64);
        let sel = "#x";
        let node = __RTS_FN_NS_DOM_QUERY_SELECTOR(h, sel.as_ptr(), sel.len() as i64);
        assert!(node >= 0);
        let text_handle = __RTS_FN_NS_DOM_GET_TEXT(h, node);
        assert_eq!(gc_str(text_handle), "Olá mundo", "textContent concatena descendentes");
        // Nó inválido (sentinela -1) → string vazia, sem panic.
        let empty = __RTS_FN_NS_DOM_GET_TEXT(h, -1);
        assert_eq!(gc_str(empty), "");
        __RTS_FN_NS_DOM_FREE(h);
    }

    #[test]
    fn get_attribute_e_tag_name_como_string() {
        let html = "<a href='https://x' class='lnk'>l</a>";
        let h = __RTS_FN_NS_DOM_PARSE_HTML(html.as_ptr(), html.len() as i64);
        let sel = "a";
        let node = __RTS_FN_NS_DOM_QUERY_SELECTOR(h, sel.as_ptr(), sel.len() as i64);
        assert!(node >= 0);
        // getAttribute presente.
        let name = "href";
        let href = __RTS_FN_NS_DOM_GET_ATTRIBUTE(h, node, name.as_ptr(), name.len() as i64);
        assert_eq!(gc_str(href), "https://x");
        // getAttribute AUSENTE → "".
        let missing = "data-nope";
        let none = __RTS_FN_NS_DOM_GET_ATTRIBUTE(h, node, missing.as_ptr(), missing.len() as i64);
        assert_eq!(gc_str(none), "");
        // tagName → minúsculas.
        let tag = __RTS_FN_NS_DOM_TAG_NAME(h, node);
        assert_eq!(gc_str(tag), "a");
        __RTS_FN_NS_DOM_FREE(h);
    }

    #[test]
    fn query_selector_all_count_e_at() {
        let html = "<ul><li class='it'>a</li><li class='it'>b</li><li class='it'>c</li></ul>";
        let h = __RTS_FN_NS_DOM_PARSE_HTML(html.as_ptr(), html.len() as i64);
        let sel = ".it";
        let count =
            __RTS_FN_NS_DOM_QUERY_SELECTOR_ALL_COUNT(h, sel.as_ptr(), sel.len() as i64);
        assert_eq!(count, 3, "três <li class='it'>");
        // itera via at e confere o texto de cada um.
        let mut texts = Vec::new();
        for i in 0..count {
            let node =
                __RTS_FN_NS_DOM_QUERY_SELECTOR_ALL_AT(h, sel.as_ptr(), sel.len() as i64, i);
            assert!(node >= 0);
            texts.push(gc_str(__RTS_FN_NS_DOM_GET_TEXT(h, node)));
        }
        assert_eq!(texts, vec!["a", "b", "c"]);
        // índice fora do intervalo → -1.
        let oob = __RTS_FN_NS_DOM_QUERY_SELECTOR_ALL_AT(h, sel.as_ptr(), sel.len() as i64, 3);
        assert_eq!(oob, NODE_NONE);
        let neg = __RTS_FN_NS_DOM_QUERY_SELECTOR_ALL_AT(h, sel.as_ptr(), sel.len() as i64, -1);
        assert_eq!(neg, NODE_NONE);
        // seletor sem match → count 0.
        let nada = ".inexistente";
        assert_eq!(
            __RTS_FN_NS_DOM_QUERY_SELECTOR_ALL_COUNT(h, nada.as_ptr(), nada.len() as i64),
            0
        );
        __RTS_FN_NS_DOM_FREE(h);
    }

    #[test]
    fn child_count_e_at_so_elementos() {
        // O <div> tem texto + 2 elementos filhos; childCount exclui o texto.
        let html = "<div>txt<span>a</span><b>c</b></div>";
        let h = __RTS_FN_NS_DOM_PARSE_HTML(html.as_ptr(), html.len() as i64);
        let sel = "div";
        let div = __RTS_FN_NS_DOM_QUERY_SELECTOR(h, sel.as_ptr(), sel.len() as i64);
        assert!(div >= 0);
        assert_eq!(__RTS_FN_NS_DOM_CHILD_COUNT(h, div), 2, "exclui o nó de texto");
        let c0 = __RTS_FN_NS_DOM_CHILD_AT(h, div, 0);
        let c1 = __RTS_FN_NS_DOM_CHILD_AT(h, div, 1);
        assert_eq!(gc_str(__RTS_FN_NS_DOM_TAG_NAME(h, c0)), "span");
        assert_eq!(gc_str(__RTS_FN_NS_DOM_TAG_NAME(h, c1)), "b");
        assert_eq!(__RTS_FN_NS_DOM_CHILD_AT(h, div, 2), NODE_NONE, "fora do intervalo → -1");
        __RTS_FN_NS_DOM_FREE(h);
    }

    #[test]
    fn define_style_block_inline_alimentam_os_stores() {
        // defineStyle acumula no store de style; defineBlock/defineInline no de block.
        let tag = "claudetag";
        __RTS_FN_NS_DOM_DEFINE_STYLE(
            tag.as_ptr(),
            tag.len() as i64,
            crate::style::SLOT_COLOR,
            0x0088FFFF,
        );
        __RTS_FN_NS_DOM_DEFINE_STYLE(
            tag.as_ptr(),
            tag.len() as i64,
            crate::style::SLOT_FONT_SIZE,
            28,
        );
        let s = crate::style::lookup_style(tag).expect("style registrado");
        assert_eq!(s.color, Some(0x0088FFFF));
        assert_eq!(s.font_size, Some(crate::style::Dimension::Px(28.0)));

        let btag = "claudeblk";
        __RTS_FN_NS_DOM_DEFINE_BLOCK(
            btag.as_ptr(),
            btag.len() as i64,
            crate::block::DISPLAY_VERTICAL,
            16.0,
            crate::block::PREFIX_BULLET,
            crate::block::FLAG_HEADING,
        );
        let b = crate::block::lookup(btag).expect("block registrado");
        assert_eq!(b.display, crate::block::DISPLAY_VERTICAL);
        assert_eq!(b.indent, 16.0);
        assert_eq!(b.prefix, crate::block::PREFIX_BULLET);
        assert!(b.has(crate::block::FLAG_HEADING));

        let itag = "claudeinl";
        __RTS_FN_NS_DOM_DEFINE_INLINE(
            itag.as_ptr(),
            itag.len() as i64,
            crate::block::FLAG_BOLD | crate::block::FLAG_ITALIC,
        );
        assert_eq!(
            crate::block::lookup_inline(itag),
            crate::block::FLAG_BOLD | crate::block::FLAG_ITALIC
        );
        // tag vazia é no-op (não panica, não registra).
        __RTS_FN_NS_DOM_DEFINE_INLINE("".as_ptr(), 0, 1);
    }

    #[test]
    fn headless_parse_query_mutate() {
        // Fluxo headless completo SEM janela: parse → query → setText → re-dump.
        let h = __RTS_FN_NS_DOM_PARSE_HTML("<div id='x'>antes</div>".as_ptr(), 23);
        assert!(h >= 1);
        // query por #id
        let sel = "#x";
        let node = __RTS_FN_NS_DOM_QUERY_SELECTOR(h, sel.as_ptr(), sel.len() as i64);
        assert!(node >= 0, "deveria achar #x");
        // setText
        let txt = "depois";
        __RTS_FN_NS_DOM_SET_TEXT(h, node, txt.as_ptr(), txt.len() as i64);
        // o dump reflete a mutação
        let dumped = with(h, |dom| dom.dump()).unwrap();
        assert!(dumped.contains("\"depois\""), "dump: {dumped}");
        assert!(!dumped.contains("\"antes\""));
        // free
        __RTS_FN_NS_DOM_FREE(h);
        assert!(with(h, |_| ()).is_none(), "handle deveria estar livre");
    }

    #[test]
    fn query_selector_inexistente_e_minus_one() {
        let h = __RTS_FN_NS_DOM_PARSE_HTML("<p>oi</p>".as_ptr(), 9);
        let sel = "#naoexiste";
        let node = __RTS_FN_NS_DOM_QUERY_SELECTOR(h, sel.as_ptr(), sel.len() as i64);
        assert_eq!(node, NODE_NONE);
        __RTS_FN_NS_DOM_FREE(h);
    }

    #[test]
    fn create_e_append_headless() {
        let h = __RTS_FN_NS_DOM_CREATE_DOCUMENT();
        let li = __RTS_FN_NS_DOM_CREATE_ELEMENT(h, "li".as_ptr(), 2);
        assert!(li >= 0);
        let root = __RTS_FN_NS_DOM_ROOT_ID(h);
        assert!(root >= 0);
        __RTS_FN_NS_DOM_APPEND_CHILD(h, root, li);
        let dumped = with(h, |dom| dom.dump()).unwrap();
        assert!(dumped.contains("<li>"), "dump: {dumped}");
        __RTS_FN_NS_DOM_FREE(h);
    }
}
