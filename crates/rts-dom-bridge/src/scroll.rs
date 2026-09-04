//! Scroll — `scrollTop`/`scrollLeft`/`scrollWidth`/`scrollHeight`/
//! `clientWidth`/`clientHeight`/`scrollTo`/`scrollIntoView` por nó, e
//! `pageScrollX`/`pageScrollY`/`setPageScroll` para a PÁGINA.
//!
//! Ficheiro À PARTE de `nodes.rs` (que já ultrapassaria o tecto de 500 linhas
//! do resto do workspace se ganhasse mais um bloco) e não uma extensão dele —
//! o padrão que os outros módulos deste crate já seguem (`events.rs` ao lado
//! de `nodes.rs`, não dentro).
//!
//! Todo o trabalho de verdade (offset, clamp, extensão de conteúdo) vive em
//! `rts_dom::dom::scroll` — este ficheiro só faz a mesma tradução que
//! `nodes.rs` já faz para o resto do DOM: `(env, this, a0..a3) -> u64` de um
//! lado, `Dom`/`NodeId`/`&str` do outro.

use rts_core::entry::Provided;
use rts_dom::NodeId;

use crate::value::{handle, integer, nothing, num, number};

pub const MEMBERS: &[(&str, Provided)] = &[
    // por nó
    ("scrollTop", scroll_top),
    ("scrollLeft", scroll_left),
    ("setScrollTop", set_scroll_top),
    ("setScrollLeft", set_scroll_left),
    ("scrollWidth", scroll_width),
    ("scrollHeight", scroll_height),
    ("clientWidth", client_width),
    ("clientHeight", client_height),
    ("elementScrollTo", element_scroll_to),
    ("scrollIntoView", scroll_into_view),
    // página
    ("pageScrollX", page_scroll_x),
    ("pageScrollY", page_scroll_y),
    ("setPageScroll", set_page_scroll),
];

/// O `NodeId` de um argumento, ou `None` para a sentinela `-1` — mesma leitura
/// que `nodes.rs::node`, repetida aqui porque `nodes.rs` não a expõe (é
/// `pub(crate)` dentro daquele ficheiro) e duplicar uma leitura de três linhas
/// custa menos do que acoplar dois módulos de fronteira por um helper.
fn node(value: u64) -> Option<NodeId> {
    NodeId::from_abi(integer(value, -1))
}

extern "C" fn scroll_top(_e: u64, _t: u64, doc: u64, n: u64, _b: u64, _c: u64) -> u64 {
    let Some(id) = node(n) else { return num(0.0) };
    let v = rts_dom::store::with_dom(handle(doc), |d| d.scroll_of(id).1).unwrap_or(0.0);
    num(v as f64)
}

extern "C" fn scroll_left(_e: u64, _t: u64, doc: u64, n: u64, _b: u64, _c: u64) -> u64 {
    let Some(id) = node(n) else { return num(0.0) };
    let v = rts_dom::store::with_dom(handle(doc), |d| d.scroll_of(id).0).unwrap_or(0.0);
    num(v as f64)
}

/// `el.scrollTop = v` — escreve só o eixo Y, preservando X. `set_scroll`
/// clampa ao conteúdo (`rts_dom::dom::scroll`); um `id` que não resolve, ou
/// que não é uma região rolável, é um no-op silencioso.
extern "C" fn set_scroll_top(_e: u64, _t: u64, doc: u64, n: u64, v: u64, _c: u64) -> u64 {
    let Some(id) = node(n) else { return nothing() };
    let y = number(v, 0.0) as f32;
    rts_dom::store::with_dom_mut(handle(doc), |d| {
        let (x, _) = d.scroll_of(id);
        d.set_scroll(id, x, y);
    });
    nothing()
}

