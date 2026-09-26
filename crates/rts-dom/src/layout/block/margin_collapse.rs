//! Margin collapse (CSS 2.1 §8.3.1) as arithmetic and as a question about
//! children: the pair rule (`collapse_margin`), the adjacent-margin set the
//! vertical flow carries (`Strut` and its two operations, `collapses_through`), and
//! the margins a block's first/last child lets escape through it
//! (`edge_margin_from_children`, `escaped_child_margins`,
//! `escaped_margins_for_box`).
//!
//! Apart because these are answers `layout_block`, `layout_children_vertical`,
//! `escaped_margin.rs` and `pseudo_block.rs` all ASK, and none of them owns:
//! kept inside `block.rs` and `vertical_flow.rs` they were two halves of one
//! rule split across the two largest functions of the folder. Rejected: one
//! file per origin (`block_margins.rs` + `flow_strut.rs`), which would keep
//! that split and only move it. Moved verbatim; `block.rs` and
//! `vertical_flow.rs` re-export every item so existing paths still resolve.

use super::*;
#[derive(Clone, Copy, PartialEq, Debug)]
enum MarginChildRole {
    Ignore,
    Barrier,
    Block { top: f32, bottom: f32 },
}

pub(in crate::layout) fn collapse_margin(first: f32, second: f32) -> f32 {
    if first >= 0.0 && second >= 0.0 {
        first.max(second)
    } else if first <= 0.0 && second <= 0.0 {
        first.min(second)
    } else {
        first + second
    }
}

fn margin_child_role(
    dom: &Dom,
    child: NodeIdx,
    content_w: f32,
    parent_font_size: f32,
    ctx: &LayoutCtx,
) -> MarginChildRole {
    match &dom.node(child).kind {
        NodeKind::Comment(_) => MarginChildRole::Ignore,
        NodeKind::Text(text) if text.trim().is_empty() => MarginChildRole::Ignore,
        NodeKind::Text(_) | NodeKind::Document => MarginChildRole::Barrier,
        NodeKind::Element { tag } if is_non_rendered_tag(tag) => MarginChildRole::Ignore,
        NodeKind::Element { .. } => {
            let css = dom.computed_style_idx(child).unwrap_or_default();
            if is_display_none(dom, child)
                || css
                    .position
                    .map(|position| position.out_of_flow())
                    .unwrap_or(false)
                || css
                    .float_side
                    .map(|side| side != crate::style::FloatSide::None)
                    .unwrap_or(false)
            {
                return MarginChildRole::Ignore;
            }
            let effective = css.effective_display();
            let block_candidate = match effective {
                Some(d) if d.is_inline_level() => false,
                Some(
                    crate::style::DisplayKind::TableRowGroup
                    | crate::style::DisplayKind::TableHeaderGroup
                    | crate::style::DisplayKind::TableFooterGroup
                    | crate::style::DisplayKind::TableRow
                    | crate::style::DisplayKind::TableCell
                    | crate::style::DisplayKind::TableCaption
                    | crate::style::DisplayKind::None,
                ) => false,
                Some(_) => true,
                None => is_block_level(dom, child) && !is_inline_block(dom, child),
            };
            if !block_candidate {
                return MarginChildRole::Barrier;
            }
            let resolve = ResolveCtx {
                parent_content_w: content_w,
                node_font_size: font_px(&css, parent_font_size),
                root_font_size: crate::style::root_font_size(),
                viewport_w: ctx.viewport_w,
                viewport_h: ctx.viewport_h,
            };
            let margin_v = css.margin_v.unwrap_or(0.0);
            let margin_top_extra = if css.margin.top == crate::style::Side::Unset {
                margin_v
            } else {
                0.0
            };
            let margin_bottom_extra = if css.margin.bottom == crate::style::Side::Unset {
                margin_v
            } else {
                0.0
            };
            MarginChildRole::Block {
                top: css.margin.top.resolve(&resolve).unwrap_or(0.0) + margin_top_extra,
                bottom: css.margin.bottom.resolve(&resolve).unwrap_or(0.0) + margin_bottom_extra,
            }
        }
    }
}

pub(in crate::layout) fn edge_margin_from_children(
    dom: &Dom,
    id: NodeIdx,
    content_w: f32,
    parent_font_size: f32,
    ctx: &LayoutCtx,
    from_end: bool,
) -> Option<f32> {
    let children = &dom.node(id).children;
    if from_end {
        for &child in children.iter().rev() {
            match margin_child_role(dom, child, content_w, parent_font_size, ctx) {
                MarginChildRole::Ignore => continue,
                MarginChildRole::Barrier => return None,
                MarginChildRole::Block { bottom, .. } => return Some(bottom),
            }
        }
    } else {
        for &child in children {
            match margin_child_role(dom, child, content_w, parent_font_size, ctx) {
                MarginChildRole::Ignore => continue,
                MarginChildRole::Barrier => return None,
                MarginChildRole::Block { top, .. } => return Some(top),
            }
        }
    }
    None
}

