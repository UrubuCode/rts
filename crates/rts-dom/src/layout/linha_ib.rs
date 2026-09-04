use super::*;

/// Pinta uma CORRIDA de inline-blocks consecutivos (botões/pills irmãos) numa
/// sequência de linhas horizontais: mede cada um (shrink, numa lista descartável),
/// põe lado a lado enquanto cabe na `content_w`, quebra linha quando enche, e
/// alinha CADA linha pelo `text-align` do pai (center do google centra os botões).
/// Devolve o novo `y` (abaixo da última linha). Vazio → devolve `y`.
#[allow(clippy::too_many_arguments)]
pub(in crate::layout) fn layout_inline_block_line(
    dom: &Dom,
    run: &[NodeIdx],
    content_x: f32,
    y: f32,
    content_w: f32,
    avail_h: Option<f32>,
    parent_css: &ComputedStyle,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) -> f32 {
    // 1) mede a largura+altura desejada (shrink) de cada item numa lista descartável.
    let mut sizes: Vec<(NodeIdx, f32, f32)> = Vec::with_capacity(run.len());
    for &child in run {
        let (w, h) = measure_block(dom, child, content_w, avail_h, None, None, true, ctx);
        sizes.push((child, w, h));
    }
    // 2) agrupa em LINHAS (soma das larguras ≤ content_w). Cada linha guarda os
    //    itens + a largura total (p/ o alinhamento).
    let mut lines: Vec<(Vec<(NodeIdx, f32, f32)>, f32)> = Vec::new();
    let mut cur: Vec<(NodeIdx, f32, f32)> = Vec::new();
    let mut cur_w = 0.0f32;
    for (child, w, h) in sizes {
        if !cur.is_empty() && cur_w + w > content_w {
            lines.push((std::mem::take(&mut cur), cur_w));
            cur_w = 0.0;
        }
        cur_w += w;
        cur.push((child, w, h));
    }
    if !cur.is_empty() {
        lines.push((cur, cur_w));
    }
    // 3) pinta cada linha: x inicial pelo text-align do pai, itens lado a lado;
    //    y avança pela ALTURA da linha (o item mais alto).
    let mut cy = y;
    for (items, line_w) in &lines {
        let free = (content_w - line_w).max(0.0);
        let mut x = match parent_css.text_align {
            Some(crate::style::TextAlign::Center) => content_x + free / 2.0,
            Some(crate::style::TextAlign::Right) => content_x + free,
            _ => content_x,
        };
        // `line_h` tem de ser conhecida ANTES de posicionar, porque é contra ela
        // que o `vertical-align` alinha — daí a passada de altura separada.
        let line_h = items.iter().fold(0.0f32, |acc, &(_, _, h)| acc.max(h));
        for &(child, w, h) in items {
            // `vertical-align`: a caixa desce dentro da altura da linha. O default
            // (`baseline`, e o não-declarado) mantém o topo, que é o que este
            // motor sempre fez — ver o corte em `style::text::VerticalAlign`.
            let dy = match dom.computed_style_idx(child).and_then(|c| c.vertical_align) {
                Some(crate::style::VerticalAlign::Middle) => (line_h - h) / 2.0,
                Some(crate::style::VerticalAlign::Bottom) => line_h - h,
                _ => 0.0,
            };
            layout_block(
                dom,
                child,
                x,
                cy + dy,
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
        cy += line_h;
    }
    cy
}
