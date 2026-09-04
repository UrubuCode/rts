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
        basis + margin_h + { let [_, r, _, l] = crate::style::borders::used_widths(&css); l + r } + css.padding.resolve_h(&resolve)
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
            + { let [_, r, _, l] = crate::style::borders::used_widths(ccss); l + r }
            + ccss.padding.resolve_h(&rc)
    };
    (
        ccss.max_width.and_then(|d| d.resolve(&rc)).map(|m| m + extra),
        ccss.min_width.and_then(|d| d.resolve(&rc)).map(|m| m + extra),
    )
}

/// A HYPOTHETICAL main size de um item (Flexbox §9.7 passo 2, combinado com o
/// piso automático de min-content do §4.5): um item `flex-grow:0` congela
/// direto na sua BASE sem nunca entrar no laço de grow/shrink de `flex.rs` —
/// e o piso tem de valer lá também, não só durante o encolhimento (onde já
/// valia, `flex.rs` linhas 328-370). Sem redistribuir pelos outros itens o
/// que o piso consome — o mesmo corte já aceite para o `max_main` acima.
/// `grid_cols` fica de fora: uma coluna de grid tem largura FIXA por desenho
/// (a base já veio zerada de grow/shrink em `flex.rs`), não pelo conteúdo.
pub(in crate::layout) fn com_piso_minimo(main: f32, min_main: f32, grid_cols: Option<i32>) -> f32 {
    if grid_cols.is_some() {
        main
    } else {
        main.max(min_main)
    }
}

/// O piso AUTOMÁTICO de min-content (Flexbox §4.5), antes de `min-width`
/// DECLARADO entrar (esse, quando presente, substitui este resultado por
/// inteiro — a spec só liga o automático a `min-width:auto`). O automático é
/// o MENOR entre o min-content (`min_content`, já medido pelo chamador) e a
/// "specified size suggestion": o `width` do item, quando é um comprimento
/// DEFINIDO — a spec exclui a `flex-basis` desta conta de propósito, por
/// isso um `flex-basis:0` sem `width` fica no min-content puro, sem teto
/// (`claude-flex-basis-zero-min-content`, o piso do lote flex-basis-piso).
///
/// Sem este teto, um item `width:100%; aspect-ratio:1/1` cujo filho também
/// mede a `100%` tinha min-content GIGANTE (o filho mede-se à custa do pai
/// que ainda não tem largura) e o piso erguia o item bem acima do `width`
/// pedido — em `flex-aspect-ratio-resize-001` (WPT) isso encravava o
/// rasterizador tentando um canvas do tamanho desse min-content.
pub(in crate::layout) fn min_automatico(
    dom: &Dom,
    id: NodeIdx,
    min_content: f32,
    ccss: &ComputedStyle,
    content_w: f32,
    font_size: f32,
    ctx: &LayoutCtx,
) -> f32 {
    let font = font_px(ccss, font_size);
    let rc = ResolveCtx {
        parent_content_w: content_w,
        node_font_size: font,
        root_font_size: crate::style::root_font_size(),
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    match ccss.width.and_then(|d| d.resolve(&rc)) {
        Some(_) => min_content.min(child_outer_width(dom, id, content_w, font_size, ctx)),
        None => min_content,
    }
}
