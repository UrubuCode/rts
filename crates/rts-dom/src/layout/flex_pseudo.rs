//! `::before`/`::after` de um contentor FLEX são itens flex (CSS Flexbox §4:
//! "each in-flow child, including generated content, becomes a flex item").
//!
//! É o caret `.dropdown-toggle::after` do Bootstrap dentro de um `<button
//! class="d-flex">`: um inline-block só de bordas que não entrava na linha de
//! itens, e o botão media 42 onde o Blink dá 54 (`claude-pseudo-item-flex`).
//! Um pseudo-elemento não é um nó, e o `layout_block` não lhe dá caixa; o que
//! um item gerado precisa é pouco — a sua largura e altura (conteúdo, padding,
//! borda, margem) e uma pintura de fundo, bordas e texto — e é isso que vive
//! aqui, fora de `flex.rs`, que está no teto. CORTE dito: `border-radius`,
//! `flex-basis` e `min/max-width` do pseudo não entram; o texto não quebra.

use super::*;

/// Um item gerado já medido: a caixa OUTER (com margens) que ocupa na linha.
pub(in crate::layout) struct PseudoItem {
    pub(in crate::layout) caixa: crate::pseudo::PseudoBox,
    pub(in crate::layout) w: f32,
    pub(in crate::layout) h: f32,
    ml: f32,
    mr: f32,
    mt: f32,
    mb: f32,
    /// as quatro arestas (borda + padding) por lado: cima, direita, baixo, esquerda
    arestas: [f32; 4],
    texto: String,
    fonte: f32,
}

fn rc(css: &ComputedStyle, base_w: f32, fonte: f32, ctx: &LayoutCtx) -> ResolveCtx {
    ResolveCtx {
        parent_content_w: base_w,
        node_font_size: fonte,
        root_font_size: crate::style::root_font_size(),
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    }
}

/// Mede o pseudo-elemento `pe` do contentor `id` como item flex, se existir
/// (tem `content`) e não for `display: none`.
pub(in crate::layout) fn medir(
    dom: &Dom,
    id: NodeIdx,
    pe: crate::style::PseudoElement,
    base_w: f32,
    font_size: f32,
    ctx: &LayoutCtx,
) -> Option<PseudoItem> {
    let caixa = dom.pseudo_box(id, pe)?;
    if caixa.css.effective_display() == Some(crate::style::DisplayKind::None) {
        return None;
    }
    let css = &caixa.css;
    let fonte = font_px(css, font_size);
    let r = rc(css, base_w, fonte, ctx);
    let [bt, br, bb, bl] = crate::style::borders::used_widths(css);
    let p = &css.padding;
    let (pl, pr) = (p.left.resolve(&r).unwrap_or(0.0), p.right.resolve(&r).unwrap_or(0.0));
    let (pt, pb) = (p.top.resolve(&r).unwrap_or(0.0), p.bottom.resolve(&r).unwrap_or(0.0));
    let m = &css.margin;
    let (ml, mr) = (m.left.resolve(&r).unwrap_or(0.0), m.right.resolve(&r).unwrap_or(0.0));
    let (mt, mb) = (m.top.resolve(&r).unwrap_or(0.0), m.bottom.resolve(&r).unwrap_or(0.0));
    let texto = super::segmento::collapse_ws(&caixa.texto, false).into_owned();
    let mono = css.font_family.as_deref().is_some_and(crate::style::is_mono_family);
    let bold = css.bold.unwrap_or(false);
    let tw = if texto.is_empty() {
        0.0
    } else {
        ctx.measurer.text_width_family(&texto, fonte, css.font_family.as_deref(), mono, bold, false)
    };
    let conteudo_w = css.width.and_then(|d| d.resolve(&r)).unwrap_or(tw);
    let conteudo_h = css.height.and_then(|d| d.resolve(&r)).unwrap_or(if texto.is_empty() {
        0.0
    } else {
        crate::inline_box::altura_da_linha(css, fonte, ctx.measurer)
    });
    let (w, h) = if css.border_box.unwrap_or(false) && (css.width.is_some() || css.height.is_some()) {
        (
            css.width.map_or(conteudo_w + pl + pr + bl + br, |_| conteudo_w),
            css.height.map_or(conteudo_h + pt + pb + bt + bb, |_| conteudo_h),
        )
    } else {
        (conteudo_w + pl + pr + bl + br, conteudo_h + pt + pb + bt + bb)
    };
    Some(PseudoItem {
        caixa,
        w: w + ml + mr,
        h: h + mt + mb,
        ml,
        mr,
        mt,
        mb,
        arestas: [bt + pt, br + pr, bb + pb, bl + pl],
        texto,
        fonte,
    })
}

