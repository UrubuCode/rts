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
// `rts-primitives::string::transform`: a fn é definida em `rts-std`
// (collector::string_pool, símbolo `__RTS_FN_NS_GC_STRING_NEW`), resolvida no link
// final do runtime — por isso só um `extern "C"` aqui, sem dep de Cargo (mantém a
// doutrina "rts-dom depende só de rts-engine"; a resolução é estática no binário).
unsafe extern "C" {
    fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64;
}

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

/// `nodeStyleSlot(domHandle, node, slot)` → valor (`i64`) do SLOT de estilo do nó
/// (estilo-de-tag + `style=""` inline resolvidos), ou `-1` se não-setado/inválido.
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
        pure: false,
        intrinsic: None,
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
            "Registers one opaque style slot for a tag (0=color 1=bg 2=font_size 3=padding 4=margin 5=border_width 6=border_color 7=corner_radius; colors as 0xRRGGBBAA u32). The TS maps CSS-name->slot; Rust never matches a CSS string. Accumulates per tag.",
            __RTS_FN_NS_DOM_DEFINE_STYLE as *const u8,
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
    use std::cell::RefCell;

    // ── Stub do pool de strings GC para os testes UNITÁRIOS deste crate ──────────
    // `__RTS_FN_NS_GC_STRING_NEW` é definido em `rts-std` (string_pool), que NÃO é
    // linkado no test-binary do `rts-dom` (dep só de `rts-engine`). Para testar os
    // membros que RETORNAM string (getText/getAttribute/tagName) de ponta a ponta
    // sem puxar o runtime inteiro, fornecemos uma definição `#[no_mangle]` do MESMO
    // símbolo só sob `cfg(test)`: ela materializa a string num pool thread-local e
    // devolve o índice como "handle"; `gc_str(h)` lê de volta. No binário REAL do
    // runtime, a definição de `rts-std` é a que vale (o E2E exercita ESSA).
    thread_local! {
        static TEST_POOL: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    }

    #[unsafe(no_mangle)]
    extern "C" fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64 {
        let s = if ptr.is_null() || len < 0 {
            String::new()
        } else {
            let slice = unsafe { std::slice::from_raw_parts(ptr, len as usize) };
            String::from_utf8_lossy(slice).into_owned()
        };
        TEST_POOL.with(|p| {
            let mut p = p.borrow_mut();
            p.push(s);
            (p.len() - 1) as u64
        })
    }

    /// Lê de volta a string internada pelo stub acima (só nos testes).
    fn gc_str(handle: u64) -> String {
        TEST_POOL.with(|p| p.borrow().get(handle as usize).cloned().unwrap_or_default())
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
        assert_eq!(s.font_size, Some(28.0));

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
