//! `align-items: baseline` / `align-self: baseline` no flex (Flexbox §8.5) e
//! a ordem das LINHAS sob `flex-wrap: wrap-reverse` (§8.3) — os dois ganchos
//! que `flex.rs` chama para não crescer além do tecto do crate.
//!
//! O ascent de um item (topo da MARGIN-BOX até à baseline do seu conteúdo) é
//! `margin-top + ascent_do_item` — [`super::linha_ib::ascent_do_item`] já
//! resolve a distância do topo da BORDER-BOX (borda+padding+meia-entrelinha+
//! ascent da fonte, ou o fundo quando o item não tem conteúdo próprio); só
//! não soma a margem do item, que aqui é conhecida por linha. O grupo
//! baseline de uma linha alinha pelo maior ascent (Flexbox §8.5: "the
//! largest of the baselines... is used"); os outros descem
//! `max_ascent − ascent_i` para partilhar essa baseline.
//!
//! CORTES declarados: só a linha (`flex-direction: row`) participa —
//! `coluna.rs::align_offset` trata `Baseline` como `FlexStart`, o fallback
//! que a própria spec prevê quando o eixo cruzado não tem baseline
//! partilhável; um `::before`/`::after` de pseudo-item e um nó de texto
//! solto nunca entram no grupo (a ascent de um pseudo seria a do
//! CONTENTOR, não a dele); só `first baseline` — o CSS Box Alignment
//! keyword `last baseline` (dois tokens) não é reconhecido pelo parser e
//! cai como declaração inválida, igual a qualquer outro valor desconhecido;
//! `align-content` distribui sobre a ordem já invertida do `wrap-reverse`
//! sem reespelhar `flex-start`/`flex-end` físicos — nenhuma fixture medida
//! exige nenhum dos dois.

use super::*;

/// O ascent de um item de flex — do topo da sua MARGIN-BOX à baseline do seu
/// conteúdo — para o grupo baseline de uma linha. `outer_h` é a altura OUTER
/// já resolvida do item (`FlexItem::h`, pós grow/shrink).
fn ascent_do_item_flex(dom: &Dom, node: NodeIdx, outer_h: f32, content_w: f32, ctx: &LayoutCtx) -> f32 {
    let css = dom.computed_style_idx(node).unwrap_or_default();
    let font = font_px(&css, DEFAULT_FONT_SIZE);
    let resolve = ResolveCtx {
        parent_content_w: content_w,
        node_font_size: font,
        root_font_size: crate::style::root_font_size(),
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    let mt = css.margin.top.resolve(&resolve).unwrap_or(0.0);
    let mb = css.margin.bottom.resolve(&resolve).unwrap_or(0.0);
    let border_h = (outer_h - mt - mb).max(0.0);
    mt + super::linha_ib::ascent_do_item(dom, node, border_h, content_w, ctx)
}

/// Os offsets cruzados de `align-items:baseline`/`align-self:baseline` para
/// UMA linha: índice N é `Some(offset)` quando o item N participa do grupo
/// (align resolvido == `Baseline`, não é pseudo nem texto solto), `None`
/// senão — o chamador cai no `align_offset` normal nesse caso. Devolve tudo
/// `None` sem medir ascent nenhum quando a linha não tem participante (o
/// caso comum: a maioria das linhas não usa baseline).
pub(in crate::layout) fn offsets_da_linha(
    dom: &Dom,
    line: &[super::flex::FlexItem],
    align: crate::style::AlignItems,
    content_w: f32,
    ctx: &LayoutCtx,
) -> Vec<Option<f32>> {
    let participa: Vec<bool> = line
        .iter()
        .map(|it| {
            it.pseudo.is_none()
                && !it.is_text
                && it.align_self.unwrap_or(align) == crate::style::AlignItems::Baseline
        })
        .collect();
    if !participa.iter().any(|&p| p) {
        return vec![None; line.len()];
    }
    let ascents: Vec<f32> = line
        .iter()
        .map(|it| ascent_do_item_flex(dom, it.node, it.h, content_w, ctx))
        .collect();
    let max_ascent = ascents
        .iter()
        .zip(&participa)
        .filter(|&(_, &p)| p)
        .map(|(&a, _)| a)
        .fold(0.0f32, f32::max);
    participa
        .iter()
        .zip(&ascents)
        .map(|(&p, &a)| p.then_some(max_ascent - a))
        .collect()
}

/// O offset cruzado final de um item: stretch vence tudo (mesma prioridade
/// de antes); senão, um baseline PRÓPRIO vence o `align_offset` genérico.
pub(in crate::layout) fn off_cross_item(
    stretches: bool,
    align: crate::style::AlignItems,
    baseline: Option<f32>,
    line_h: f32,
    item_h: f32,
) -> f32 {
    if stretches {
        return 0.0;
    }
    match (align, baseline) {
        (crate::style::AlignItems::Baseline, Some(o)) => o,
        _ => align_offset(align, line_h, item_h),
    }
}

/// `flex-wrap: wrap-reverse` troca cross-start/cross-end (Flexbox §8.3): o
/// agrupamento em linhas é o de `wrap` (já feito por quem chama); só a ORDEM
/// em que elas se empilham no eixo cruzado inverte — a linha que o
/// documento escreve DEPOIS desenha-se no INÍCIO do eixo cruzado.
pub(in crate::layout) fn reverte_linhas_se_wrap_reverse(
    lines: &mut [Vec<super::flex::FlexItem>],
    wrap: Option<crate::style::FlexWrap>,
) {
    if wrap == Some(crate::style::FlexWrap::WrapReverse) {
        lines.reverse();
    }
}
