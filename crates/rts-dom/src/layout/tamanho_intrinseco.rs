//! The min-content/max-content question, asked WITH AN AXIS — `PLAN.md` §11,
//! lot INTR.
//!
//! ## What was there before this file
//!
//! Only the INLINE axis had the pair: `table::min_content` (min-content width)
//! and `medida::intrinsic_content_width` (max-content width), each its own
//! function, neither knowing the other exists as "the same question on a
//! different axis". The BLOCK axis had no equivalent at all — every site that
//! needed an intrinsic height improvised, and the improvisation in general use
//! is `coluna_shrink::altura_conteudo_sem_height`: it sums each child's own
//! outer height, which is a model of children STACKED. Measured against the
//! Blink on 2026-09-16 (`PLAN.md` §11, INTR): three inline-blocks that share a
//! line come out 22px there and 40 (the sum) here, and three floats side by
//! side come out 16px there and 40 here.
//!
//! ## The one new thing this file adds
//!
//! [`intrinsic_size`] takes an [`Axis`] instead of being two functions with
//! "width" and "height" in their names. On the INLINE axis it does not
//! recompute anything — it calls the two functions above and returns exactly
//! what they return, because widening either of them must widen this too
//! rather than drifting into a second implementation.
//!
//! On the BLOCK axis it is genuinely new, and it is not free: CSS Sizing 3
//! only defines a block-axis intrinsic size THROUGH layout at a given inline
//! size — a block's height depends on where its content wraps, and wrapping
//! depends on the width it is laid out at (`PLAN.md` §11 names this
//! dependency directly, as the reason IFC/LOG come before it in the plan).
//! So [`intrinsic_size`] takes `inline_size: Option<f32>` and answers `None`
//! on the block axis when the caller has no width to give — that is the
//! honest answer this signature can give, not a guess dressed as one.
//!
//! Given a width, the answer is the REAL height a full [`super::measure_block`]
//! pass gives — the same machinery [`super::medida::child_outer_height`]
//! already trusts for the cross-axis of a flex item, which is why it does not
//! divide by min/max: this engine does not distinguish a min-content block
//! size from a max-content one (`coluna_shrink::min_main` already says as
//! much for `min-height:auto`, and normal-flow content has no block-axis
//! compressibility to make the two differ the way width does under a narrow
//! `nowrap`-free word). What changes versus the stacking approximation is
//! that `measure_block` runs the REAL layout — inline-blocks that end up on
//! one line, or floats that sit side by side, come out as one line's height,
//! not a sum of each child's own.
//!
//! ## What this file does NOT do
//!
//! It does not thread a fifth parameter through `vertical.rs`/`bloco.rs` to
//! reach every existing improvisation — that exact move (`avail_h` through
//! five signatures) was tried, MEASURED and REVERTED elsewhere in this crate
//! (`containing_block.rs`, header). Nothing here replaces
//! `altura_conteudo_sem_height`'s call sites; this lot delivers the question,
//! with the number now demonstrably closer to what a real layout gives, and
//! leaves converting a call site to a follow-up that can measure that
//! conversion on its own.

use super::*;
use crate::style::Axis;

/// Which of the two questions CSS Sizing 3 §2.1 asks: the smallest size a box
/// can take without overflowing its own content, or the size it would take
/// with no constraint at all.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(in crate::layout) enum IntrinsicKind {
    Min,
    Max,
}

