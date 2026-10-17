//! The record one item of a WRAPPED column carries between the passes of
//! `column_wrap.rs`: its main size (grows and shrinks per column), its
//! natural cross size, its flex factors and its `auto` main-axis margins.
//!
//! Apart from `column_wrap.rs` because it answers "what is measured about an
//! item before the columns exist", where that file answers "how the columns
//! are formed and placed". Rejected: putting it beside `FlexItem` in
//! `row.rs` or `ColItem` in `column.rs` — the three records answer the same
//! question for three directions, and this lot moves rather than unifies.

use super::*;

/// Um item pré-medido: `main` é a altura outer (cresce/encolhe por coluna,
/// mesmo papel de `ColItem::h` em `column.rs`); `cross` é a largura NATURAL
/// (shrink-to-fit — para um item com `width` explícito é essa largura, sem
/// depender de quantas colunas existem: por isso pode ser medida ANTES de se
/// saber a largura da coluna).
pub(super) struct Item {
    pub(super) node: NodeIdx,
    pub(super) box_id: crate::boxes::BoxId,
    pub(super) main: f32,
    pub(super) cross: f32,
    pub(super) is_text: bool,
    pub(super) grow: f32,
    pub(super) shrink: f32,
    pub(super) min_main: f32,
    pub(super) align_self: Option<crate::style::AlignItems>,
    /// só estica no eixo cruzado quando a LARGURA não é explícita — o mesmo
    /// "`can_stretch`" de `FlexItem` em `row.rs`, só que no eixo trocado.
    pub(super) can_stretch: bool,
    pub(super) order: i32,
    /// margens `auto` no eixo PRINCIPAL (vertical) — mesma leitura de
    /// `ColItem::mt_auto`/`mb_auto` em `column.rs`, que faltava aqui (corte
    /// dito no cabeçalho do módulo até este lote): uma margem `auto` vence o
    /// `justify-content` da COLUNA que a contém (spec §8.1) — sem isto, um
    /// item de `margin-bottom:auto` numa coluna de `flex-wrap` recebia o
    /// mesmo offset `space-around`/`space-between` das colunas SEM margem
    /// `auto`, em vez de ficar encostado ao início com o livre absorvido no
    /// fim (`flexbox-column-row-gap-001`, WPT).
    pub(super) mt_auto: bool,
    pub(super) mb_auto: bool,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn collect_items(
    dom: &Dom,
    tree: &crate::boxes::BoxTree,
    container: crate::boxes::BoxId,
    content_w: f32,
    container_content_h: f32,
    css: &ComputedStyle,
    align: crate::style::AlignItems,
    font_size: f32,
    ctx: &LayoutCtx,
) -> Vec<Item> {
    let mut items: Vec<Item> = Vec::new();
    for &box_id in tree.children_without_generated(container) {
        let Some(child) = tree.node_of(box_id) else {
            continue;
        };
        if let NodeKind::Element { tag } = &dom.node(child).kind {
            if is_non_rendered_tag(tag) {
                continue;
            }
        }
        if is_out_of_flow(dom, child) || is_display_none(dom, child) {
            continue;
        }
        if matches!(dom.node(child).kind, NodeKind::Text(_)) {
            let text = collect_text(dom, child);
            if text.trim().is_empty() {
                continue;
            }
            let h = crate::inline_box::altura_da_linha(css, font_size, ctx.measurer);
            let w = ctx
                .measurer
                .text_width(&text, font_size, false, false, false);
            items.push(Item {
                node: child,
                box_id,
                main: h,
                cross: w,
                is_text: true,
                grow: 0.0,
                shrink: 0.0,
                min_main: h,
                align_self: None,
                can_stretch: false,
                order: 0,
                mt_auto: false,
                mb_auto: false,
            });
            continue;
        }
        let ccss = dom.computed_style_idx(child).unwrap_or_default();
        let stretched = ccss.align_self.unwrap_or(align) == crate::style::AlignItems::Stretch;
        let natural_h = if stretched {
            measure_block(
                dom,
                child,
                box_id,
                content_w,
                Some(container_content_h),
                None,
                None,
                false,
                ctx,
            )
            .1
        } else {
            crate::layout::child_outer_height(
                dom,
                child,
                box_id,
                content_w,
                Some(container_content_h),
                css,
                font_size,
                ctx,
            )
        };
        let child_font = font_px(&ccss, font_size);
        let main = super::column_shrink::base_outer(
            &ccss,
            natural_h,
            Some(container_content_h),
            content_w,
            child_font,
            ctx,
        );
        let child_resolve = ResolveCtx {
            parent_content_w: content_w,
            node_font_size: child_font,
            root_font_size: crate::style::root_font_size(),
            viewport_w: ctx.viewport_w,
            viewport_h: ctx.viewport_h,
        };
        let min_main = super::column_shrink::min_main(
            dom,
            child,
            box_id,
            &ccss,
            natural_h,
            Some(container_content_h),
            &child_resolve,
            ctx,
        );
        let (cross, _) = measure_block(
            dom,
            child,
            box_id,
            content_w,
            Some(container_content_h),
            None,
            None,
            true,
            ctx,
        );
        items.push(Item {
            node: child,
            box_id,
            main,
            cross,
            is_text: false,
            grow: ccss.flex_grow.unwrap_or(0.0),
            shrink: ccss.flex_shrink.unwrap_or(1.0),
            min_main,
            align_self: ccss.align_self,
            can_stretch: ccss.width.is_none(),
            order: ccss.order.unwrap_or(0),
            mt_auto: ccss.margin.top.is_auto(),
            mb_auto: ccss.margin.bottom.is_auto(),
        });
    }
    items.sort_by_key(|it| it.order);
    items
}
