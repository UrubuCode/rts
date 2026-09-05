//! Largura `max-content`/shrink-to-fit de um `display:flex;flex-direction:
//! column;flex-wrap:wrap` — que é a SOMA das colunas, nunca o maior item
//! (`medida::intrinsic_content_width` trata coluna sempre como um bloco que
//! empilha, "mesmo com `flex-wrap`", e documenta a multi-coluna como corte —
//! ver o comentário lá; este módulo é o fecho desse corte, à parte porque
//! `intrinsic_content_width` pertence a outro lote em curso, "largura de
//! controlos", e não pode crescer).
//!
//! **Sem altura definida (`height`/`max-height`), não há limiar de quebra**
//! (`coluna_wrap.rs`/`bloco.rs::wrap_definite_h`) — o contentor nunca chega a
//! ter mais de uma coluna, então a soma degenera na largura de UM item, que é
//! exatamente o que `intrinsic_content_width` já devolve. Por isso esta
//! função só precisa de agrupar quando o limiar existe; sem ele, delega.
//!
//! O agrupamento aqui é DELIBERADAMENTE mais simples do que
//! `coluna_wrap.rs`: nenhum item aqui ainda tem `flex-grow`/`flex-shrink`
//! resolvido (a largura `max-content` do CONTENTOR precisa de existir ANTES
//! de layoutar os filhos — é o containing block deles), então a base de cada
//! item é a sua altura NATURAL (`child_outer_height`), sem crescer/encolher.
//! É a mesma ordem de dependência que já vale para `content_w` inteiro:
//! `bloco.rs` resolve a largura do container antes de chamar
//! `layout_children_column`/`coluna_wrap.rs`.

use super::*;

/// Largura que o WPT `col-wrap-*` chama de `width:max-content` num flex
/// column-wrap: a soma da largura de cada coluna (o maior item dela) mais o
/// `column-gap` entre colunas — o mesmo PASSO 1/2/4 de `coluna_wrap.rs`, só
/// que medindo em vez de posicionar, e sem grow/shrink (ver o comentário do
/// módulo).
pub(in crate::layout) fn max_content_width(
    dom: &Dom,
    id: NodeIdx,
    font_size: f32,
    avail_h: Option<f32>,
    css: &ComputedStyle,
    ctx: &LayoutCtx,
) -> f32 {
    let fallback = || content_natural_width(dom, id, font_size, ctx);
    let is_column = css.flex_direction.map(|f| f.is_column()).unwrap_or(false);
    let is_wrap = css.flex_wrap == Some(crate::style::FlexWrap::Wrap)
        || css.flex_wrap == Some(crate::style::FlexWrap::WrapReverse);
    if !is_column || !is_wrap {
        return fallback();
    }
    let resolve = ResolveCtx {
        parent_content_w: ctx.viewport_w,
        node_font_size: font_size,
        root_font_size: crate::style::root_font_size(),
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    // Mesmo limiar de `bloco.rs::wrap_definite_h`: `height`/`max-height`
    // resolvidos, NUNCA `min-height` (um piso não é o que decide onde uma
    // coluna "encheu" — ver o comentário lá).
    let wrap_definite_h = resolve_height(css.height, avail_h, &resolve)
        .or_else(|| resolve_height(css.max_height, avail_h, &resolve));
    let Some(container_content_h) = wrap_definite_h else {
        return fallback();
    };
    let main_gap = resolve_height(css.row_gap, Some(container_content_h), &resolve)
        .unwrap_or(0.0)
        .max(0.0);
    let cross_gap = css
        .gap
        .and_then(|d| d.resolve(&resolve))
        .unwrap_or(0.0)
        .max(0.0);

    let mut items: Vec<(f32, f32)> = Vec::new(); // (altura natural, largura natural)
    for &child in &dom.node(id).children {
        if let NodeKind::Element { tag } = &dom.node(child).kind {
            if is_non_rendered_tag(tag) {
                continue;
            }
        }
        if is_out_of_flow(dom, child) || e_display_none(dom, child) {
            continue;
        }
        if matches!(dom.node(child).kind, NodeKind::Text(_)) {
            let text = collect_text(dom, child);
            if text.trim().is_empty() {
                continue;
            }
            let h = crate::inline_box::altura_da_linha(css, font_size, ctx.measurer);
            let w = ctx.measurer.text_width(&text, font_size, false, false, false);
            items.push((h, w));
            continue;
        }
        let ccss = dom.computed_style_idx(child).unwrap_or_default();
        let child_font = font_px(&ccss, font_size);
        let natural_h =
            child_outer_height(dom, child, ctx.viewport_w, Some(container_content_h), css, font_size, ctx);
        let main = super::coluna_shrink::base_outer(
            &ccss,
            natural_h,
            Some(container_content_h),
            ctx.viewport_w,
            child_font,
            ctx,
        );
        let (cross, _) = measure_block(
            dom, child, ctx.viewport_w, Some(container_content_h), None, None, true, ctx,
        );
        items.push((main, cross));
    }
    if items.is_empty() {
        return fallback();
    }

    // Mesmo empacotamento guloso de `coluna_wrap.rs` PASSO 2, sem `order`
    // (a ordem do documento chega a ele já correta para este propósito: só
    // conta QUANTAS colunas nascem e o maior item de cada uma).
    let mut columns: Vec<f32> = vec![0.0]; // largura (maior item) de cada coluna
    let mut col_h = 0.0f32;
    let mut col_w = 0.0f32;
    for (main, cross) in items {
        let with_gap = if col_h > 0.0 { main_gap } else { 0.0 };
        if col_h > 0.0 && col_h + with_gap + main > container_content_h {
            *columns.last_mut().unwrap() = col_w;
            columns.push(0.0);
            col_h = main;
            col_w = cross;
        } else {
            col_h += with_gap + main;
            col_w = col_w.max(cross);
        }
    }
    *columns.last_mut().unwrap() = col_w;

    columns.iter().sum::<f32>() + (columns.len().saturating_sub(1)) as f32 * cross_gap
}
