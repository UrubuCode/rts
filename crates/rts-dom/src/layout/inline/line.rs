//! FLUXO INLINE: percorrer os filhos de um bloco inline-formatting-context,
//! montar as linhas e emitir a pintura de cada uma.
//!
//! Movido de `layout.rs` na modularização. O fragmento de cada dono e as
//! superfícies por linha vivem em `inline_fragments.rs` (teto de 500).

use super::*;

/// Um `<canvas>` na linha: a pergunta é da TAG e não do estilo, e aparece em
/// dois sítios desta função (quem pinta, e quem já gravou a caixa) — ter o
/// `match` escrito duas vezes era convidar as duas respostas a divergirem.
pub(in crate::layout) fn is_canvas(dom: &Dom, id: NodeIdx) -> bool {
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
    owner: NodeIdx,
    // Cada membro do grupo com a sua CAIXA — é por ela que `collect_runs`
    // desce, e para um FRAGMENTO de um inline partido (CSS 2.1 §9.2.1.1) é o
    // que o impede de descer no bloco que partiu o inline.
    group: &[(NodeIdx, crate::boxes::BoxId)],
    x: f32,
    y: f32,
    content_w: f32,
    // The owner's content height when DEFINITE - `layout_children_vertical`'s
    // `avail_h`, and the basis of a replaced atom's `height: %`. It reaches
    // both the measurement (`collect_runs`) and the painting (`emit_atom`) so
    // the line reserves the box that is then drawn.
    cb_h: Option<f32>,
    parent_css: &ComputedStyle,
    font_size: f32,
    // Os floats abertos que atravessam este fluxo. É a razão de a exclusão
    // atravessar DUAS camadas em vez de ficar no empilhamento de blocos: pelo
    // CSS a caixa de bloco ao lado de um float não desce nem encolhe — mantém a
    // largura e sobrepõe-se ao float —, e quem encolhe são as CAIXAS DE LINHA
    // lá dentro. Parar de empurrar o bloco sem encurtar as linhas trocava um
    // erro de posição por texto pintado por baixo da figura. Ver [`Exclusion`].
    //
    // The BFC and not a copy of its exclusions: a float that appears in the
    // MIDDLE of this flow is placed here (`in_line.rs`) and has to reach
    // the siblings that come after, as a direct child's float does.
    bfc: &BlockFormattingContext,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) -> f32 {
    let _phase = crate::metrics::phases::scope("layout-inline");
    // coleta os RUNS (cada pedaço de texto com a SUA cor/bold herdada do span que
    // o contém) de TODOS os nós do grupo, em ordem de documento.
    let mut runs = Vec::new();
    let whole_owner = inline_fragments::group_is_whole_owner(dom, owner, group);
    let base_color = cor_visivel(parent_css, parent_css.color.unwrap_or(0x000000FF));
    if whole_owner {
        runs.extend(pseudo_run(
            dom,
            owner,
            &[owner],
            crate::style::PseudoElement::Before,
            base_color,
            parent_css.italic.unwrap_or(false),
            content_w,
            ctx,
        ));
    }
    // `Rc` clonado antes do laço: `list` é escrito ao longo da função inteira, e
    // é a mesma razão pela qual `layout_children_vertical` o clona.
    let tree = std::rc::Rc::clone(&list.tree);
    for &(id, box_id) in group {
        runs.extend(collect_runs(dom, id, box_id, &tree, parent_css, content_w, cb_h, ctx));
    }
    if whole_owner {
        runs.extend(pseudo_run(
            dom,
            owner,
            &[owner],
            crate::style::PseudoElement::After,
            base_color,
            parent_css.italic.unwrap_or(false),
            content_w,
            ctx,
        ));
    }
    let family = parent_css.font_family.as_deref();
    let mono = family.is_some_and(crate::style::is_mono_family);
    // A pergunta "e Ahem?" ja vivia aqui para `line_break::wrap_runs`; o item de
    // texto passa a carregar a MESMA resposta em vez de a fazer outra vez.
    let ahem = crate::layout::measure::font_metrics::uses_ahem(family);
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
    // The removed `nowrap`/`pre` shortcut said a finite width was harmless
    // for an all-nowrap flow because `wrap_runs` never checks it to SPLIT a
    // run it will not break at anyway — missing `fechar_cluster!`'s OTHER
    // caller: an atomic marker (a float's anchor) closes the open cluster
    // UNCONDITIONALLY and that close still asks "does this fit?", not to
    // split but to decide whether the unbreakable chunk moves to a fresh
    // line — which an all-nowrap flow must never do (CSS 2.1 §9.5/§9.5.1: no
    // soft-wrap opportunity anywhere means ONE line, forced breaks aside).
    // `CSS2/floats/float-nowrap-7.html` (container AND span both `nowrap`)
    // pinned exactly this. Gated per-FLOW rather than per-container-property,
    // so a flow with one wrapping run (`a_normal_span_inside_pre_still_
    // wraps`) keeps the finite width that run's own clusters still check.
    // `line_start_offset` is unaffected either way — a nowrap line still
    // starts past a float crossing it; only the width question here changes.
    let flow_spaces = super::preserved_spaces::Spaces::from_flow(dom, &runs, parent_css);
    let never_wraps = (0..runs.len()).all(|i| !flow_spaces.of(i).wraps());
    let line_width = |exclusions: &[Exclusion], i: usize| -> f32 {
        if never_wraps {
            return f32::INFINITY;
        }
        if exclusions.is_empty() {
            return content_w;
        }
        free_band(exclusions, y + i as f32 * lh, lh, x, content_w).1
    };
    // The same band's LEFT edge, minus the content edge (`x`): how far line
    // `i` starts from the content edge because a float shortens it from the
    // left. A tab stop is measured from the content edge (CSS Text 3 §4.2),
    // not from where the line happens to start after a float — the defect
    // this offset exists to fix (`tab-stop-with-float`).
    // Unlike `line_width`, this ignores `nowrap`: a `white-space: pre`
    // line still STARTS past a float that crosses it (the float only stops
    // affecting the WIDTH question, which no longer applies once the line
    // cannot wrap) — the tab stop must see where the line actually begins.
    let line_start_offset = |exclusions: &[Exclusion], i: usize| -> f32 {
        if exclusions.is_empty() {
            return 0.0;
        }
        free_band(exclusions, y + i as f32 * lh, lh, x, content_w).0 - x
    };
    // quebra os runs em LINHAS, cada linha = sequência de pedaços coloridos (word).
    let fonts = super::run_font::Fonts::of_flow(dom, &runs, family, font_size, mono);
    let wrap = |exclusions: &[Exclusion]| {
        wrap_runs(
            &runs,
            &mut |i| line_width(exclusions, i),
            &mut |i| line_start_offset(exclusions, i),
            font_size,
            mono,
            crate::inline_box::quebra_dentro(parent_css),
            super::preserved_spaces::Spaces::from_flow(dom, &runs, parent_css),
            parent_css.word_spacing.unwrap_or(0.0),
            parent_css.hyphens != Some(crate::style::vocab::Hyphens::None),
            &fonts, ctx.measurer,
        )
    };
    // Floats that appear in the MIDDLE of this flow are placed BEFORE the final
    // line breaking: each shortens the lines it crosses. See `in_line.rs`.
    crate::layout::float::in_line::place_anchored_floats(dom, &tree, &runs, &wrap, (x, y, content_w, lh), nowrap, parent_css, font_size, bfc, ctx, list);
    // Um MARKER (inline vazio) não cria linha — um `<span></span>` sozinho não muda a altura.
    if runs.iter().all(|r| r.text.trim().is_empty() && !r.atomic.is_some_and(|(_, _, k)| k.tem_corpo())) {
        // Continua sem linha; cada Marker ganha 0×0 (`inline_fragmentos`).
        inline_fragments::register_markers_without_line(list, x, y, &runs, group);
        return y;
    }
    let exclusions = bfc.snapshot();
    let lines = wrap(&exclusions);
    // `text-overflow: ellipsis` — depois da quebra e antes da colocação, porque
    // o que se corta é uma LINHA já formada. Ver [`apply_ellipsis`].
    let font_of = |owners: &[NodeIdx]| match super::run_font::of_segment(dom, owners, family, font_size, ctx.measurer) {
        Some(f) => (f.font.size, f.font.mono, f.ahem),
        None => (font_size, mono, ahem),
    };
    let lines = match requested_ellipsis(parent_css, nowrap) {
        true => apply_ellipsis(lines, content_w, &font_of, ctx.measurer),
        false => lines,
    };
    // `-webkit-line-clamp`/`line-clamp` — limita a N linhas, com "…" na
    // última. Ver `tab_size::apply_line_clamp` para porque a altura da
    // caixa não precisa de um segundo cálculo.
    let lines = match parent_css.line_clamp {
        Some(n) if n > 0 => crate::layout::inline::tab_size::apply_line_clamp(
            lines,
            n as usize,
            content_w,
            &font_of, ctx.measurer,
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
    // The last line's baseline, for an atom measuring its own (`line_baseline.rs`).
    let mut last_baseline: Option<f32> = None;
    // A generated inline broken across lines carries its open surface over.
    let mut carried = super::inline_fragments::Surfaces::default();
    // CONSUMINDO as linhas: o texto de cada segmento vai direto para o
    // `DisplayItem`, em vez de ser clonado. Eram milhares de `String` alocadas
    // por passada de layout, uma por segmento, para copiar algo que ninguém mais
    // usaria depois.
    for line in lines {
        let line_id = super::LineScope::fresh(group); // one per line box: `box_rects.rs`
        // A line holding nothing but ANCHORS is not a line box: a float or an
        // absolute box is out of flow and generates none (CSS 2.1 §9.5).
        // `a<br><float>` made a phantom second line — a full line of height, and
        // the LAST line box an enclosing inline-block then took its baseline from.
        if super::static_anchor::anchors_only_line(dom, &line, x, cy, list) {
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
        let content = crate::inline_box::altura_do_conteudo(font_size, family, ctx.measurer);
        let half_leading = crate::inline_box::meia_entrelinha(line_h, content);
        let has_text = line
            .iter()
            .any(|s| s.atomic.is_none() && !s.text.trim().is_empty());
        let tall_image_without_text = super::line_baseline::tall_image_without_text(&line, line_h, lh, has_text);
        // As superfícies (fundo/borda) dos inlines por fragmentos desta
        // linha: acumulam-se ao longo dos segmentos e inserem-se ATRÁS deles.
        let line_at = list.pieces.len();
        let mut surfaces = std::mem::take(&mut carried);
        // A line holding an inline-block is placed by the §10.8.1 envelope
        // (`line_baseline.rs`): one baseline, each item's extent above and below
        // it. It replaced a special case that sat every inline-block on its bottom
        // edge. Lines of text and images alone keep the half-leading placement.
        let ascent = ctx.measurer.font_ascent_family(font_size, family);
        let envelope = super::line_baseline::line_envelope(dom, &line, font_size, lh, family, content_w, ctx);
        let on_baseline = envelope.is_some();
        let (text_top, text_owner_anchor, line_advance) = match &envelope {
            Some(env) => (cy + env.above - ascent, cy + env.above, env.height()),
            None if tall_image_without_text => (cy + line_h - ascent, cy + half_leading, line_h),
            None => (cy + half_leading, cy + half_leading, line_h),
        };
        // A banda desta linha, no `cy` VERDADEIRO — é aqui que o texto passa a
        // correr ao lado do float em vez de por baixo dele.
        let (band_x, band_w) = if exclusions.is_empty() {
            (x, content_w)
        } else {
            free_band(&exclusions, cy, line_h, x, content_w)
        };
        let free = (band_w - line_w).max(0.0);
        let mut seg_x = match parent_css.text_align {
            Some(crate::style::TextAlign::Right) => band_x + free,
            Some(crate::style::TextAlign::Center) => band_x + free / 2.0,
            _ => band_x, // left/justify
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
            if seg.atomic.is_some() {
                // The whole atom branch — widget, replaced, inline-block,
                // generated atom — moved to `line_atoms.rs` (teto de 500):
                // a pure move, see that file's header.
                super::line_atoms::emit_atom(
                    dom, ctx, list, &seg, &mut seg_x, x, cy, line_advance, line_at,
                    &mut surfaces, text_top, ascent, font_size, family,
                    &envelope, content_w, cb_h, on_baseline, text_owner_anchor, content,
                    &line_id,
                );
                continue;
            }
            let ls = parent_css.letter_spacing.unwrap_or(0.0);
            let w = seg.text_width + ls * seg.text.chars().count() as f32;
            surfaces.cover(dom, &seg.owners, seg_x, seg_x + w);
            // Its OWN font on the shared baseline (`run_font.rs`), shifted with a relative inline.
            let own_font = super::run_font::of_segment(dom, &seg.owners, family, font_size, ctx.measurer);
            let (seg_y, seg_size, seg_mono, seg_ahem) = match &own_font {
                Some(f) => (text_top + ascent - f.ascent, f.font.size, f.font.mono, f.ahem),
                None => (text_top, font_size, mono, ahem),
            };
            let (rx, ry) = crate::layout::positioned::relative::inline_offset(dom, seg.owners.last().copied(), ctx);
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
                    super::inline_fragments::owner_fragment(
                        dom,
                        owner,
                        seg_x,
                        text_owner_anchor,
                        w.max(0.0),
                        content,
                        ctx,
                        on_baseline,
                    ), &line_id,
                );
            }
            seg_x += w;
        }
        carried = surfaces.paint(
            dom, list, line_at, line_id.id,
            text_owner_anchor,
            content,
            on_baseline,
            ctx,
        );
        last_baseline = Some(text_top + ctx.measurer.font_ascent_family(font_size, family));
        cy += line_advance;
    }
    if let Some(b) = last_baseline {
        super::line_baseline::register_last_line(owner, b);
    }
    cy
}
