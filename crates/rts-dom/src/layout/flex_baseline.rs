//! `align-items: baseline` / `align-self: baseline` no flex (Flexbox §8.5) —
//! o gancho que `flex.rs` chama para não crescer além do tecto do crate. A
//! ordem das linhas sob `flex-wrap: wrap-reverse` mudou-se para
//! `eixos_flex::wrap_reverse_efetivo` no lote `flex-writing-mode`, que
//! combina o `wrap-reverse` declarado com o sentido físico do eixo sob
//! `writing-mode` — a pergunta deixou de ser só do `flex_wrap` do CSS.
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
//! O CROSS-SIZE de uma linha com grupo baseline não é o maior item — é o
//! ENVELOPE (Flexbox §9.4 passo 8): `max_ascent + max_descent`, onde
//! `descent_i = altura_outer_i − ascent_i`. Um item de ascent grande e
//! descent pequeno ao lado de um de ascent pequeno e descent grande somam
//! mais do que qualquer um dos dois sozinho — exatamente o padrão de
//! `multiline-reverse-wrap-baseline` (`#third`, `margin-top:5px`, empurra o
//! grupo 5px para baixo SEM alterar a altura de `#fourth`; a linha total
//! cresce os mesmos 5px, não fica do tamanho do maior item). Medido: sem
//! isto a 2ª linha (wrap-reverse) começava 5px cedo demais porque `items_h`
//! só olhava para a altura crua de cada item.
//!
//! CORTES declarados: só a linha (`flex-direction: row`) participa —
//! `coluna.rs::align_offset` trata `Baseline` como `FlexStart`, o fallback
//! que a própria spec prevê quando o eixo cruzado não tem baseline
//! partilhável; um `::before`/`::after` de pseudo-item e um nó de texto
//! solto nunca entram no grupo (a ascent de um pseudo seria a do
//! CONTENTOR, não a dele); `align-content` distribui sobre a ordem já
//! invertida do `wrap-reverse` sem reespelhar `flex-start`/`flex-end`
//! físicos — nenhuma fixture medida exige nenhum dos dois. `last baseline`
//! tem variante PRÓPRIA (`style::AlignItems::LastBaseline`) e nunca entra
//! neste grupo — cai direto no fallback físico de `align_offset`, dito lá.

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

/// O cross-size e os offsets de UMA linha, à luz do grupo
/// `align-items:baseline`/`align-self:baseline` (`None` quando a linha não
/// tem participante nenhum — o caso comum — sem medir ascent algum). Os dois
/// vêm juntos porque nascem da MESMA passada sobre os ascents: separar a
/// altura do offset exigiria medir os ascents duas vezes.
pub(in crate::layout) struct LinhaBaseline {
    pub(in crate::layout) cross_size: f32,
    /// índice N é `Some(offset)` quando o item N participa do grupo; `None`
    /// cai no `align_offset` normal (`off_cross_item`).
    pub(in crate::layout) offsets: Vec<Option<f32>>,
}

pub(in crate::layout) fn calcula_linha(
    dom: &Dom,
    line: &[super::flex::FlexItem],
    align: crate::style::AlignItems,
    content_w: f32,
    ctx: &LayoutCtx,
) -> Option<LinhaBaseline> {
    let participa: Vec<bool> = line
        .iter()
        .map(|it| {
            it.pseudo.is_none()
                && !it.is_text
                && it.align_self.unwrap_or(align) == crate::style::AlignItems::Baseline
        })
        .collect();
    if !participa.iter().any(|&p| p) {
        return None;
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
    let max_descent = line
        .iter()
        .zip(&ascents)
        .zip(&participa)
        .filter(|&(_, &p)| p)
        .map(|((it, &a), _)| (it.h - a).max(0.0))
        .fold(0.0f32, f32::max);
    // `max(..)` com a altura crua (spec §9.4 passo 8, item 1): um item FORA
    // do grupo pode ainda ser mais alto do que o envelope baseline sozinho.
    let items_h = line.iter().fold(0.0f32, |a, it| a.max(it.h));
    let cross_size = (max_ascent + max_descent).max(items_h);
    let offsets = participa
        .iter()
        .zip(&ascents)
        .map(|(&p, &a)| p.then_some(max_ascent - a))
        .collect();
    Some(LinhaBaseline { cross_size, offsets })
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
