//! The vertical placement of a LINE THAT HOLDS AN INLINE-BLOCK (CSS 2.1
//! §10.8.1): every item — the text's strut and each atom — has an extent
//! above and below one shared baseline, and the line box is the largest of
//! each side.
//!
//! The text flow (`linha.rs`) did not ask this. It took the line's height as
//! the tallest atom and treated every inline-block as sitting on its bottom
//! edge, with a special case for "taller than the strut and next to text".
//! That gave an empty 20px inline-block a 20px line where Blink gives 25 (the
//! text's descent hangs below the box, whose bottom is on the baseline), a
//! padded one with text 37 where Blink gives 32 (its baseline is its TEXT's,
//! not its bottom), and ignored `vertical-align` entirely
//! (`claude-inline-block-baseline`).
//!
//! The model already existed — [`Envelope`] and
//! [`topo_do_item_com_baseline`] in `alinhamento_vertical.rs`, used by the
//! run of sibling inline-blocks (`linha_ib.rs`) — and its module doc named
//! this migration as the cut still open. This module only feeds it the atoms
//! of a text line. A line with NO inline-block keeps the old path untouched:
//! text and images alone were already right, and moving them is a separate,
//! separately measured change.

use super::alinhamento_vertical::{Envelope, envelope_com_baseline, topo_do_item_com_baseline};
use super::*;
use crate::style::VerticalAlign;

/// One atom of the line as the envelope sees it: its outer height, the
/// distance from its top to ITS baseline, and its `vertical-align`.
struct Atomo {
    altura: f32,
    ascent: f32,
    valign: VerticalAlign,
}

/// The envelope of `line` when it holds an inline-block (a real one or a
/// generated one), `None` otherwise — the caller then keeps its old path.
#[allow(clippy::too_many_arguments)]
pub(in crate::layout) fn envelope_da_linha(
    dom: &Dom,
    line: &[Segment],
    font_size: f32,
    line_height: f32,
    family: Option<&str>,
    content_w: f32,
    ctx: &LayoutCtx,
) -> Option<Envelope> {
    if !line.iter().any(|s| e_inline_block(s)) {
        return None;
    }
    let itens: Vec<(f32, f32, VerticalAlign)> = line
        .iter()
        .filter_map(|s| atomo(dom, s, content_w, ctx))
        .map(|a| (a.altura, a.ascent, a.valign))
        .collect();
    Some(envelope_com_baseline(&itens, font_size, line_height, family, ctx.measurer))
}

/// The top of an atom of a line whose envelope is `env` and whose top is `cy`.
#[allow(clippy::too_many_arguments)]
pub(in crate::layout) fn topo_do_atomo(
    dom: &Dom,
    seg: &Segment,
    cy: f32,
    env: &Envelope,
    font_size: f32,
    family: Option<&str>,
    content_w: f32,
    ctx: &LayoutCtx,
) -> f32 {
    match atomo(dom, seg, content_w, ctx) {
        Some(a) => topo_do_item_com_baseline(a.valign, a.altura, a.ascent, cy, env, font_size, family, ctx.measurer),
        None => cy,
    }
}

fn e_inline_block(seg: &Segment) -> bool {
    matches!(
        seg.atomic,
        Some((_, _, AtomicKind::Block | AtomicKind::Gerada(_, crate::inline_box::ParteGerada::Atomo)))
    )
}

/// The atom behind a segment, for the atoms that have a body on the line:
/// inline-blocks (real and generated), replaced elements and form widgets.
fn atomo(dom: &Dom, seg: &Segment, content_w: f32, ctx: &LayoutCtx) -> Option<Atomo> {
    let (id, caixa, kind) = seg.atomic?;
    let altura = seg.wh;
    match kind {
        AtomicKind::Block => {
            let css = dom.computed_style_idx(id)?;
            Some(Atomo {
                altura,
                ascent: ascent_do_bloco(dom, id, caixa, &css, altura, seg.ww, content_w, ctx),
                valign: css.vertical_align.unwrap_or(VerticalAlign::Baseline),
            })
        }
        AtomicKind::Gerada(pe, crate::inline_box::ParteGerada::Atomo) => {
            let caixa = dom.pseudo_box(id, pe)?;
            Some(Atomo {
                altura,
                ascent: ascent_do_gerado(&caixa, altura, content_w, ctx),
                valign: caixa.css.vertical_align.unwrap_or(VerticalAlign::Baseline),
            })
        }
        // A replaced element has no baseline of its own: its bottom margin edge
        // sits on the line's (CSS 2.1 §10.8.1), which is where `linha.rs`
        // already puts it.
        AtomicKind::Replaced => Some(Atomo { altura, ascent: altura, valign: VerticalAlign::Baseline }),
        AtomicKind::Widget => Some(Atomo {
            altura,
            ascent: super::linha_ib::ascent_do_item(dom, id, altura, content_w, ctx),
            valign: dom
                .computed_style_idx(id)
                .and_then(|c| c.vertical_align)
                .unwrap_or(VerticalAlign::Baseline),
        }),
        _ => None,
    }
}

