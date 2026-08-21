//! FLUXO INLINE: percorrer os filhos de um bloco inline-formatting-context,
//! montar as linhas e emitir a pintura de cada uma.
//!
//! Movido de `layout.rs` na modularização; nenhuma linha de lógica foi
//! alterada — a reconstrução deste ficheiro é byte a byte a do original.

use super::*;
/// Desenha um nó como linha(s) de texto (texto solto ou inline simples), herdando
/// cor/tamanho do bloco pai, e devolve o `y` abaixo. Caso de UM nó do fluxo
/// inline — o caminho geral (irmãos inline fluindo juntos) é
/// [`layout_inline_flow`].
fn layout_inline_line(
    dom: &Dom,
    id: NodeIdx,
    x: f32,
    y: f32,
    content_w: f32,
    parent_css: &ComputedStyle,
    font_size: f32,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) -> f32 {
    layout_inline_flow(
        dom,
        id,
        &[id],
        x,
        y,
        content_w,
        parent_css,
        font_size,
        &[],
        ctx,
        list,
    )
}

/// O FLUXO INLINE RICO (P4): um GRUPO de irmãos inline consecutivos (nós de texto
/// + elementos inline como `<a>`/`<b>`/`<span>`) flui como UM contexto — os runs
/// de todos concatenam, quebram por palavra na largura, e cada pedaço pinta com a
/// SUA cor/peso. É o que faz `<p>texto <a>link</a>, fim</p>` virar UMA linha
/// (antes cada filho virava uma linha própria — o footer do Bootstrap cover saía
/// em 5 linhas).
pub(in crate::layout) fn layout_inline_flow(
    dom: &Dom,
    // O elemento DONO deste fluxo — de quem são as caixas geradas
    // (`::before`/`::after`) que envolvem o grupo. Ver `pseudo_run`.
    dono: NodeIdx,
    group: &[NodeIdx],
    x: f32,
    y: f32,
    content_w: f32,
    parent_css: &ComputedStyle,
    font_size: f32,
    // Os floats abertos que atravessam este fluxo. É a razão de a exclusão
    // atravessar DUAS camadas em vez de ficar no empilhamento de blocos: pelo
    // CSS a caixa de bloco ao lado de um float não desce nem encolhe — mantém a
    // largura e sobrepõe-se ao float —, e quem encolhe são as CAIXAS DE LINHA
    // lá dentro. Parar de empurrar o bloco sem encurtar as linhas trocava um
    // erro de posição por texto pintado por baixo da figura. Ver [`Exclusao`].
    exclusoes: &[Exclusao],
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) -> f32 {
    let _phase = crate::metrics::phases::scope("layout-inline");
    // coleta os RUNS (cada pedaço de texto com a SUA cor/bold herdada do span que
    // o contém) de TODOS os nós do grupo, em ordem de documento.
    let mut runs = Vec::new();
    // A caixa gerada do DONO envolve todo o conteúdo dele — e só existe como run
    // aqui quando este grupo É todo o conteúdo. Com filhos de bloco pelo meio, o
    // conteúdo do dono parte-se em vários grupos e a caixa gerada teria de virar
    // um bloco anónimo, que é maquinaria de árvore de caixas que este layout não
    // tem; nesse caso não se gera nada, que é o estado anterior, em vez de a pôr
    // num pedaço arbitrário do conteúdo.
    // "este grupo é TODO o conteúdo do dono?" — contado sobre os filhos que
    // geram conteúdo. Os nós de texto só com espaços não contam: um HTML
    // indentado põe um antes e outro depois de cada elemento, e compará-los
    // fazia um `<div>` com o `<span>` numa linha indentada parecer conteúdo
    // partido, e perdia a caixa gerada em quase toda a página real.
    let filhos_com_conteudo = dom
        .node(dono)
        .children
        .iter()
        .filter(|&&c| !matches!(&dom.node(c).kind, NodeKind::Text(t) if t.trim().is_empty()))
        .count();
    let dono_inteiro = group
        .iter()
        .filter(|&&c| !matches!(&dom.node(c).kind, NodeKind::Text(t) if t.trim().is_empty()))
        .count()
        == filhos_com_conteudo;
    let cor_base = cor_visivel(parent_css, parent_css.color.unwrap_or(0x000000FF));
    if dono_inteiro {
        runs.extend(pseudo_run(
            dom,
            dono,
            &[dono],
            crate::style::PseudoElement::Before,
            cor_base,
            parent_css.italic.unwrap_or(false),
        ));
    }
    for &id in group {
        runs.extend(collect_runs(dom, id, parent_css, content_w, ctx));
    }
    if dono_inteiro {
        runs.extend(pseudo_run(
            dom,
            dono,
            &[dono],
            crate::style::PseudoElement::After,
            cor_base,
            parent_css.italic.unwrap_or(false),
        ));
    }
    // Um MARKER (elemento inline vazio) não conta como conteúdo: um `<span></span>`
    // sozinho num bloco não cria linha nenhuma no browser, e criá-la aqui mudaria
    // a altura do bloco — o oposto de "acrescenta geometria, não muda a pintura".
    if runs.iter().all(|r| {
        r.text.trim().is_empty()
            && !matches!(
                r.atomic,
                Some((
                    _,
                    AtomicKind::Widget
                        | AtomicKind::Replaced
                        | AtomicKind::Block
                        | AtomicKind::Break
                ))
            )
    }) {
        return y;
    }
    let mono = parent_css
        .font_family
        .as_deref()
        .map(crate::style::is_mono_family)
        .unwrap_or(false);
    // line-height: do CSS (multiplicador ou px), senão o default do measurer —
    // #1749. O medidor é também quem responde por `line-height: normal`, porque
    // esse valor sai das MÉTRICAS DA FONTE e não de uma constante: sem isto, o
    // elemento sem declaração e o que declara `normal` — a spec diz que são o
    // mesmo valor — davam alturas diferentes.
    let lh = crate::inline_box::altura_da_linha(parent_css, font_size, ctx.measurer);
    let nowrap = matches!(
        parent_css.white_space,
        Some(crate::style::WhiteSpace::Nowrap | crate::style::WhiteSpace::Pre)
    );
    // A LARGURA DE QUEBRA, linha a linha: onde um float estorva, a linha é
    // curta; onde ele acaba, volta a ser a do content.
    //
    // ⚠️ APROXIMAÇÃO DECLARADA: a banda de cada linha é prevista pelo ÍNDICE
    // dela, assumindo que todas medem `lh`. Uma linha com um widget mais alto
    // desloca as seguintes e a previsão fica uma fração de linha acima do
    // sítio real. É uma decisão, não um esquecimento: a alternativa é quebrar e
    // posicionar na mesma passagem, o que obriga a intercalar `wrap_runs` com o
    // avanço do cursor. A PINTURA não usa esta previsão — usa o `cy` verdadeiro
    // (ver a banda recalculada no laço), portanto o erro fica no ponto de
    // quebra e nunca em texto pintado por cima de um float.
    let mut largura_da_linha = |i: usize| -> f32 {
        if nowrap {
            return f32::INFINITY;
        }
        if exclusoes.is_empty() {
            return content_w;
        }
        banda_livre(exclusoes, y + i as f32 * lh, lh, x, content_w).1
    };
    // quebra os runs em LINHAS, cada linha = sequência de pedaços coloridos (word).
    let lines = wrap_runs(
        &runs,
        &mut largura_da_linha,
        font_size,
        mono,
        crate::inline_box::quebra_dentro(parent_css),
        ctx.measurer,
    );
    // `text-overflow: ellipsis` — depois da quebra e antes da colocação, porque
    // o que se corta é uma LINHA já formada. Ver [`aplicar_elipse`].
    let lines = match elipse_pedida(parent_css, nowrap) {
        true => aplicar_elipse(lines, content_w, font_size, mono, ctx.measurer),
        false => lines,
    };
    // `text-indent`: recuo da PRIMEIRA linha (MDN). ⚠️ CORTE: recua o início da
    // linha mas NÃO encurta a largura de quebra dela — a quebra já foi calculada
    // acima, e refazê-la só para a primeira linha exigia partir o `wrap_runs` em
    // duas passadas. O erro fica no ponto de quebra da 1ª linha; o recuo, que é o
    // efeito que a página pede, está certo. Negativo é aceite (o truque de
    // esconder texto atrás da margem).
    let indent = parent_css
        .text_indent
        .and_then(|d| {
            d.resolve_signed(&ResolveCtx {
                parent_content_w: content_w,
                node_font_size: font_size,
                root_font_size: crate::style::root_font_size(),
                viewport_w: ctx.viewport_w,
                viewport_h: ctx.viewport_h,
            })
        })
        .unwrap_or(0.0);
    let mut first_line = true;
    let mut cy = y;
    // CONSUMINDO as linhas: o texto de cada segmento vai direto para o
    // `DisplayItem`, em vez de ser clonado. Eram milhares de `String` alocadas
    // por passada de layout, uma por segmento, para copiar algo que ninguém mais
    // usaria depois.
    for line in lines {
        // largura total da linha (texto no SEU peso + widgets) p/ text-align.
        let line_w: f32 = line
            .iter()
            .map(|seg| {
                seg.lead_w
                    + match seg.atomic {
                        Some(_) => seg.ww,
                        None => seg.text_width,
                    }
            })
            .sum();
        // altura da linha: o texto (lh) ou o widget mais alto nela.
        let line_h = line
            .iter()
            .filter(|s| {
                matches!(
                    s.atomic,
                    Some((
                        _,
                        AtomicKind::Widget
                            | AtomicKind::Replaced
                            | AtomicKind::Block
                            | AtomicKind::Break
                    ))
                )
            })
            .map(|s| s.wh)
            .fold(lh, f32::max);
        // A CAIXA de cada inline desta linha: a content area da fonte, centrada na
        // linha pela meia-entrelinha. A linha continua a avançar `line_h` — quem
        // decide o espaçamento é o `line-height`, quem decide a caixa é a fonte.
        let conteudo = crate::inline_box::altura_do_conteudo(font_size, ctx.measurer);
        let meia = crate::inline_box::meia_entrelinha(line_h, conteudo);
        // A banda desta linha, no `cy` VERDADEIRO — é aqui que o texto passa a
        // correr ao lado do float em vez de por baixo dele.
        let (linha_x, linha_w) = if exclusoes.is_empty() {
            (x, content_w)
        } else {
            banda_livre(exclusoes, cy, line_h, x, content_w)
        };
        let free = (linha_w - line_w).max(0.0);
        let mut seg_x = match parent_css.text_align {
            Some(crate::style::TextAlign::Right) => linha_x + free,
            Some(crate::style::TextAlign::Center) => linha_x + free / 2.0,
            _ => linha_x, // left/justify
        };
        if first_line {
            seg_x += indent;
            first_line = false;
        }
        // pinta cada pedaço NA SUA COR e PESO, avançando o x.
        for seg in line {
            let seg: Segment = seg;
            // O vão que precede o segmento ocupa lugar na linha mas não pertence
            // a nada: avança o cursor antes de qualquer caixa ser calculada.
            seg_x += seg.lead_w;
            if let Some((a_idx, kind)) = seg.atomic {
                match kind {
                    AtomicKind::Widget => {
                        // WIDGET inline: pinta a caixa no lugar (botão via layout_button;
                        // campo de texto via layout_input com o avail da linha).
                        let wcss = dom.computed_style_idx(a_idx).unwrap_or_default();
                        let itype = dom
                            .node(a_idx)
                            .attr("type")
                            .map(|t| t.to_ascii_lowercase())
                            .unwrap_or_default();
                        if matches!(itype.as_str(), "submit" | "button" | "reset") {
                            layout_button(dom, a_idx, &wcss, seg_x, cy, ctx, list);
                        } else {
                            // `None` de altura disponível: uma caixa atómica numa
                            // linha não tem containing block de altura definida, e
                            // é isso que faz `height:%` valer `auto` — como no
                            // browser.
                            layout_input(
                                dom, a_idx, &wcss, seg_x, cy, seg.ww, None, None, ctx, list,
                            );
                        }
                    }
                    AtomicKind::Replaced => {
                        // REPLACED inline (um `<img>` no meio do texto): a caixa é o
                        // tamanho já medido. Só se pinta quando há pixels — e aí é
                        // `layout_image` que o faz, o mesmo caminho do fluxo de bloco,
                        // em vez de um segundo emissor de imagem só para o inline.
                        if dom.image_of(a_idx).is_some() {
                            let icss = dom.computed_style_idx(a_idx).unwrap_or_default();
                            layout_image(dom, a_idx, &icss, seg_x, cy, seg.ww.max(1.0), ctx, list);
                        }
                    }
                    AtomicKind::Block => {
                        // Um inline-block PINTA-SE como bloco (fundo, borda,
                        // padding) mas na posição que a linha lhe deu. É o mesmo
                        // `layout_block` da corrida de inline-blocks irmãos —
                        // não um segundo emissor — só que o x/y vem do fluxo.
                        layout_block(
                            dom,
                            a_idx,
                            seg_x,
                            cy,
                            seg.ww.max(1.0),
                            None,
                            None,
                            None,
                            true,
                            &[],
                            ctx,
                            list,
                        );
                    }
                    AtomicKind::Marker | AtomicKind::Break => {}
                }
                // A CAIXA DO PRÓPRIO: a de uma caixa atómica é o seu tamanho; a
                // de um vazio/quebra é a fatia de linha que ele ocupa.
                let propria = match kind {
                    AtomicKind::Marker | AtomicKind::Break => {
                        Rect::new(seg_x, cy + meia, 0.0, conteudo)
                    }
                    _ => Rect::new(seg_x, cy, seg.ww, seg.wh),
                };
                crate::inline_box::union_rect(list, a_idx, propria);
                // A CAIXA DOS ANCESTRAIS inline: a largura que esta caixa ocupa na
                // linha, com a altura da FONTE — um `<a>` à volta de uma imagem de
                // 528px de altura mede 17px no browser, não 528. É a mesma regra
                // que já vale para o texto, aplicada ao que não é texto.
                for &owner in &seg.owners {
                    crate::inline_box::union_rect(
                        list,
                        owner,
                        fragmento_do_dono(dom, owner, seg_x, cy + meia, seg.ww, conteudo, ctx),
                    );
                }
                // A CAIXA DOS ANCESTRAIS inline: a largura que esta caixa ocupa na
                // linha, com a altura da FONTE — um `<a>` à volta de uma imagem de
                // 528px de altura mede 17px no browser, não 528. É a mesma regra
                // que já vale para o texto, aplicada ao que não é texto.

                seg_x += seg.ww;
                continue;
            }
            let ls = parent_css.letter_spacing.unwrap_or(0.0);
            let w = seg.text_width + ls * seg.text.chars().count() as f32;
            list.items.push(DisplayItem::Text {
                x: seg_x,
                y: cy + meia,
                text: seg.text.into(),
                color: seg.color,
                size: font_size,
                mono,
                bold: seg.bold,
                italic: seg.italic,
                letter_spacing: ls,
                decoration: seg.deco,
            });
            for &owner in &seg.owners {
                crate::inline_box::union_rect(
                    list,
                    owner,
                    fragmento_do_dono(dom, owner, seg_x, cy + meia, w.max(0.0), conteudo, ctx),
                );
            }
            seg_x += w;
        }
        cy += line_h;
    }
    cy
}

