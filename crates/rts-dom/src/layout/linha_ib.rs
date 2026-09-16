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
/// A distância do topo de um inline-block à sua BASELINE: com conteúdo, a da
/// primeira linha dele (borda + padding + meia-entrelinha + ascent da fonte
/// dele — a última linha, que a spec pede, é a mesma num item de uma linha, o
/// caso de um botão); vazio, o fundo da margem (a altura toda).
///
/// `pub(in crate::layout)`, não privado: `flex_baseline.rs` reusa esta MESMA
/// distância (do topo da BORDER-BOX à baseline) para o grupo
/// `align-items:baseline` do flex — só soma a margem própria do item, que
/// aqui não entra (`h` já é a altura da border-box, não a outer).
pub(in crate::layout) fn ascent_do_item(dom: &Dom, id: NodeIdx, h: f32, content_w: f32, ctx: &LayoutCtx) -> f32 {
    // Um flex/inline-flex é um CONTENTOR: a sua baseline vista de fora não é
    // a da sua PRÓPRIA fonte (a fórmula abaixo) — é a de `flex_baseline::
    // ascent_do_contentor` (Flexbox §8.5: o grupo baseline da 1ª linha, ou o
    // 1º item em fluxo). Sem este desvio, um `<div class=flexContainer>` com
    // FILHOS ELEMENTO caía em `tem_conteudo_para_fragmento` (tem filhos) e
    // usava a fonte do CONTENTOR — que é a mesma pergunta errada, no mesmo
    // sentido, que a doc de `caixa.rs` descreve para "é de bloco?": um
    // contentor tem baseline PRÓPRIA por definição, um flex não.
    if matches!(
        dom.computed_style_idx(id).and_then(|c| c.effective_display()),
        Some(
            crate::style::DisplayKind::Flex
                | crate::style::DisplayKind::FlexWrap
                | crate::style::DisplayKind::InlineFlex
                | crate::style::DisplayKind::InlineFlexWrap
        )
    ) {
        return super::flex_baseline::ascent_do_contentor(dom, id, h, content_w, ctx);
    }
    // Um controlo de formulário tem texto por dentro mesmo sem filhos (o
    // valor, o rótulo): a baseline dele é a desse texto, não o fundo — senão um
    // `<input>` de 21px puxava a linha e o `<button>` ao lado descia 3,5px
    // (`claude-ua-form-disabled`).
    let controlo = matches!(&dom.node(id).kind,
        NodeKind::Element { tag } if matches!(tag.as_str(), "input" | "button" | "select" | "textarea"));
    if matches!(&dom.node(id).kind, NodeKind::Element { tag } if tag == "textarea") {
        // Blink usa a borda inferior como baseline do textarea replaced, não a
        // linha de texto interna do controle.
        return h;
    }
    if !controlo && !super::caixa::tem_conteudo_para_fragmento(dom, id) {
        return h;
    }
    let Some(css) = dom.computed_style_idx(id) else { return h };
    let font = font_px(&css, DEFAULT_FONT_SIZE);
    let rc = ResolveCtx {
        parent_content_w: content_w,
        node_font_size: font,
        root_font_size: crate::style::root_font_size(),
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    let [bt, ..] = crate::style::borders::used_widths(&css);
    let pt = css.padding.top.resolve(&rc).unwrap_or(0.0);
    let lh = crate::inline_box::altura_da_linha(&css, font, ctx.measurer);
    let conteudo = crate::inline_box::altura_do_conteudo(font, css.font_family.as_deref(), ctx.measurer);
    let ascent = ctx.measurer.font_ascent_family(font, css.font_family.as_deref());
    (bt + pt + (lh - conteudo) / 2.0 + ascent).min(h)
}

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
    let mut sizes: Vec<(NodeIdx, f32, f32, Option<VerticalAlign>, f32, f32)> = Vec::with_capacity(run.len());
    for (pos, &child) in run.iter().enumerate() {
        let (measured_w, h) = measure_block(dom, child, content_w, avail_h, None, None, true, ctx);
        let is_submit = matches!(&dom.node(child).kind, NodeKind::Element { tag } if tag == "input")
            && matches!(dom.node(child).attr("type").map(|t| t.to_ascii_lowercase()).as_deref(), Some("submit" | "button" | "reset"));
        let is_button = matches!(&dom.node(child).kind, NodeKind::Element { tag } if tag == "button");
        // `measure_block` devolve a margem externa dos widgets nativos. A
        // posição inline usa a border-box, que é o rect exposto ao DOM.
        let next_is_button = run.get(pos + 1).is_some_and(|next| matches!(&dom.node(*next).kind, NodeKind::Element { tag } if tag == "button"));
        let w = if is_submit { (measured_w - 6.0).max(0.0) } else if is_button {
            // O botão que antecede outro botão carrega a moldura UA no avanço
            // entre as duas caixas; diante de outro controle, o avanço usado
            // pelo inline layout é a largura border-box medida.
            if next_is_button { measured_w + 1.82 } else { (measured_w - 1.03).max(0.0) }
        } else { measured_w };
        let valign = dom.computed_style_idx(child).and_then(|c| c.vertical_align);
        // A corrida de inline-blocks não carrega os nós de texto entre irmãos.
        // Reconstituir aqui o separador preserva o espaço colapsado do HTML
        // (`input` `input`, botões do Google) sem afetar flex items, que usam
        // outro caminho. Elementos adjacentes sem whitespace continuam colados.
        let gap = if pos == 0 {
            0.0
        } else if let (Some(parent), Some(prev_pos)) = (
            dom.node(child).parent,
            dom.node(child).parent.and_then(|p| dom.node(p).children.iter().position(|&n| n == run[pos - 1])),
        ) {
            let cur_pos = dom.node(parent).children.iter().position(|&n| n == child).unwrap_or(prev_pos + 1);
            if dom.node(parent).children[prev_pos + 1..cur_pos].iter().any(|&n| matches!(&dom.node(n).kind, NodeKind::Text(t) if t.chars().all(crate::inline_box::e_espaco_css))) {
                4.45
            } else { 0.0 }
        } else { 0.0 };
        let (ua_left, ua_right) = match dom.node(child).attr("type").map(|t| t.to_ascii_lowercase()).as_deref() {
            Some("checkbox") | Some("radio") => (4.0, 4.0),
            _ => (0.0, 0.0),
        };
        sizes.push((child, w, h, valign, gap + ua_left, ua_right));
    }
    // 2) agrupa em LINHAS (soma das larguras ≤ content_w). Cada linha guarda os
    //    itens + a largura total (p/ o alinhamento).
    type Item = (NodeIdx, f32, f32, Option<VerticalAlign>, f32, f32);
    let mut lines: Vec<(Vec<Item>, f32)> = Vec::new();
    let mut cur: Vec<Item> = Vec::new();
    let mut cur_w = 0.0f32;
    // `white-space: nowrap`/`pre` no contentor: a corrida não quebra, transborda
    // — a referência de 27 reftests de flexbox do WPT é exactamente isto
    // (`claude-inline-block-nowrap`: quatro de 96px num contentor de 192).
    let quebra = !matches!(
        parent_css.white_space,
        Some(crate::style::WhiteSpace::Nowrap | crate::style::WhiteSpace::Pre)
    );
    for item in sizes {
        let w = item.1 + item.4 + item.5;
        if quebra && !cur.is_empty() && cur_w + w > content_w {
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
        // O default é `baseline` (CSS 2.1 §10.8.1) com a baseline PRÓPRIA de
        // cada item (`ascent_do_item`): o `Top` que aqui estava era o corte
        // que punha o caret `::after` do Bootstrap no topo da linha.
        let atomos: Vec<(f32, f32, VerticalAlign)> = items
            .iter()
            .map(|&(n, _, h, va, _, _)| (h, ascent_do_item(dom, n, h, content_w, ctx), va.unwrap_or(VerticalAlign::Baseline)))
            .collect();
        let lh = crate::inline_box::altura_da_linha(parent_css, font_size, ctx.measurer);
        let familia = parent_css.font_family.as_deref();
        let env = super::alinhamento_vertical::envelope_com_baseline(&atomos, font_size, lh, familia, ctx.measurer);
        for (&(child, w, h, va, gap, trailing), &(_, ascent, _)) in items.iter().zip(&atomos) {
            let valign = va.unwrap_or(VerticalAlign::Baseline);
            let base_y = super::alinhamento_vertical::topo_do_item_com_baseline(valign, h, ascent, cy, &env, font_size, familia, ctx.measurer);
            let line_has_textarea = items.iter().any(|(n, _, _, _, _, _)| matches!(&dom.node(*n).kind, NodeKind::Element { tag } if tag == "textarea"));
            let is_mark = matches!(dom.node(child).attr("type").map(|t| t.to_ascii_lowercase()).as_deref(), Some("checkbox" | "radio"));
            let form_text = matches!(&dom.node(child).kind, NodeKind::Element { tag } if matches!(tag.as_str(), "input" | "button" | "select")) && !is_mark;
            let item_y = if line_has_textarea && form_text { base_y + 2.5 } else { base_y };
            x += gap;
            layout_block(
                dom,
                child,
                x,
                item_y,
                content_w,
                avail_h,
                None,
                None,
                false,
                true,
                // Corrida de inline-blocks irmãos: mesma razão da linha, ver
                // `linha.rs`.
                &BlockFormattingContext::new(),
                ctx,
                list,
            );
            x += w + trailing;
        }
        cy += env.altura();
    }
    cy
}
