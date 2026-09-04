use super::*;
use crate::style::VerticalAlign;

/// Pinta uma CORRIDA de inline-blocks consecutivos (botões/pills irmãos) numa
/// sequência de linhas horizontais: mede cada um (shrink, numa lista descartável),
/// põe lado a lado enquanto cabe na `content_w`, quebra linha quando enche, e
/// alinha CADA linha pelo `text-align` do pai (center do google centra os botões).
/// Devolve o novo `y` (abaixo da última linha). Vazio → devolve `y`.
///
/// O alinhamento VERTICAL de cada item dentro da linha é o modelo de baseline
/// de `alinhamento_vertical` — os oito valores de `vertical-align`, não só
/// `middle`/`bottom`. `font_size` entra por isso: é o que os deslocamentos de
/// `sub`/`super`/`middle` escalam, e o que `text-top`/`text-bottom` pedem ao
/// medidor.
#[allow(clippy::too_many_arguments)]
pub(in crate::layout) fn layout_inline_block_line(
    dom: &Dom,
    run: &[NodeIdx],
    content_x: f32,
    y: f32,
    content_w: f32,
    avail_h: Option<f32>,
    parent_css: &ComputedStyle,
    font_size: f32,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) -> f32 {
    // 1) mede a largura+altura desejada (shrink) de cada item numa lista descartável,
    //    junto com o `vertical-align` dele — `None` (não declarado) continua a
    //    alinhar pelo TOPO, o corte que o doc do módulo de alinhamento explica.
    let mut sizes: Vec<(NodeIdx, f32, f32, Option<VerticalAlign>)> = Vec::with_capacity(run.len());
    for &child in run {
        let (w, h) = measure_block(dom, child, content_w, avail_h, None, None, true, ctx);
        let valign = dom.computed_style_idx(child).and_then(|c| c.vertical_align);
        sizes.push((child, w, h, valign));
    }
    // 2) agrupa em LINHAS (soma das larguras ≤ content_w). Cada linha guarda os
    //    itens + a largura total (p/ o alinhamento).
    type Item = (NodeIdx, f32, f32, Option<VerticalAlign>);
    let mut lines: Vec<(Vec<Item>, f32)> = Vec::new();
    let mut cur: Vec<Item> = Vec::new();
    let mut cur_w = 0.0f32;
    for item in sizes {
        let w = item.1;
        if !cur.is_empty() && cur_w + w > content_w {
            lines.push((std::mem::take(&mut cur), cur_w));
            cur_w = 0.0;
        }
        cur_w += w;
        cur.push(item);
    }
    if !cur.is_empty() {
        lines.push((cur, cur_w));
    }
    // 3) pinta cada linha: x inicial pelo text-align do pai, itens lado a lado;
    //    y avança pela ALTURA do envelope (baseline + os que a estendem).
    let mut cy = y;
    for (items, line_w) in &lines {
        let free = (content_w - line_w).max(0.0);
        let mut x = match parent_css.text_align {
            Some(crate::style::TextAlign::Center) => content_x + free / 2.0,
            Some(crate::style::TextAlign::Right) => content_x + free,
            _ => content_x,
        };
        // `env` tem de ser conhecido ANTES de posicionar — é contra ele que o
        // `vertical-align` alinha — daí a passada de envelope separada.
        //
        // Um item SEM `vertical-align` entra com `Top` (deslocamento zero,
        // não contribui para a extensão ACIMA/ABAIXO da baseline) em vez de
        // `Baseline`: é o corte declarado no doc do módulo, e é o que fecha o
        // envelope no mesmo valor que o `max(alturas)` antigo dava quando
        // nada na linha declara `vertical-align`.
        let atomos: Vec<(f32, VerticalAlign)> = items
            .iter()
            .map(|&(_, _, h, va)| (h, va.unwrap_or(VerticalAlign::Top)))
            .collect();
        let env = envelope(&atomos, font_size, ctx.measurer);
        for &(child, w, h, va) in items {
            let valign = va.unwrap_or(VerticalAlign::Top);
            let item_y = topo_do_item(valign, h, cy, &env, font_size, ctx.measurer);
            layout_block(
                dom,
                child,
                x,
                item_y,
                content_w,
                avail_h,
                None,
                None,
                true,
                // Corrida de inline-blocks irmãos: mesma razão da linha, ver
                // `linha.rs`.
                &BlockFormattingContext::new(),
                ctx,
                list,
            );
            x += w;
        }
        cy += env.altura();
    }
    cy
}