/// O fragmento que ESTE dono recebe desta fatia de linha.
///
/// A altura é a content area da fonte DELE, não a do bloco que conduz o fluxo:
/// um `<span>` de 14px dentro de um título de 17,5px mede 15,75 e não 19,7. Sem
/// isto, 1 172 dos 1 257 `<span>` da Wikipédia com altura errada tinham
/// exatamente `1.125 x a fonte de um ANCESTRAL` — quase sempre o bloco quatro
/// níveis acima.
///
/// Fica CENTRADO na content area da linha, que é a mesma aproximação da
/// meia-entrelinha (o browser alinha pela linha de base; centrar acertou dentro
/// de 1px no caso medido do `<a>` à volta de uma imagem).
#[allow(clippy::too_many_arguments)]
fn fragmento_do_dono(
    dom: &Dom,
    dono: NodeIdx,
    x: f32,
    y: f32,
    w: f32,
    conteudo_da_linha: f32,
    ctx: &LayoutCtx,
) -> Rect {
    let Some(css) = dom.computed_style_idx(dono) else {
        return Rect::new(x, y, w, conteudo_da_linha);
    };
    let Some(crate::style::Dimension::Px(fonte)) = css.font_size else {
        return Rect::new(x, y, w, conteudo_da_linha);
    };
    let conteudo = crate::inline_box::altura_do_conteudo(fonte, ctx.measurer);
    Rect::new(x, y + (conteudo_da_linha - conteudo) / 2.0, w, conteudo)
}
