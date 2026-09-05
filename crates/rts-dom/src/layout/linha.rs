//! FLUXO INLINE: percorrer os filhos de um bloco inline-formatting-context,
//! montar as linhas e emitir a pintura de cada uma.
//!
//! Movido de `layout.rs` na modularização. O fragmento de cada dono e as
//! superfícies por linha vivem em `inline_fragmentos.rs` (teto de 500).

use super::*;

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
    // `tab-size` — só sob `white-space: pre`/`pre-wrap`, onde o `\t` sobrevive
    // ao invés de colapsar como um espaço qualquer (`preserves_spaces`, hoje só
    // lido aqui). A coluna encadeia ENTRE runs do mesmo fluxo — ver o corte
    // declarado em `tabulacao::expandir_tabs`.
    if parent_css
        .white_space
        .map(|w| w.preserves_spaces())
        .unwrap_or(false)
    {
        let tab_size = parent_css.tab_size.unwrap_or(8.0).round().max(1.0) as usize;
        let mut coluna = 0usize;
        for run in runs.iter_mut() {
            if run.atomic.is_none() && !run.text.is_empty() {
                let (expandido, fim) =
                    crate::layout::tabulacao::expandir_tabs(&run.text, tab_size, coluna);
                run.text = expandido;
                coluna = fim;
            }
        }
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
    // Um MARKER (inline vazio) não cria linha — um `<span></span>` sozinho não muda a altura.
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
        // Continua sem linha; cada Marker ganha 0×0 (`inline_fragmentos`).
        inline_fragmentos::registar_markers_sem_linha(list, x, y, &runs);
        return y;
    }
    let family = parent_css.font_family.as_deref();
    let mono = family.is_some_and(crate::style::is_mono_family);
    let ahem = family.is_some_and(crate::style::is_ahem_family); // ver quebra::wrap_runs
    // line-height: do CSS (multiplicador ou px), senão o default do measurer —
    // #1749. O medidor é também quem responde por `line-height: normal`, porque
    // esse valor sai das MÉTRICAS DA FONTE e não de uma constante: sem isto, o
    // elemento sem declaração e o que declara `normal` — a spec diz que são o
    // mesmo valor — davam alturas diferentes.
    let lh = crate::inline_box::altura_da_linha(parent_css, font_size, ctx.measurer);
    // `text-wrap: nowrap` é um ALIAS do que `white-space: nowrap` já decide —
    // não uma segunda propriedade com regra própria (é o que o MDN documenta:
    // `text-wrap` só acrescenta `balance`/`pretty`, que caem no `wrap` normal
    // por não termos a segunda passada que pedem — ver `vocab::TextWrap`). Só
    // `Nowrap` muda este booleano; `Wrap`/`Balance`/`Pretty` são o mesmo `false`
    // que a ausência da propriedade já dava.
    let nowrap = matches!(
        parent_css.white_space,
        Some(crate::style::WhiteSpace::Nowrap | crate::style::WhiteSpace::Pre)
    ) || parent_css.text_wrap == Some(crate::style::vocab::TextWrap::Nowrap);
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
        parent_css
            .white_space
            .map(|w| w.preserves_newlines())
            .unwrap_or(false),
        parent_css.word_spacing.unwrap_or(0.0),
        parent_css.hyphens != Some(crate::style::vocab::Hyphens::None),
        ahem, ctx.measurer,
    );
    // `text-overflow: ellipsis` — depois da quebra e antes da colocação, porque
    // o que se corta é uma LINHA já formada. Ver [`aplicar_elipse`].
    let lines = match elipse_pedida(parent_css, nowrap) {
        true => aplicar_elipse(lines, content_w, font_size, mono, ahem, ctx.measurer),
        false => lines,
    };
    // `-webkit-line-clamp`/`line-clamp` — limita a N linhas, com "…" na
    // última. Ver `tabulacao::aplicar_line_clamp` para porque a altura da
    // caixa não precisa de um segundo cálculo.
    let lines = match parent_css.line_clamp {
        Some(n) if n > 0 => crate::layout::tabulacao::aplicar_line_clamp(
            lines,
            n as usize,
            content_w,
            font_size,
            mono, ahem, ctx.measurer,
        ),
        _ => lines,
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
        let conteudo = crate::inline_box::altura_do_conteudo(font_size, family, ctx.measurer);
        let meia = crate::inline_box::meia_entrelinha(line_h, conteudo);
        // O descent extra abaixo só vale com TEXTO de verdade a partilhar a
        // baseline com o inline-block — sem texto, `line_h` (a margin box do
        // átomo) já É o avanço certo. Sem este filtro, `claude-word-spacing.html`
        // (só `inline-block` + `<br>`, sem texto) ganhava `font_descent` a mais:
        // pitch 30 onde o Chrome dá 25 (a margin box sozinha).
        let tem_texto = line
            .iter()
            .any(|s| s.atomic.is_none() && !s.text.trim().is_empty());
        let tall_inline_block = line_h > lh + 0.001
            && tem_texto
            && line
                .iter()
                .any(|segment| matches!(segment.atomic, Some((_, AtomicKind::Block))));
        // Um inline-block vazio alinha pela baseline no seu fundo. Quando ele é
        // mais alto que o strut, o texto mantém o ascent da fonte acima dessa
        // baseline e o descent do strut fica abaixo dela. É o contrato Blink que
        // dá, na fixture display, texto em y=55 e o bloco seguinte em y=75.
        let text_top = if tall_inline_block {
            cy + line_h - ctx.measurer.font_ascent_family(font_size, family)
        } else {
            cy + meia
        };
        // As superfícies (fundo/borda) dos inlines por fragmentos desta
        // linha: acumulam-se ao longo dos segmentos e inserem-se ATRÁS deles.
        let at_linha = list.items.len();
        let filhos_antes_da_linha = list.children.len();
        let mut superficies = super::inline_fragmentos::Superficies::default();
        let text_owner_anchor = if tall_inline_block {
            cy + line_h
        } else {
            cy + meia
        };
        let line_advance = if tall_inline_block {
            line_h + ctx.measurer.font_descent_family(font_size, family)
        } else {
            line_h
        };
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
                                dom, a_idx, &wcss, seg_x, cy, seg.ww, None, None, None, ctx, list,
                            );
                        }
                    }
                    AtomicKind::Replaced => {
                        // REPLACED inline (um `<img>` no meio do texto): a caixa é o
                        // tamanho já medido. Só se pinta quando há pixels — e aí é
                        // `layout_image` que o faz, o mesmo caminho do fluxo de bloco,
                        // em vez de um segundo emissor de imagem só para o inline.
                        // Replaced inline senta na BASELINE (§10.8; `claude-img-ficheiro`: y=15).
                        let topo = text_top + ctx.measurer.font_ascent_family(font_size, family) - seg.wh;
                        if dom.image_dims(a_idx).is_some() {
                            let icss = dom.computed_style_idx(a_idx).unwrap_or_default();
                            layout_image(dom, a_idx, &icss, seg_x, topo, seg.ww.max(1.0), None, None, ctx, list);
                        }
                    }
                    AtomicKind::Block => {
                        // Um inline-block PINTA-SE como bloco (fundo, borda,
                        // padding) mas na posição que a linha lhe deu. É o mesmo
                        // `layout_block` da corrida de inline-blocks irmãos —
                        // não um segundo emissor — só que o x/y vem do fluxo.
                        // Inline-block VAZIO senta o fundo na baseline (§10.8.1; caret do Bootstrap a y=9).
                        let vazio = !super::caixa::tem_conteudo_para_fragmento(dom, a_idx);
                        let topo = if vazio && seg.wh < line_h { text_top + ctx.measurer.font_ascent_family(font_size, family) - seg.wh } else { cy };
                        layout_block(
                            dom,
                            a_idx,
                            seg_x,
                            topo,
                            seg.ww.max(1.0),
                            None,
                            None,
                            None,
                            false,
                            true,
                            // Inline-block atômico de uma linha: isolado, como
                            // qualquer inline-block (estabelece BFC próprio).
                            &BlockFormattingContext::new(),
                            ctx,
                            list,
                        );
                    }
                    AtomicKind::Marker
                    | AtomicKind::Break
                    | AtomicKind::ArestaInicio
                    | AtomicKind::ArestaFim => {}
                }
                superficies.ver(dom, &seg.owners, seg_x, seg_x + seg.ww);
                match kind {
                    AtomicKind::ArestaInicio => superficies.marca(a_idx, true),
                    AtomicKind::ArestaFim => superficies.marca(a_idx, false),
                    _ => {}
                }
                // A CAIXA DO PRÓPRIO: só regista aqui quem NADA mais registou.
                // `Widget`/`Block` chamam `layout_input`/`layout_button`/
                // `layout_block` INCONDICIONALMENTE (o `match` acima), e cada
                // um já grava a SUA — a border box (correta); unir aqui
                // `seg.ww`/`seg.wh` (a OUTER, com margem, que é o que a LINHA
                // reserva) inflava o rect do nó com a margem por cima da que
                // já tinha: um `inline-block` com `margin-bottom:5px`
                // respondia h=25 em vez de 20.
                //
                // `Replaced` é DIFERENTE: só grava quando há pixels (o mesmo
                // guard do `match` acima) — sem imagem decodificada,
                // `layout_image` nunca corre e É esta união que dá caixa ao
                // `<img>` enquanto não há pixels.
                // Uma aresta não é caixa própria: o dono (que está em `owners`)
                // recebe-a como fragmento no laço abaixo.
                let ja_registado = matches!(
                    kind,
                    AtomicKind::Widget | AtomicKind::Block | AtomicKind::ArestaInicio | AtomicKind::ArestaFim
                ) || (kind == AtomicKind::Replaced && dom.image_dims(a_idx).is_some());
                if !ja_registado {
                    let propria = match kind {
                        // `Marker`: inline SEM conteúdo (`<span></span>`) —
                        // no Blink dá 0×0, não a altura do strut. Um vazio
                        // com `content` gerado nunca chega aqui — `runs.rs`
                        // só emite `Marker` quando não gerou run nenhum.
                        AtomicKind::Marker => Rect::new(seg_x, text_top, 0.0, 0.0),
                        AtomicKind::Break => Rect::new(seg_x, text_top, 0.0, conteudo),
                        AtomicKind::Replaced =>
                            Rect::new(seg_x, text_top + ctx.measurer.font_ascent_family(font_size, family) - seg.wh, seg.ww, seg.wh),
                        _ => Rect::new(seg_x, cy, seg.ww, seg.wh),
                    };
                    crate::inline_box::union_rect(list, a_idx, propria);
                }
                // A CAIXA DOS ANCESTRAIS inline: a largura que esta caixa ocupa na
                // linha, com a altura da FONTE — um `<a>` à volta de uma imagem de
                // 528px de altura mede 17px no browser, não 528. É a mesma regra
                // que já vale para o texto, aplicada ao que não é texto.
                //
                // `Marker`: `a_idx` está no fim de `seg.owners` (todo
                // container inline entra na própria cadeia, para dar caixa a
                // um `::before`/`::after` gerado por ele) — mas um marker
                // GENUÍNO (sem gerado nenhum, único caso em que existe) não
                // tem fragmento a dar-se, e isto devolvia a altura que a
                // `própria` acima acabou de zerar. `owner != a_idx` evita-o.
                let sem_a_si = |o: &&NodeIdx| !(kind == AtomicKind::Marker && **o == a_idx);
                for &owner in seg.owners.iter().filter(sem_a_si) {
                    crate::inline_box::union_rect(
                        list,
                        owner,
                        super::inline_fragmentos::fragmento_do_dono(
                            dom,
                            owner,
                            seg_x,
                            cy + meia,
                            seg.ww,
                            conteudo,
                            ctx,
                            false,
                        ),
                    );
                }

                seg_x += seg.ww;
                continue;
            }
            let ls = parent_css.letter_spacing.unwrap_or(0.0);
            let w = seg.text_width + ls * seg.text.chars().count() as f32;
            superficies.ver(dom, &seg.owners, seg_x, seg_x + w);
            list.items.push(DisplayItem::Text {
                x: seg_x,
                y: text_top,
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
                    super::inline_fragmentos::fragmento_do_dono(
                        dom,
                        owner,
                        seg_x,
                        text_owner_anchor,
                        w.max(0.0),
                        conteudo,
                        ctx,
                        tall_inline_block,
                    ),
                );
            }
            seg_x += w;
        }
        superficies.pintar(
            dom,
            list,
            at_linha,
            filhos_antes_da_linha,
            text_owner_anchor,
            conteudo,
            tall_inline_block,
            ctx,
        );
        cy += line_advance;
    }
    cy
}
