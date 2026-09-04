//! Os LIMITES de um item flex no eixo principal: a base (`flex-basis` ou a
//! largura natural) e o tecto/piso de `max-width`/`min-width`, ambos como
//! OUTER — o que `FlexItem::base`/`main` são. Extraído de `flex.rs` (no teto
//! de 500) com o lote que deu ao item o `max-width` que lhe faltava
//! (`claude-flex-item-max-width`: o `.cover-container` do Bootstrap).

use super::*;

/// A BASE outer de um item flex no eixo principal: `flex-basis` explícita
/// (resolvida como o width — respeita box-sizing) + margens; `auto`/ausente →
/// width/conteúdo ([`child_outer_width`]). O `.col` do Bootstrap tem basis `0%`
/// → a base é só o frame (e o grow distribui o espaço).
pub(in crate::layout) fn flex_base_outer(
    dom: &Dom,
    id: NodeIdx,
    container_w: f32,
    parent_font: f32,
    ctx: &LayoutCtx,
) -> f32 {
    let css = dom.computed_style_idx(id).unwrap_or_default();
    let font = font_px(&css, parent_font);
    let resolve = ResolveCtx {
        parent_content_w: container_w,
        node_font_size: font,
        root_font_size: crate::style::root_font_size(),
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    let basis = css.flex_basis.and_then(|d| match d {
        crate::style::Dimension::Auto => None,
        other => other.resolve(&resolve),
    });
    let Some(basis) = basis else {
        return child_outer_width(dom, id, container_w, parent_font, ctx);
    };
    let margin_h = css.margin.resolve_h(&resolve);
    if css.border_box.unwrap_or(false) {
        basis + margin_h // border-box: a basis JÁ é a caixa (pad+borda inclusos)
    } else {
        basis + margin_h + 2.0 * css.border_width.unwrap_or(0.0) + css.padding.resolve_h(&resolve)
    }
}


/// `(max-width, min-width)` declarados do item, resolvidos e convertidos a
/// OUTER: em content-box somam o frame (padding + borda) e a margem; em
/// border-box só a margem. `None` quando não declarados.
pub(in crate::layout) fn limites_do_item(
    ccss: &ComputedStyle,
    content_w: f32,
    font_size: f32,
    ctx: &LayoutCtx,
) -> (Option<f32>, Option<f32>) {
    let font = font_px(ccss, font_size);
    let rc = ResolveCtx {
        parent_content_w: content_w,
        node_font_size: font,
        root_font_size: crate::style::root_font_size(),
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    let extra = if ccss.border_box.unwrap_or(false) {
        ccss.margin.resolve_h(&rc)
    } else {
        ccss.margin.resolve_h(&rc)
            + 2.0 * ccss.border_width.unwrap_or(0.0)
            + ccss.padding.resolve_h(&rc)
    };
    (
        ccss.max_width.and_then(|d| d.resolve(&rc)).map(|m| m + extra),
        ccss.min_width.and_then(|d| d.resolve(&rc)).map(|m| m + extra),
    )
}