/// An inline-block's baseline, measured from the top of its OUTER box: its
/// LAST line box's baseline, or its bottom margin edge when it has no line
/// box or its `overflow` is not `visible` (CSS 2.1 §10.8.1).
///
/// The last line box is found by LAYING THE ATOM OUT in a throwaway list, the
/// way `measure_block` measures a height, and taking the lowest text it
/// painted. `ascent_do_item` answered with a formula over the box's OWN font,
/// which is right only when the box holds one line of its own text: a 14px
/// box holding a 26px line put its baseline 6px high, and the WPT
/// `flexbox-baseline-*` references (inline-blocks) disagreed with their tests
/// (inline-flexes). A flex container keeps `ascent_do_item`: its baseline is
/// its first item's (Flexbox §8.5), not its last line's. The cost is one extra
/// layout of each inline-block on a line that holds one — stated, not measured.
#[allow(clippy::too_many_arguments)]
fn ascent_do_bloco(
    dom: &Dom,
    id: NodeIdx,
    caixa: Option<crate::boxes::BoxId>,
    css: &ComputedStyle,
    altura: f32,
    largura: f32,
    content_w: f32,
    ctx: &LayoutCtx,
) -> f32 {
    let recorta = [css.overflow_x, css.overflow_y].iter().flatten().any(|o| o.clips());
    if recorta {
        return altura;
    }
    let flex = matches!(
        css.effective_display(),
        Some(
            crate::style::DisplayKind::Flex
                | crate::style::DisplayKind::FlexWrap
                | crate::style::DisplayKind::InlineFlex
                | crate::style::DisplayKind::InlineFlexWrap
        )
    );
    let fonte = font_px(css, DEFAULT_FONT_SIZE);
    let r = ResolveCtx {
        parent_content_w: content_w,
        node_font_size: fonte,
        root_font_size: crate::style::root_font_size(),
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    let (mt, mb) = (css.margin.top.resolve(&r).unwrap_or(0.0), css.margin.bottom.resolve(&r).unwrap_or(0.0));
    if flex || caixa.is_none() {
        let borda = (altura - mt - mb).max(0.0);
        let dentro = super::linha_ib::ascent_do_item(dom, id, borda, content_w, ctx);
        // `ascent_do_item` answers the whole border box when the box is empty:
        // then the baseline is the bottom MARGIN edge, which is `altura`.
        return if dentro >= borda { altura } else { mt + dentro };
    }
    match baseline_da_ultima_linha(dom, id, caixa.expect("checked above"), largura, content_w, ctx) {
        Some(b) => b.min(altura),
        None => altura,
    }
}

/// The baseline of the lowest line of text an atom paints, from the top of
/// its outer box, or `None` when it paints no text.
fn baseline_da_ultima_linha(
    dom: &Dom,
    id: NodeIdx,
    caixa: crate::boxes::BoxId,
    largura: f32,
    content_w: f32,
    ctx: &LayoutCtx,
) -> Option<f32> {
    let mut scratch = DisplayList::for_dom(dom);
    layout_block(
        dom,
        id,
        Some(caixa),
        0.0,
        0.0,
        content_w,
        None,
        Some(largura),
        None,
        false,
        true,
        &BlockFormattingContext::new(),
        ctx,
        &mut scratch,
    );
    scratch
        .materialized()
        .iter()
        .filter_map(|item| match item {
            DisplayItem::Text { y, size, is_ahem, .. } => {
                let familia = is_ahem.then_some("Ahem");
                Some(y + ctx.measurer.font_ascent_family(*size, familia))
            }
            _ => None,
        })
        .fold(None, |acc: Option<f32>, b| Some(acc.map_or(b, |m| m.max(b))))
}

/// The same for a generated `inline-block` (`::before`/`::after`): its text's
/// first line baseline, or its bottom margin edge when it has no text.
fn ascent_do_gerado(caixa: &crate::pseudo::PseudoBox, altura: f32, content_w: f32, ctx: &LayoutCtx) -> f32 {
    let css = &caixa.css;
    let recorta = [css.overflow_x, css.overflow_y].iter().flatten().any(|o| o.clips());
    if recorta || caixa.texto.trim().is_empty() {
        return altura;
    }
    let fonte = font_px(css, DEFAULT_FONT_SIZE);
    let r = ResolveCtx {
        parent_content_w: content_w,
        node_font_size: fonte,
        root_font_size: crate::style::root_font_size(),
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    let arestas = super::pseudo_caixa::resolve_arestas(css, &r);
    let familia = css.font_family.as_deref();
    let lh = crate::inline_box::altura_da_linha(css, fonte, ctx.measurer);
    let conteudo = crate::inline_box::altura_do_conteudo(fonte, familia, ctx.measurer);
    (arestas.mt + arestas.valores[0] + (lh - conteudo) / 2.0 + ctx.measurer.font_ascent_family(fonte, familia)).min(altura)
}