/// A largura OUTER que o pseudo `pe` acrescenta à largura intrínseca de um
/// contentor flex em linha (zero se não existe).
pub(in crate::layout) fn largura(
    dom: &Dom,
    id: NodeIdx,
    pe: crate::style::PseudoElement,
    font_size: f32,
    ctx: &LayoutCtx,
) -> f32 {
    medir(dom, id, pe, ctx.viewport_w, font_size, ctx).map_or(0.0, |p| p.w)
}

/// Pinta o item gerado com o canto superior esquerdo da sua margin box em
/// (`x`, `y`): fundo, as quatro barras de borda, o texto.
pub(in crate::layout) fn pintar(list: &mut DisplayList, item: &PseudoItem, x: f32, y: f32, ctx: &LayoutCtx) {
    let css = &item.caixa.css;
    let r = Rect::new(x + item.ml, y + item.mt, item.w - item.ml - item.mr, item.h - item.mt - item.mb);
    if let Some(bg) = css.bg {
        list.items.push(DisplayItem::SolidRect { rect: r, color: bg, radius: Corners::ZERO });
    }
    let sides = crate::style::borders::resolved_sides(css);
    let [bt, br, bb, bl] = crate::style::borders::used_widths(css);
    let barras = [
        (Rect::new(r.x, r.y, r.w, bt), sides[0]),
        (Rect::new(r.x + r.w - br, r.y, br, r.h), sides[1]),
        (Rect::new(r.x, r.y + r.h - bb, r.w, bb), sides[2]),
        (Rect::new(r.x, r.y, bl, r.h), sides[3]),
    ];
    for (rect, side) in barras {
        if side.paints() && side.color & 0xFF != 0 {
            list.items.push(DisplayItem::SolidRect { rect, color: side.color, radius: Corners::ZERO });
        }
    }
    if !item.texto.is_empty() {
        let mono = css.font_family.as_deref().is_some_and(crate::style::is_mono_family);
        let lh = crate::inline_box::altura_da_linha(css, item.fonte, ctx.measurer);
        let conteudo = crate::inline_box::altura_do_conteudo(item.fonte, ctx.measurer);
        list.items.push(DisplayItem::Text {
            x: r.x + item.arestas[3],
            y: r.y + item.arestas[0] + (lh - conteudo) / 2.0,
            text: item.texto.clone().into(),
            color: css.color.unwrap_or(0x000000FF),
            size: item.fonte,
            mono,
            bold: css.bold.unwrap_or(false),
            italic: false,
            letter_spacing: css.letter_spacing.unwrap_or(0.0),
            decoration: 0,
        });
    }
}

/// O item flex de um pseudo-elemento gerado do contentor, se existir.
pub(in crate::layout) fn item_flex(dom: &Dom, id: NodeIdx, pe: crate::style::PseudoElement, content_w: f32, font_size: f32, ctx: &LayoutCtx) -> Option<super::flex::FlexItem> {
    let p = medir(dom, id, pe, content_w, font_size, ctx)?;
    let css = &p.caixa.css;
    Some(super::flex::FlexItem {
        node: id,
        base: p.w,
        main: p.w,
        h: p.h,
        is_text: false,
        grow: css.flex_grow.unwrap_or(0.0),
        shrink: css.flex_shrink.unwrap_or(1.0),
        align_self: css.align_self,
        order: css.order.unwrap_or(0),
        can_stretch: false,
        min_main: p.w,
        max_main: None,
        auto_esq: false,
        auto_dir: false,
        auto_topo: false,
        auto_fundo: false,
        pseudo: Some(p),
    })
}
