//! FLUXO INLINE: percorrer os filhos de um bloco inline-formatting-context,
//! montar as linhas e emitir a pintura de cada uma.
//!
//! Movido de `layout.rs` na modularização. O fragmento de cada dono e as
//! superfícies por linha vivem em `inline_fragmentos.rs` (teto de 500).

use super::*;

/// Um `<canvas>` na linha: a pergunta é da TAG e não do estilo, e aparece em
/// dois sítios desta função (quem pinta, e quem já gravou a caixa) — ter o
/// `match` escrito duas vezes era convidar as duas respostas a divergirem.
fn e_canvas(dom: &Dom, id: NodeIdx) -> bool {
    matches!(&dom.node(id).kind, crate::dom::NodeKind::Element { tag } if tag == "canvas")
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
    // Cada membro do grupo com a sua CAIXA — é por ela que `collect_runs`
    // desce, e para um FRAGMENTO de um inline partido (CSS 2.1 §9.2.1.1) é o
    // que o impede de descer no bloco que partiu o inline.
    group: &[(NodeIdx, crate::boxes::BoxId)],
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
    //
    // The BFC and not a copy of its exclusions: a float that appears in the
    // MIDDLE of this flow is placed here (`float_in_line.rs`) and has to reach
    // the siblings that come after, as a direct child's float does.
    bfc: &BlockFormattingContext,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) -> f32 {
    let _phase = crate::metrics::phases::scope("layout-inline");
    // coleta os RUNS (cada pedaço de texto com a SUA cor/bold herdada do span que
    // o contém) de TODOS os nós do grupo, em ordem de documento.
    let mut runs = Vec::new();
    let dono_inteiro = inline_fragmentos::group_is_whole_owner(dom, dono, group);
    let cor_base = cor_visivel(parent_css, parent_css.color.unwrap_or(0x000000FF));
    if dono_inteiro {
        runs.extend(pseudo_run(
            dom,
            dono,
            &[dono],
            crate::style::PseudoElement::Before,
            cor_base,
            parent_css.italic.unwrap_or(false),
            content_w,
            ctx,
        ));
    }
    // `Rc` clonado antes do laço: `list` é escrito ao longo da função inteira, e
    // é a mesma razão pela qual `layout_children_vertical` o clona.
    let arvore = std::rc::Rc::clone(&list.tree);
    for &(id, caixa) in group {
        runs.extend(collect_runs(dom, id, caixa, &arvore, parent_css, content_w, ctx));
    }
    if dono_inteiro {
        runs.extend(pseudo_run(
            dom,
            dono,
            &[dono],
            crate::style::PseudoElement::After,
            cor_base,
            parent_css.italic.unwrap_or(false),
            content_w,
            ctx,
        ));
    }
    let family = parent_css.font_family.as_deref();
    let mono = family.is_some_and(crate::style::is_mono_family);
    // A pergunta "e Ahem?" ja vivia aqui para `quebra::wrap_runs`; o item de
    // texto passa a carregar a MESMA resposta em vez de a fazer outra vez.
    let ahem = super::fonte_metricas::usa_ahem(family);
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
    let largura_da_linha = |exclusoes: &[Exclusao], i: usize| -> f32 {
        if nowrap {
            return f32::INFINITY;
        }
        if exclusoes.is_empty() {
            return content_w;
        }
        banda_livre(exclusoes, y + i as f32 * lh, lh, x, content_w).1
    };
    // The same band's LEFT edge, minus the content edge (`x`): how far line
    // `i` starts from the content edge because a float shortens it from the
    // left. A tab stop is measured from the content edge (CSS Text 3 §4.2),
    // not from where the line happens to start after a float — the defect
    // this offset exists to fix (`tab-stop-with-float`).
    // Unlike `largura_da_linha`, this ignores `nowrap`: a `white-space: pre`
    // line still STARTS past a float that crosses it (the float only stops
    // affecting the WIDTH question, which no longer applies once the line
    // cannot wrap) — the tab stop must see where the line actually begins.
    let offset_da_linha = |exclusoes: &[Exclusao], i: usize| -> f32 {
        if exclusoes.is_empty() {
            return 0.0;
        }
        banda_livre(exclusoes, y + i as f32 * lh, lh, x, content_w).0 - x
    };
    // quebra os runs em LINHAS, cada linha = sequência de pedaços coloridos (word).
    let fontes = super::fonte_do_trecho::Fontes::do_fluxo(dom, &runs, family, font_size, mono);
    let quebrar = |exclusoes: &[Exclusao]| {
        wrap_runs(
            &runs,
            &mut |i| largura_da_linha(exclusoes, i),
            &mut |i| offset_da_linha(exclusoes, i),
            font_size,
            mono,
            crate::inline_box::quebra_dentro(parent_css),
            super::preserved_spaces::Spaces::from_flow(dom, &runs, parent_css),
            parent_css.word_spacing.unwrap_or(0.0),
            parent_css.hyphens != Some(crate::style::vocab::Hyphens::None),
            &fontes, ctx.measurer,
        )
    };
    // Floats that appear in the MIDDLE of this flow are placed BEFORE the final
    // line breaking: each shortens the lines it crosses. See `float_in_line.rs`.
    super::float_in_line::place_anchored_floats(dom, &arvore, &runs, &quebrar, (x, y, content_w, lh), nowrap, parent_css, font_size, bfc, ctx, list);
    // Um MARKER (inline vazio) não cria linha — um `<span></span>` sozinho não muda a altura.
    if runs.iter().all(|r| r.text.trim().is_empty() && !r.atomic.is_some_and(|(_, _, k)| k.tem_corpo())) {
        // Continua sem linha; cada Marker ganha 0×0 (`inline_fragmentos`).
        inline_fragmentos::registar_markers_sem_linha(list, x, y, &runs, group);
        return y;
    }
    let exclusoes = bfc.snapshot();
    let lines = quebrar(&exclusoes);
    // `text-overflow: ellipsis` — depois da quebra e antes da colocação, porque
    // o que se corta é uma LINHA já formada. Ver [`aplicar_elipse`].
    let fonte_de = |owners: &[NodeIdx]| match super::fonte_do_trecho::do_segmento(dom, owners, family, font_size, ctx.measurer) {
        Some(f) => (f.fonte.size, f.fonte.mono, f.ahem),
        None => (font_size, mono, ahem),
    };
    let lines = match elipse_pedida(parent_css, nowrap) {
        true => aplicar_elipse(lines, content_w, &fonte_de, ctx.measurer),
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
            &fonte_de, ctx.measurer,
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
    // The last line's baseline, for an atom measuring its own (`linha_baseline.rs`).
    let mut ultima_baseline: Option<f32> = None;
    // A generated inline broken across lines carries its open surface over.
    let mut transporte = super::inline_fragmentos::Superficies::default();
    // CONSUMINDO as linhas: o texto de cada segmento vai direto para o
    // `DisplayItem`, em vez de ser clonado. Eram milhares de `String` alocadas
    // por passada de layout, uma por segmento, para copiar algo que ninguém mais
    // usaria depois.
    for line in lines {
        let line_id = super::LineScope::fresh(group); // one per line box: `box_fragments.rs`
        // A line holding nothing but ANCHORS is not a line box: a float or an
        // absolute box is out of flow and generates none (CSS 2.1 §9.5).
        // `a<br><float>` made a phantom second line — a full line of height, and
        // the LAST line box an enclosing inline-block then took its baseline from.
        if super::ancora_estatica::linha_so_de_ancoras(dom, &line, x, cy, list) {
            continue;
        }
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
            .filter(|s| s.atomic.is_some_and(|(_, _, k)| k.tem_corpo()))
            .map(|s| s.wh)
            .fold(lh, f32::max);
        // A CAIXA de cada inline desta linha: a content area da fonte, centrada na
        // linha pela meia-entrelinha. A linha continua a avançar `line_h` — quem
        // decide o espaçamento é o `line-height`, quem decide a caixa é a fonte.
        let conteudo = crate::inline_box::altura_do_conteudo(font_size, family, ctx.measurer);
        let meia = crate::inline_box::meia_entrelinha(line_h, conteudo);
        let tem_texto = line
            .iter()
            .any(|s| s.atomic.is_none() && !s.text.trim().is_empty());
        let imagem_alta_sem_texto = super::linha_baseline::imagem_alta_sem_texto(&line, line_h, lh, tem_texto);
        // As superfícies (fundo/borda) dos inlines por fragmentos desta
        // linha: acumulam-se ao longo dos segmentos e inserem-se ATRÁS deles.
        let at_linha = list.pieces.len();
        let mut superficies = std::mem::take(&mut transporte);
        // A line holding an inline-block is placed by the §10.8.1 envelope
        // (`linha_baseline.rs`): one baseline, each item's extent above and below
        // it. It replaced a special case that sat every inline-block on its bottom
        // edge. Lines of text and images alone keep the half-leading placement.
        let ascent = ctx.measurer.font_ascent_family(font_size, family);
        let envelope = super::linha_baseline::envelope_da_linha(dom, &line, font_size, lh, family, content_w, ctx);
        let na_baseline = envelope.is_some();
        let (text_top, text_owner_anchor, line_advance) = match &envelope {
            Some(env) => (cy + env.acima - ascent, cy + env.acima, env.altura()),
            None if imagem_alta_sem_texto => (cy + line_h - ascent, cy + meia, line_h),
            None => (cy + meia, cy + meia, line_h),
        };
        // A banda desta linha, no `cy` VERDADEIRO — é aqui que o texto passa a
        // correr ao lado do float em vez de por baixo dele.
        let (linha_x, linha_w) = if exclusoes.is_empty() {
            (x, content_w)
        } else {
            banda_livre(&exclusoes, cy, line_h, x, content_w)
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
            if let Some((a_idx, caixa, kind)) = seg.atomic {
                // Float and static-position anchors have nothing on the line (`ancora_estatica.rs`).
                if super::ancora_estatica::fora_da_linha(dom, (a_idx, caixa, kind), seg_x, x, cy, cy + line_advance, at_linha, list) {
                    continue;
                }
                let (desde, (rx, ry)) = (list.pieces.len(), super::relativo::offset_do_inline(dom, seg.owners.last().copied(), ctx));
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
                            layout_button(
                                dom,
                                a_idx,
                                caixa,
                                &wcss,
                                seg_x,
                                cy,
                                None,
                                ctx,
                                list,
                            );
                        } else {
                            // `None` de altura disponível: uma caixa atómica numa
                            // linha não tem containing block de altura definida, e
                            // é isso que faz `height:%` valer `auto` — como no
                            // browser.
                            layout_input(
                                dom, a_idx, caixa, &wcss, seg_x, cy, seg.ww, None, None, None, ctx, list,
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
                        if e_canvas(dom, a_idx) {
                            // O `<canvas>` pinta-se SEMPRE que está na linha, e
                            // não só quando tem desenho: a superfície pode
                            // chegar depois, e é `layout_canvas` que reserva a
                            // caixa entretanto — a mesma doutrina que o
                            // `<img>` segue no caminho de bloco.
                            let ccss = dom.computed_style_idx(a_idx).unwrap_or_default();
                            layout_canvas(dom, a_idx, caixa, &ccss, seg_x, topo, seg.ww.max(1.0), ctx, list);
                        } else if dom.image_dims(a_idx).is_some() {
                            let icss = dom.computed_style_idx(a_idx).unwrap_or_default();
                            layout_image(dom, a_idx, caixa, &icss, seg_x, topo, seg.ww.max(1.0), None, None, ctx, list);
                        }
                    }
                    AtomicKind::Block => {
                        // Um inline-block PINTA-SE como bloco (fundo, borda,
                        // padding) mas na posição que a linha lhe deu. É o mesmo
                        // `layout_block` da corrida de inline-blocks irmãos —
                        // não um segundo emissor — só que o x/y vem do fluxo.
                        // Inline-block VAZIO senta o fundo na baseline (§10.8.1; caret do Bootstrap a y=9).
                        let topo = match &envelope {
                            Some(env) => super::linha_baseline::topo_do_atomo(dom, &seg, cy, env, font_size, family, content_w, ctx),
                            None => cy,
                        };
                        layout_block(
                            dom,
                            a_idx,
                            caixa,
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
                    AtomicKind::Gerada(pe, ParteGerada::Atomo) => {
                        // A generated inline-block always makes `envelope` Some.
                        let topo = envelope
                            .as_ref()
                            .map_or(cy, |env| super::linha_baseline::topo_do_atomo(dom, &seg, cy, env, font_size, family, content_w, ctx));
                        super::pseudo_inline::pintar_atomo(dom, a_idx, pe, caixa, seg_x, topo, content_w, ctx, list);
                    }
                    AtomicKind::Marker
                    | AtomicKind::Break
                    | AtomicKind::ArestaInicio
                    | AtomicKind::ArestaFim
                    | AtomicKind::Gerada(..)
                    | AtomicKind::Float
                    | AtomicKind::Estatica => {}
                }
                super::relativo::desloca_desde(list, desde, kind.tem_corpo().then_some(caixa), rx, ry);
                superficies.ver(dom, &seg.owners, seg_x, seg_x + seg.ww);
                match kind {
                    AtomicKind::ArestaInicio => superficies.marca(a_idx, true),
                    AtomicKind::ArestaFim => superficies.marca(a_idx, false),
                    AtomicKind::Gerada(pe, ParteGerada::Inicio) => superficies.abre_gerada(dom, a_idx, pe, seg_x, seg.ww, content_w, ctx),
                    AtomicKind::Gerada(pe, ParteGerada::Fim) => superficies.fecha_gerada(dom, a_idx, pe, content_w, ctx),
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
                    AtomicKind::Widget | AtomicKind::Block | AtomicKind::ArestaInicio | AtomicKind::ArestaFim | AtomicKind::Gerada(..)
                ) || (kind == AtomicKind::Replaced
                    && (dom.image_dims(a_idx).is_some() || e_canvas(dom, a_idx)));
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
                    crate::inline_box::union_rect(list, a_idx, Rect::new(propria.x + rx, propria.y + ry, propria.w, propria.h), &line_id);
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
                            // The text's own anchor: `cy + meia` made the span 5px too tall on a line an atom grew.
                            text_owner_anchor,
                            seg.ww,
                            conteudo,
                            ctx,
                            na_baseline,
                        ), &line_id,
                    );
                }

                seg_x += seg.ww;
                continue;
            }
            let ls = parent_css.letter_spacing.unwrap_or(0.0);
            let w = seg.text_width + ls * seg.text.chars().count() as f32;
            superficies.ver(dom, &seg.owners, seg_x, seg_x + w);
            // Its OWN font on the shared baseline (`fonte_do_trecho.rs`), shifted with a relative inline.
            let propria = super::fonte_do_trecho::do_segmento(dom, &seg.owners, family, font_size, ctx.measurer);
            let (seg_y, seg_size, seg_mono, seg_ahem) = match &propria {
                Some(f) => (text_top + ascent - f.ascent, f.fonte.size, f.fonte.mono, f.ahem),
                None => (text_top, font_size, mono, ahem),
            };
            let (rx, ry) = super::relativo::offset_do_inline(dom, seg.owners.last().copied(), ctx);
            list.push_item(DisplayItem::Text {
                x: seg_x + rx,
                y: seg_y + ry,
                text: seg.text.into(),
                color: seg.color,
                size: seg_size,
                mono: seg_mono,
                is_ahem: seg_ahem,
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
                        na_baseline,
                    ), &line_id,
                );
            }
            seg_x += w;
        }
        transporte = superficies.pintar(
            dom, list, at_linha, line_id.id,
            text_owner_anchor,
            conteudo,
            na_baseline,
            ctx,
        );
        ultima_baseline = Some(text_top + ctx.measurer.font_ascent_family(font_size, family));
        cy += line_advance;
    }
    if let Some(b) = ultima_baseline {
        super::linha_baseline::regista_ultima_linha(dono, b);
    }
    cy
}