pub(in crate::layout) fn escaped_child_margins(
    dom: &Dom,
    id: NodeIdx,
    parent_css: &ComputedStyle,
    content_w: f32,
    parent_font_size: f32,
    ctx: &LayoutCtx,
    pad_top: f32,
    border_top: f32,
    pad_bottom: f32,
    border_bottom: f32,
    bottom_auto_height: bool,
) -> (f32, f32) {
    if establishes_block_formatting_context(dom, id, parent_css) {
        return (0.0, 0.0);
    }
    let top = if pad_top == 0.0 && border_top == 0.0 {
        edge_margin_from_children(dom, id, content_w, parent_font_size, ctx, false).unwrap_or(0.0)
    } else {
        0.0
    };
    let bottom = if bottom_auto_height && pad_bottom == 0.0 && border_bottom == 0.0 {
        edge_margin_from_children(dom, id, content_w, parent_font_size, ctx, true).unwrap_or(0.0)
    } else {
        0.0
    };
    (top, bottom)
}

pub(in crate::layout) fn escaped_margins_for_box(
    dom: &Dom,
    id: NodeIdx,
    content_w: f32,
    parent_font_size: f32,
    ctx: &LayoutCtx,
) -> (f32, f32) {
    let css = dom.computed_style_idx(id).unwrap_or_default();
    let resolve = ResolveCtx {
        parent_content_w: content_w,
        node_font_size: font_px(&css, parent_font_size),
        root_font_size: crate::style::root_font_size(),
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    let pad_top = css.padding.top.resolve(&resolve).unwrap_or(0.0).max(0.0);
    let pad_bottom = css.padding.bottom.resolve(&resolve).unwrap_or(0.0).max(0.0);
    let [border_top, _, border_bottom, _] = crate::style::borders::used_widths(&css);
    escaped_child_margins(
        dom,
        id,
        &css,
        content_w,
        parent_font_size,
        ctx,
        pad_top,
        border_top,
        pad_bottom,
        border_bottom,
        css.height.is_none() && css.min_height.is_none(),
    )
}

/// O CONJUNTO de margens adjacentes ainda aberto, como o Blink o guarda
/// (`MarginStrut`): o MAIOR dos positivos e o MENOR dos negativos, somados uma
/// só vez no fim.
///
/// Substituiu a cadeia binária `colapso(colapso(a, b), c)`, que **não é
/// associativa com sinais mistos** e por isso respondia conforme a ordem:
/// (+10, −5, +20) dá 20 par a par e **15** pelo conjunto, que é o que um Chrome
/// real responde. Um par cabia num `f32`; um conjunto não, e era essa a falta —
/// não a fórmula do par, que estava certa.
pub(in crate::layout) type Strut = (f32, f32);

/// Junta mais uma margem ao conjunto. Cada sinal vai para o seu lado: um
/// positivo só compete com positivos, um negativo só com negativos.
///
/// É aqui e no [`collapsed_strut`] que vivem as três formas da regra do CSS
/// 2.1 §8.3.1, que antes eram um `colapso_de_margens(a, b)` binário: duas
/// positivas dão a maior (o `max` daqui), duas negativas dão a mais negativa (o
/// `min`), e uma de cada sinal dá a SOMA — que é o `pos + neg` do outro. É por
/// isso que uma margem negativa CANCELA uma positiva em vez de ser ignorada
/// por ela.
pub(in crate::layout) fn join_strut((pos, neg): Strut, m: f32) -> Strut {
    if m >= 0.0 {
        (pos.max(m), neg)
    } else {
        (pos, neg.min(m))
    }
}

/// O valor colapsado do conjunto — e é aqui que os dois sinais se encontram,
/// UMA vez. Com (+10, −5, +20) dá 20 − 5 = 15.
pub(in crate::layout) fn collapsed_strut((pos, neg): Strut) -> f32 {
    pos + neg
}

/// `true` se a caixa se ATRAVESSA a si própria (self-collapsing, CSS 2.1
/// §8.3.1): a altura externa é exactamente a soma das duas margens, logo o
/// conteúdo, o padding e a borda somaram zero. Medido num Chrome real: um
/// `<div style="margin:20px 0 30px">` vazio entre dois blocos injecta 30 e tem
/// altura 0; nós injectávamos 50.
///
/// A condição é lida do que foi CALCULADO e não rededuzida do estilo, o que a
/// torna certa de graça em dois casos que uma leitura de estilo erraria: um
/// bloco que cresceu para conter um float deixa de casar, e um com borda ou
/// padding também.
///
/// **O que ela ainda não sabe** é que uma caixa que estabelece um contexto de
/// formatação próprio (`overflow` ≠ visible, `flow-root`) NÃO se atravessa,
/// mesmo vazia. Isso é o lote do BFC; enquanto não houver, um `<div
/// style="overflow:hidden">` vazio e sem altura colapsa aqui e não devia.
pub(in crate::layout) fn collapses_through(box_h: f32, top_margin: f32, bottom_margin: f32) -> bool {
    (box_h - (top_margin + bottom_margin)).abs() < 0.01
}