/// The min-content/max-content size of `id` on `axis`.
///
/// - `Axis::Inline`: exactly [`table::min_content`](crate::table::min_content)
///   for `Min`, exactly
///   [`medida::intrinsic_content_width`](super::medida::intrinsic_content_width)
///   for `Max`. `inline_size` is ignored — the inline axis never needed a
///   width to measure against, that is what makes it intrinsic.
/// - `Axis::Block`: `inline_size` is the width to lay `id` out at, because a
///   block's height cannot be answered without one. `None` in, `None` out —
///   refusing to guess is the answer, not an omission. `Min` and `Max`
///   currently answer the SAME value on this axis (see the module doc for
///   why that is not a shortcut taken here but a limit of the engine already
///   documented elsewhere).
pub(in crate::layout) fn intrinsic_size(
    dom: &Dom,
    id: NodeIdx,
    axis: Axis,
    kind: IntrinsicKind,
    inline_size: Option<f32>,
    font: f32,
    ctx: &LayoutCtx,
) -> Option<f32> {
    match axis {
        Axis::Inline => Some(match kind {
            IntrinsicKind::Min => crate::table::min_content(dom, id, font, ctx),
            IntrinsicKind::Max => super::medida::intrinsic_content_width(dom, id, font, ctx),
        }),
        Axis::Block => {
            let w = inline_size?;
            let (_, outer_h) = super::measure_block(dom, id, w, None, None, None, true, ctx);
            Some(outer_h)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::table::tests::geometria;

    fn ctx() -> LayoutCtx<'static> {
        LayoutCtx {
            viewport_w: 1280.0,
            viewport_h: 600.0,
            measurer: &crate::layout::medida::ApproxMeasurer,
        }
    }

    fn id_de(dom: &crate::Dom, sel: &str) -> NodeIdx {
        let ids = dom.query_all(sel);
        let id = ids.first().unwrap_or_else(|| panic!("sem {sel}"));
        dom.resolve(*id).expect("no vivo")
    }

    // ---- the inline axis: must not move ----

    /// `Axis::Inline` + `Max` answers exactly what `intrinsic_content_width`
    /// answers directly, for a container whose max-content is a SUM (two
    /// row-flex children side by side) — the case the module doc names as
    /// the one a stacking model gets wrong on the other axis.
    #[test]
    fn max_content_no_eixo_inline_e_a_soma_de_dois_filhos_lado_a_lado() {
        const HTML: &str = r#"<style>#f{display:flex}</style>
<div id="f"><div style="width:50px"></div><div style="width:70px"></div></div>"#;
        let (dom, _list) = geometria(HTML, 1280.0);
        let id = id_de(&dom, "#f");
        let c = ctx();
        let direto = super::super::medida::intrinsic_content_width(&dom, id, 16.0, &c);
        let via_eixo =
            intrinsic_size(&dom, id, Axis::Inline, IntrinsicKind::Max, None, 16.0, &c);
        assert_eq!(via_eixo, Some(direto), "a pergunta com eixo nao pode mudar o numero antigo");
        assert!((direto - 120.0).abs() < 0.5, "50+70 lado a lado: {direto}");
    }

    /// `Axis::Inline` + `Min` answers exactly what `table::min_content`
    /// answers directly.
    #[test]
    fn min_content_no_eixo_inline_bate_a_funcao_antiga() {
        const HTML: &str = r#"<div id="p">um texto razoavelmente longo aqui</div>"#;
        let (dom, _list) = geometria(HTML, 1280.0);
        let id = id_de(&dom, "#p");
        let c = ctx();
        let direto = crate::table::min_content(&dom, id, 16.0, &c);
        let via_eixo =
            intrinsic_size(&dom, id, Axis::Inline, IntrinsicKind::Min, None, 16.0, &c);
        assert_eq!(via_eixo, Some(direto), "a pergunta com eixo nao pode mudar o numero antigo");
    }

    // ---- the block axis: honest refusal, and the real answer when given a width ----

    /// Without an `inline_size`, the block axis answers `None` rather than a
    /// guess — the signature's whole point.
    #[test]
    fn sem_largura_o_eixo_de_bloco_recusa_em_vez_de_adivinhar() {
        const HTML: &str = r#"<div id="c"><div style="height:20px"></div></div>"#;
        let (dom, _list) = geometria(HTML, 1280.0);
        let id = id_de(&dom, "#c");
        let c = ctx();
        let r = intrinsic_size(&dom, id, Axis::Block, IntrinsicKind::Max, None, 16.0, &c);
        assert_eq!(r, None);
    }

    /// The case the module doc opens with: two inline-blocks that share ONE
    /// line. The stacking approximation (`altura_conteudo_sem_height`) sums
    /// their heights (20+40=60); a real layout at a width wide enough for
    /// both to sit side by side gives the line's own height, close to the
    /// TALLER child (40) and strictly less than the stacked sum.
    #[test]
    fn eixo_de_bloco_de_dois_inline_block_na_mesma_linha_nao_e_a_soma() {
        const HTML: &str = r#"<div id="c" style="width:200px"><span style="display:inline-block;width:10px;height:20px"></span><span style="display:inline-block;width:10px;height:40px"></span></div>"#;
        let (dom, _list) = geometria(HTML, 1280.0);
        let id = id_de(&dom, "#c");
        let c = ctx();
        let via_eixo =
            intrinsic_size(&dom, id, Axis::Block, IntrinsicKind::Max, Some(200.0), 16.0, &c)
                .expect("com largura, o eixo de bloco responde");
        let css = dom.computed_style_idx(id).unwrap_or_default();
        let soma_empilhada = super::super::coluna_shrink::altura_conteudo_sem_height(
            &dom, id, &css, 200.0, 16.0, &c,
        );
        assert!(
            (soma_empilhada - 60.0).abs() < 0.5,
            "a aproximacao empilhada soma os dois: {soma_empilhada}"
        );
        assert!(
            via_eixo < soma_empilhada - 5.0,
            "a resposta real (mesma linha) tem de ficar bem abaixo da soma empilhada: real={via_eixo} soma={soma_empilhada}"
        );
        assert!(
            via_eixo >= 40.0 - 0.5,
            "a linha nao pode ser mais baixa que o maior filho: {via_eixo}"
        );
    }

    /// And a block with `Min` and `Max` on the block axis answer the SAME
    /// number, given the same width — the documented limit, pinned so a
    /// future change that starts distinguishing them is a decision and not
    /// an accident.
    #[test]
    fn no_eixo_de_bloco_min_e_max_respondem_o_mesmo_hoje() {
        const HTML: &str = r#"<div id="c" style="width:200px"><div style="height:33px"></div></div>"#;
        let (dom, _list) = geometria(HTML, 1280.0);
        let id = id_de(&dom, "#c");
        let c = ctx();
        let min = intrinsic_size(&dom, id, Axis::Block, IntrinsicKind::Min, Some(200.0), 16.0, &c);
        let max = intrinsic_size(&dom, id, Axis::Block, IntrinsicKind::Max, Some(200.0), 16.0, &c);
        assert_eq!(min, max);
    }
}