extern "C" fn set_scroll_left(_e: u64, _t: u64, doc: u64, n: u64, v: u64, _c: u64) -> u64 {
    let Some(id) = node(n) else { return nothing() };
    let x = number(v, 0.0) as f32;
    rts_dom::store::with_dom_mut(handle(doc), |d| {
        let (_, y) = d.scroll_of(id);
        d.set_scroll(id, x, y);
    });
    nothing()
}

extern "C" fn scroll_width(_e: u64, _t: u64, doc: u64, n: u64, _b: u64, _c: u64) -> u64 {
    let Some(id) = node(n) else { return num(0.0) };
    let v = rts_dom::store::with_dom(handle(doc), |d| d.scroll_extent(id).0).unwrap_or(0.0);
    num(v as f64)
}

extern "C" fn scroll_height(_e: u64, _t: u64, doc: u64, n: u64, _b: u64, _c: u64) -> u64 {
    let Some(id) = node(n) else { return num(0.0) };
    let v = rts_dom::store::with_dom(handle(doc), |d| d.scroll_extent(id).1).unwrap_or(0.0);
    num(v as f64)
}

extern "C" fn client_width(_e: u64, _t: u64, doc: u64, n: u64, _b: u64, _c: u64) -> u64 {
    let Some(id) = node(n) else { return num(0.0) };
    let v = rts_dom::store::with_dom(handle(doc), |d| d.scroll_extent(id).2).unwrap_or(0.0);
    num(v as f64)
}

extern "C" fn client_height(_e: u64, _t: u64, doc: u64, n: u64, _b: u64, _c: u64) -> u64 {
    let Some(id) = node(n) else { return num(0.0) };
    let v = rts_dom::store::with_dom(handle(doc), |d| d.scroll_extent(id).3).unwrap_or(0.0);
    num(v as f64)
}

/// `el.scrollTo(x, y)` / o alvo de `el.scrollBy(dx, dy)` (a fachada em
/// `dom.ts` soma `scrollLeft`/`scrollTop` antes de chamar) — os dois eixos de
/// uma vez, clampados juntos.
extern "C" fn element_scroll_to(_e: u64, _t: u64, doc: u64, n: u64, x: u64, y: u64) -> u64 {
    let Some(id) = node(n) else { return nothing() };
    let x = number(x, 0.0) as f32;
    let y = number(y, 0.0) as f32;
    rts_dom::store::with_dom_mut(handle(doc), |d| d.set_scroll(id, x, y));
    nothing()
}

/// `el.scrollIntoView()` — mínimo: alinha o topo do nó com o topo da região
/// (ou da página) que rola. Ver `Dom::scroll_into_view` para os cortes.
extern "C" fn scroll_into_view(_e: u64, _t: u64, doc: u64, n: u64, _b: u64, _c: u64) -> u64 {
    let Some(id) = node(n) else { return nothing() };
    rts_dom::store::with_dom_mut(handle(doc), |d| d.scroll_into_view(id));
    nothing()
}

extern "C" fn page_scroll_x(_e: u64, _t: u64, doc: u64, _a: u64, _b: u64, _c: u64) -> u64 {
    let v = rts_dom::store::with_dom(handle(doc), |d| d.page_scroll().0).unwrap_or(0.0);
    num(v as f64)
}

extern "C" fn page_scroll_y(_e: u64, _t: u64, doc: u64, _a: u64, _b: u64, _c: u64) -> u64 {
    let v = rts_dom::store::with_dom(handle(doc), |d| d.page_scroll().1).unwrap_or(0.0);
    num(v as f64)
}

/// `window.scrollTo(x, y)` / `window.scrollBy(dx, dy)` (a fachada em
/// `window.ts` soma `scrollX`/`scrollY` antes de chamar, como no elemento).
extern "C" fn set_page_scroll(_e: u64, _t: u64, doc: u64, x: u64, y: u64, _c: u64) -> u64 {
    let x = number(x, 0.0) as f32;
    let y = number(y, 0.0) as f32;
    rts_dom::store::with_dom_mut(handle(doc), |d| d.set_page_scroll(x, y));
    nothing()
}
