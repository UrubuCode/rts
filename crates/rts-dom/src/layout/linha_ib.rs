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
    //    y avança pela altura da CAIXA DE LINHA (ver a composição abaixo).
    //
    // O *strut* é o texto do pai: mesmo uma linha só de `inline-block`s carrega
    // a métrica da fonte dele, e é isso que põe descida abaixo da base.
    let font = font_px(parent_css, DEFAULT_FONT_SIZE);
    let conteudo = crate::inline_box::altura_do_conteudo(font, ctx.measurer);
    let meia = crate::inline_box::meia_entrelinha(
        crate::inline_box::altura_da_linha(parent_css, font, ctx.measurer),
        conteudo,
    );
    let mut cy = y;
    for (items, line_w) in &lines {
        let free = (content_w - line_w).max(0.0);
        let mut x = match parent_css.text_align {
            Some(crate::style::TextAlign::Center) => content_x + free / 2.0,
            Some(crate::style::TextAlign::Right) => content_x + free,
            _ => content_x,
        };
        // A caixa de linha compõe-se À VOLTA DA LINHA DE BASE, e não é o máximo
        // das alturas dos itens. Medido no Chrome: duas caixas de 10 e 30 px num
        // `<div>` de 16 px dão uma linha de 34 e não de 30 — o *strut* do texto
        // acrescenta descida POR BAIXO da base, e o `<div>` volta a 30 assim que
        // se põe `line-height: 0`, o que isola a variável.
        //
        // Daí ascent e descent em vez de uma altura só: um item alinhado pela
        // base contribui para a ascent com a sua altura inteira (a linha de base
        // de um `inline-block` é a sua aresta INFERIOR) e nada para a descent.
        let ascent_strut = ctx.measurer.ascent(font) + meia;
        let descent_strut = (conteudo - ctx.measurer.ascent(font)) + meia;
        let mut ascent = ascent_strut;
        let mut descent = descent_strut;
        for &(child, _, h) in items {
            match dom.computed_style_idx(child).and_then(|c| c.vertical_align) {
                // Só a base participa da composição; os outros valores POSICIONAM
                // dentro da linha, e o que eles fazem à altura dela é a fatia
                // seguinte deste trabalho.
                Some(crate::style::VerticalAlign::Middle)
                | Some(crate::style::VerticalAlign::Bottom)
                | Some(crate::style::VerticalAlign::Top) => {}
                _ => {
                    ascent = ascent.max(h);
                    // Um item alinhado pela base não desce abaixo dela — e é isso
                    // que impede a linha de ENCOLHER: com `line-height: 0` a
                    // meia-entrelinha é negativa e a descida do strut fica em
                    // −5,22, o que dava uma linha de 24,78 onde o Chrome dá 30.
                    descent = descent.max(0.0);
                }
            }
        }
        // O `max` com a régua antiga é o que segura `top`/`middle`/`bottom`: eles
        // não entram na composição pela base, mas a linha não pode ser mais baixa
        // do que a caixa deles. Sem isto, dois `vertical-align: top` de 30 px
        // davam uma linha de 18.
        let line_h = (ascent + descent).max(items.iter().fold(0.0f32, |a, &(_, _, h)| a.max(h)));
        // `line_h_itens` é a régua ANTIGA — o máximo das alturas — e fica aqui
        // porque `middle` e `bottom` continuam a alinhar contra ela. Medidos, os
        // dois erram (Chrome dá 21,42 e 24 onde damos 10 e 20) e a causa é esta
        // mesma: a caixa de linha não é aquilo. Corrigi-los muda-os de sítio e
        // este lote é o `baseline` sozinho, medido sem nada ao lado.
        let line_h_itens = items.iter().fold(0.0f32, |acc, &(_, _, h)| acc.max(h));
        for &(child, w, h) in items {
            // `vertical-align`: onde a caixa assenta dentro da linha.
            let dy = match dom.computed_style_idx(child).and_then(|c| c.vertical_align) {
                Some(crate::style::VerticalAlign::Middle) => (line_h_itens - h) / 2.0,
                Some(crate::style::VerticalAlign::Bottom) => line_h_itens - h,
                // `top` era o ÚNICO dos oito que este motor acertava, e acertava
                // por cair no ramo neutro: o topo da linha é mesmo 0. Passa a ser
                // explícito porque o ramo neutro deixou de ser 0 — sem esta linha
                // o `top` herdava o `baseline` e a única resposta certa que
                // havia virava errada.
                Some(crate::style::VerticalAlign::Top) => 0.0,
                // `baseline` — o valor INICIAL, e o que toda a página usa. A
                // aresta inferior da caixa assenta na linha de base, portanto
                // duas caixas de alturas diferentes partilham o FUNDO e não o
                // topo. Era o topo que este motor dava, o que faz dele um `top`
                // com outro nome.
                _ => ascent - h,
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
                &[],
                ctx,
                list,
            );
            x += w;
        }
        cy += line_h;
    }
    cy
}
