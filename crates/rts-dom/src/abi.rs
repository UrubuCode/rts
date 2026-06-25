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

use std::cell::RefCell;
use std::collections::HashMap;

use rts_engine::abi::str_abi;
use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

use crate::dom::{parse_html_to_dom, Dom, NodeId};

// Handle de DOM avulso cruza como `u64` (mesma convenção do egui UiCtx); NodeId
// versionado cruza como `i64`. Aliases locais p/ legibilidade das `Sig`.
use AbiType::{I64, StrPtr, U64 as Handle};

thread_local! {
    /// Store de Doms avulsos: `handle u64 → Dom`. Próprio deste crate (o engine
    /// não conhece o DOM — doutrina). Cresce sob `parseHtml`/`createDocument`,
    /// some sob `freeDom`.
    static DOMS: RefCell<HashMap<u64, Dom>> = RefCell::new(HashMap::new());
    /// Próximo handle a alocar (começa em 1; 0 = "nenhum DOM").
    static NEXT: RefCell<u64> = const { RefCell::new(1) };
}

/// Aloca um handle e guarda o `Dom`. Retorna o handle (≥ 1).
fn insert(dom: Dom) -> u64 {
    let h = NEXT.with(|n| {
        let mut n = n.borrow_mut();
        let h = *n;
        *n += 1;
        h
    });
    DOMS.with(|m| m.borrow_mut().insert(h, dom));
    h
}

/// Roda `f` com acesso imutável ao `Dom` de `h`. `None` se o handle não existe.
fn with<R>(h: u64, f: impl FnOnce(&Dom) -> R) -> Option<R> {
    DOMS.with(|m| m.borrow().get(&h).map(f))
}

/// Roda `f` com acesso mutável ao `Dom` de `h`. `None` se o handle não existe.
fn with_mut<R>(h: u64, f: impl FnOnce(&mut Dom) -> R) -> Option<R> {
    DOMS.with(|m| m.borrow_mut().get_mut(&h).map(f))
}

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
    DOMS.with(|m| {
        m.borrow_mut().remove(&h);
    });
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
        .done();
}

#[cfg(test)]
mod tests {
    use super::*;

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
