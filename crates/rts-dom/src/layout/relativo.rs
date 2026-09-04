//! `position: relative` — desloca a PINTURA sem tirar o elemento do fluxo.
//!
//! CSS 2.1 §9.4.3: os offsets de um `relative` movem a caixa já layoutada —
//! e o que a segue (`getBoundingClientRect`) — sem reservar nem libertar
//! espaço nenhum: o irmão seguinte fica exactamente onde ficaria se não
//! houvesse deslocamento. É por isso que este módulo corre DEPOIS de
//! `bloco.rs` já ter medido e posicionado a caixa e os filhos na posição
//! NATURAL — nada aqui volta a medir, só translada o que já existe.
//!
//! O mecanismo de "deslocar uma subárvore já pintada, in-place" já existe
//! para `transform` (`bloco.rs`, atalho `so_translate`) e é reusado aqui para
//! a metade da PINTURA (`list.items`/`list.children`). A diferença, e a razão
//! de não bastar chamar essa função: `transform` nunca toca `list.node_rects`
//! — é visual, não move o `getBoundingClientRect` (decisão já tomada nesse
//! módulo) — mas o offset de `relative` TEM de mover, porque é exactamente o
//! que o Chrome mede em `claude-position-relative.esperado.json`.
//! `list.node_rects` é um mapa achatado por NÓ, não uma fatia por posição
//! como `list.items`; a única forma de saber quais entradas são desta
//! subárvore é andar o DOM a partir de `id` — custa O(tamanho da subárvore),
//! pago só nos nós `relative` com offset não-nulo.
//!
//! Alternativa rejeitada: deslocar a caixa na MEDIÇÃO (somar o offset a
//! `x`/`y` antes de layoutar `id`), como o `absolute` faz contra o seu
//! containing block. Isso desloca a pintura correctamente, mas desloca
//! também o CURSOR que o pai devolve ao irmão seguinte — exactamente o que a
//! fixture pina que NÃO deve acontecer (`#seguinte` fica onde ficaria sem o
//! `#relativo` deslocado). Deslocar DEPOIS, só a pintura e a geometria já
//! produzidas, é o que mantém o espaço reservado no fluxo intacto.

use super::*;

/// Aplica o deslocamento de `position:relative` a um bloco já layoutado.
/// `box_index` é o mesmo marcador que `bloco.rs` usa para o `transform` — o
/// início, em `list.items`, da pintura desta caixa e dos seus descendentes.
/// Sem efeito quando `css.position` não é `Relative`, ou quando os quatro
/// insets resolvem a deslocamento nulo (não vale andar a subárvore à toa).
#[allow(clippy::too_many_arguments)]
pub(in crate::layout) fn aplica_offset_relativo(
    dom: &Dom,
    id: NodeIdx,
    css: &ComputedStyle,
    avail_w: f32,
    avail_h: Option<f32>,
    font_size: f32,
    box_index: usize,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) {
    if css.position != Some(crate::style::Position::Relative) {
        return;
    }
    let resolve = ResolveCtx {
        parent_content_w: avail_w,
        node_font_size: font_size,
        root_font_size: crate::style::root_font_size(),
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    // `left` vence `right` e `top` vence `bottom` em LTR (CSS 2.1 §9.4.3): com
    // os dois presentes de um eixo, só o do lado "de leitura" desloca. Sem
    // nenhum dos dois no eixo, offset zero — a caixa fica onde estava.
    let left = super::posicionado::resolve_inset(css.inset_left, avail_w, &resolve);
    let right = super::posicionado::resolve_inset(css.inset_right, avail_w, &resolve);
    let avail_h_axis = avail_h.unwrap_or(0.0);
    let top = super::posicionado::resolve_inset(css.inset_top, avail_h_axis, &resolve);
    let bottom = super::posicionado::resolve_inset(css.inset_bottom, avail_h_axis, &resolve);
    let dx = match (left, right) {
        (Some(l), _) => l,
        (None, Some(r)) => -r,
        (None, None) => 0.0,
    };
    let dy = match (top, bottom) {
        (Some(t), _) => t,
        (None, Some(b)) => -b,
        (None, None) => 0.0,
    };
    if dx == 0.0 && dy == 0.0 {
        return;
    }
    for it in list.items[box_index..].iter_mut() {
        translate_item(it, dx, dy);
    }
    for child in list.children.iter_mut().filter(|c| c.at >= box_index) {
        child.dx += dx;
        child.dy += dy;
    }
    // As subárvores servidas por fragmento (`list.children`, já deslocadas
    // acima) não têm entrada em `list.node_rects` — o passeio abaixo não as
    // encontra, e está certo que não encontre: já foram tratadas pelo `dx`/`dy`
    // do `ChildRef`, que `geometry_now`/`collect_geometry` somam ao ler.
    desloca_node_rects(dom, id, dx, dy, list);
}

fn desloca_node_rects(dom: &Dom, id: NodeIdx, dx: f32, dy: f32, list: &mut DisplayList) {
    if let Some(r) = list.node_rects.get_mut(&id) {
        r.x += dx;
        r.y += dy;
    }
    for &child in &dom.node(id).children {
        desloca_node_rects(dom, child, dx, dy, list);
    }
}
