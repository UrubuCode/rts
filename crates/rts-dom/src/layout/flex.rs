//! FLEX no eixo horizontal: a base de cada item, `grow`/`shrink`, e a
//! distribuição do espaço livre numa linha.
//!
//! Movido de `layout.rs` na modularização; nenhuma linha de lógica foi
//! alterada — a reconstrução deste ficheiro é byte a byte a do original.

use super::*;
/// Um item do flex (pré-pass), com a BASE no eixo principal (flex-basis/width/
/// conteúdo, outer com margem), o MAIN size final (após grow/shrink) e os
/// fatores de flexibilidade lidos do estilo.
struct FlexItem {
    node: NodeIdx,
    /// tamanho BASE outer no eixo principal (antes de grow/shrink).
    base: f32,
    /// main size FINAL outer (após grow/shrink) — começa igual à base.
    main: f32,
    /// altura outer (cross) — re-medida com o main final quando ele muda.
    h: f32,
    /// `true` se é um nó de texto solto (pintado direto, não via layout_block).
    is_text: bool,
    /// `flex-grow` (0 = não cresce).
    grow: f32,
    /// `flex-shrink` (1 = default do CSS; texto solto não encolhe).
    shrink: f32,
    /// `align-self` do item (None = usa o align-items do container).
    align_self: Option<crate::style::AlignItems>,
    /// `order` (menor primeiro; empate = ordem do documento — sort estável).
    order: i32,
    /// o item PODE ser esticado pelo stretch (sem `height` explícito).
    can_stretch: bool,
    /// piso de `min-content` no eixo principal (spec flexbox §9.7) — o
    /// `flex-shrink` nunca encolhe o item abaixo disto. Texto solto e itens de
    /// `grid_cols` não têm piso próprio (a coluna de grid não é um item de
    /// conteúdo variável no mesmo sentido).
    min_main: f32,
}

/// A BASE outer de um item flex no eixo principal: `flex-basis` explícita
/// (resolvida como o width — respeita box-sizing) + margens; `auto`/ausente →
/// width/conteúdo ([`child_outer_width`]). O `.col` do Bootstrap tem basis `0%`
/// → a base é só o frame (e o grow distribui o espaço).
fn flex_base_outer(
    dom: &Dom,
    id: NodeIdx,
    container_w: f32,
    parent_font: f32,
    ctx: &LayoutCtx,
) -> f32 {
    let css = dom.computed_style_idx(id).unwrap_or_default();
    let font = font_px(&css, parent_font);
    let resolve = ResolveCtx {
        parent_content_w: container_w,
        node_font_size: font,
        root_font_size: crate::style::root_font_size(),
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    let basis = css.flex_basis.and_then(|d| match d {
        crate::style::Dimension::Auto => None,
        other => other.resolve(&resolve),
    });
    let Some(basis) = basis else {
        return child_outer_width(dom, id, container_w, parent_font, ctx);
    };
    let margin_h = css.margin.resolve_h(&resolve);
    if css.border_box.unwrap_or(false) {
        basis + margin_h // border-box: a basis JÁ é a caixa (pad+borda inclusos)
    } else {
        basis + margin_h + 2.0 * css.border_width.unwrap_or(0.0) + css.padding.resolve_h(&resolve)
    }
}

/// Dispõe os filhos HORIZONTAL (flex-row). Implementa gap, justify-content (eixo
/// principal) e align-items (eixo cruzado). Devolve a altura total do content.
///
/// - `wrap = false` (flex sem wrap): tudo numa linha; justify distribui o espaço
///   livre; em overflow, cai para flex-start (transborda no fim).
/// - `wrap = true` (inline-block/flex-wrap): quebra para a próxima linha quando não
///   cabe; justify/align aplicam POR LINHA.
pub(in crate::layout) fn layout_children_horizontal(
    dom: &Dom,
    id: NodeIdx,
    content_x: f32,
    content_y: f32,
    content_w: f32,
    // altura do CONTENT do container quando explícita (já resolvida pelo caller,
    // no eixo certo) — referência do cross-axis p/ align-items e containing block
    // dos filhos.
    container_content_h: Option<f32>,
    css: &ComputedStyle,
    font_size: f32,
    wrap: bool,
    // `Some(N)` quando `display:grid`: cada item vira uma coluna de largura fixa
    // `(content_w - (N-1)*gap)/N` e a linha quebra a cada N. `None` = flex/wrap normal.
    grid_cols: Option<i32>,
    // `flex-direction: row-reverse` — a ordem VISUAL principal inverte (spec
    // §5.1); aplicado depois do `order` (`items.sort_by_key` abaixo), que é o
    // mesmo efeito de inverter a atribuição de posições no eixo principal.
    reverse: bool,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) -> f32 {
    // gap/row-gap resolvidos do CSS (px/%/… contra o content do container).
    let resolve = ResolveCtx {
        parent_content_w: content_w,
        node_font_size: font_size,
        root_font_size: crate::style::root_font_size(),
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    let gap = css
        .gap
        .and_then(|d| d.resolve(&resolve))
        .unwrap_or(0.0)
        .max(0.0);
    let row_gap = css
        .row_gap
        .and_then(|d| d.resolve(&resolve))
        .unwrap_or(0.0)
        .max(0.0);
    let justify = css
        .justify
        .unwrap_or(crate::style::JustifyContent::FlexStart);
    let align = css.align_items.unwrap_or(crate::style::AlignItems::Stretch);
    // `0` = sem height explícito (o cross-size da linha usa o max dos itens).
    let container_cross_h = container_content_h.unwrap_or(0.0);

    // ── PRÉ-PASS: coleta cada filho renderável com a BASE flex + fatores ─────────
    let mut items: Vec<FlexItem> = Vec::new();
    for &child in &dom.node(id).children {
        if let NodeKind::Element { tag } = &dom.node(child).kind {
            if is_non_rendered_tag(tag) {
                continue;
            }
        }
        // fora do fluxo: não é item flex (pintado na passada out-of-flow).
        if is_out_of_flow(dom, child) {
            continue;
        }
        // `display:none` não é item de flex: não conta para o wrap, não come um
        // `gap` e não recebe main size. `layout_block` já lhe dava caixa zero, o
        // que escondia o defeito — a caixa era invisível mas o LUGAR dela não.
        if e_display_none(dom, child) {
            continue;
        }
        // BLOCKIFICAÇÃO: um filho de flex é um item de nível BLOCO, mesmo sendo
        // um `<span>` (a spec blockifica os itens de flex; o Chrome reporta
        // `display:block` neles). Só um NÓ DE TEXTO é item anónimo.
        //
        // A condição era `!is_block_level`, e por isso um `<span>` filho de flex
        // caía no ramo de texto: era achatado para uma string, pintado com o
        // estilo do CONTAINER, e não registava caixa nenhuma — 345 dos 351
        // elementos `display:block` sem caixa da Wikipédia eram exatamente isto.
        if matches!(dom.node(child).kind, NodeKind::Text(_)) {
            // texto solto: largura medida; vazio é ignorado. Não cresce nem encolhe.
            let text = collect_text(dom, child);
            if text.trim().is_empty() {
                continue;
            }
            let w = ctx
                .measurer
                .text_width(&text, font_size, false, false, false);
            let h = crate::inline_box::altura_da_linha(css, font_size, ctx.measurer);
            items.push(FlexItem {
                node: child,
                base: w,
                main: w,
                h,
                is_text: true,
                grow: 0.0,
                shrink: 0.0,
                align_self: None,
                order: 0,
                can_stretch: false,
                min_main: w, // texto solto: o piso é ele mesmo (não quebra aqui).
            });
            continue;
        }
        let ccss = dom.computed_style_idx(child).unwrap_or_default();
        let base = flex_base_outer(dom, child, content_w, font_size, ctx);
        let h = child_outer_height(
            dom,
            child,
            content_w,
            container_content_h,
            css,
            font_size,
            ctx,
        );
        // Piso de `min-content` (spec §9.7): reusa `cell_min_max` do algoritmo
        // de largura de tabela — a mesma pergunta ("a palavra mais larga, com o
        // frame do elemento"), sem duplicar a travessia. `grid_cols` zera o
        // resultado a seguir (a coluna de grid não encolhe por conteúdo), então
        // medir aqui sempre é mais simples que condicionar a medição.
        let min_main = crate::table::min_content(dom, child, font_size, ctx);
        items.push(FlexItem {
            node: child,
            base,
            main: base,
            h,
            is_text: false,
            grow: ccss.flex_grow.unwrap_or(0.0),
            shrink: ccss.flex_shrink.unwrap_or(1.0), // 1 é o default do CSS
            align_self: ccss.align_self,
            order: ccss.order.unwrap_or(0),
            can_stretch: ccss.height.is_none(),
            min_main,
        });
    }
    // `order` reordena ANTES do wrap (sort estável: empate = ordem do documento).
    items.sort_by_key(|it| it.order);
    // `row-reverse`: a ordem VISUAL principal inverte DEPOIS do `order` (spec
    // §5.1) — reverter a lista já ordenada tem o mesmo efeito de inverter a
    // atribuição de posições no eixo principal, sem duplicar o algoritmo de
    // posicionamento abaixo.
    if reverse {
        items.reverse();
    }

    // GRID: cada item (não-texto) vira uma coluna de largura fixa. Fixa base=main=col_w
    // e zera grow/shrink (a coluna não flui) → o wrap abaixo quebra a cada N colunas.
    if let Some(n) = grid_cols {
        let n = n.max(1) as f32;
        let col_w = ((content_w - (n - 1.0) * gap) / n).max(0.0);
        for it in items.iter_mut() {
            if it.is_text {
                continue;
            }
            it.base = col_w;
            it.main = col_w;
            it.grow = 0.0;
            it.shrink = 0.0;
        }
    }

    // agrupa em LINHAS pela BASE (o wrap decide pelas bases; grow/shrink POR linha).
    let mut lines: Vec<Vec<FlexItem>> = vec![Vec::new()];
    let mut line_w = 0.0f32;
    for it in items {
        let cur = lines.last_mut().unwrap();
        let with_gap = if cur.is_empty() { 0.0 } else { gap };
        if wrap && !cur.is_empty() && line_w + with_gap + it.base > content_w {
            lines.push(Vec::new());
            line_w = it.base;
        } else {
            line_w += with_gap + it.base;
        }
        lines.last_mut().unwrap().push(it);
    }

    // `align-content` em MULTI-LINHA (spec §8.4/flexbox §8): distribui o
    // espaço cruzado sobrante entre as linhas de wrap, com o mesmo
    // `justify_offsets` do flex/grid — a estimativa de altura de cada linha
    // usa a medição do PRÉ-PASS (antes do grow/shrink do eixo principal, que
    // é o que a resolução por linha abaixo ainda vai fazer): a altura de uma
    // linha muda por causa da largura só quando o encolhimento força quebra de
    // texto, um efeito de segunda ordem que esta aproximação aceita — refinar
    // exigiria resolver todas as linhas duas vezes.
    let mut line_align_leading = 0.0f32;
    let mut line_align_between = 0.0f32;
    if wrap && lines.len() > 1 {
        if let Some(v) = css.align_content {
            if container_cross_h > 0.0 {
                let estimativa: f32 = lines
                    .iter()
                    .map(|l| l.iter().fold(0.0f32, |a, it| a.max(it.h)))
                    .sum::<f32>()
                    + (lines.len().saturating_sub(1)) as f32 * row_gap;
                let free = (container_cross_h - estimativa).max(0.0);
                let (leading, between) =
                    crate::layout::coluna::justify_offsets(v, free, lines.len());
                line_align_leading = leading;
                line_align_between = between;
            }
        }
    }

    // ── RESOLVE + POSICIONA por linha: grow/shrink (main), justify, align ────────
    let mut line_y = content_y + line_align_leading;
    for line in &mut lines {
        if line.is_empty() {
            continue;
        }
        let n = line.len();
        let total_gap = (n.saturating_sub(1)) as f32 * gap;

        // GROW/SHRINK (spec flexbox §9.7 simplificada): espaço livre positivo
        // distribui ∝ flex-grow (o `.col { flex:1 0 0% }` divide igual); negativo
        // encolhe ∝ shrink × base (itens maiores cedem mais), clamp ≥ 0.
        let sum_base: f32 = line.iter().map(|it| it.base).sum();
        let free_pre = content_w - sum_base - total_gap;
        let sum_grow: f32 = line.iter().map(|it| it.grow).sum();
        if free_pre > 0.0 && sum_grow > 0.0 {
            for it in line.iter_mut() {
                it.main = it.base + free_pre * it.grow / sum_grow;
            }
        } else if free_pre < 0.0 {
            // ENCOLHIMENTO com PISO de `min-content` (spec §9.7): a cada
            // iteração repartimos o défice pelos itens ainda LIVRES
            // (`shrink*base` ponderado); um item que bateria abaixo do seu
            // `min_main` congela nele e sai da repartição — o défice que ele
            // não absorveu volta para os itens que sobraram, na iteração
            // seguinte. Sem isto um item de texto longo encolhia até
            // sobrepor-se ao próprio conteúdo (achado da auditoria de
            // 2026-09-04, `04-layout.md` finding 6).
            let n = line.len();
            let mut frozen = vec![false; n];
            let mut deficit = free_pre; // negativo
            loop {
                let weighted: f32 = line
                    .iter()
                    .zip(&frozen)
                    .filter(|&(_, f)| !f)
                    .map(|(it, _)| it.shrink * it.base)
                    .sum();
                if weighted <= 0.0 || deficit >= -0.01 {
                    break;
                }
                let mut novo_congelado = false;
                for (it, f) in line.iter_mut().zip(frozen.iter_mut()) {
                    if *f {
                        continue;
                    }
                    let proposto = it.base + deficit * (it.shrink * it.base) / weighted;
                    if proposto <= it.min_main {
                        it.main = it.min_main;
                        *f = true;
                        novo_congelado = true;
                    } else {
                        it.main = proposto;
                    }
                }
                if !novo_congelado {
                    break; // convergiu sem ninguém bater no piso: acabou.
                }
                // défice restante = o que os itens NÃO congelados ainda devem
                // absorver — a soma dos `main` correntes contra as bases.
                deficit = line
                    .iter()
                    .zip(&frozen)
                    .filter(|&(_, f)| !f)
                    .map(|(it, _)| it.main - it.base)
                    .sum::<f32>()
                    .min(0.0);
                if deficit >= -0.01 {
                    break;
                }
            }
        }
        // re-mede a ALTURA com o main final (mais largura → menos linhas de texto);
        // só quando o main mudou (senão a medição do pré-pass vale).
        for it in line.iter_mut() {
            if !it.is_text && (it.main - it.base).abs() > 0.5 {
                let (_, h) = measure_block(
                    dom,
                    it.node,
                    content_w,
                    container_content_h,
                    Some(it.main),
                    None,
                    true,
                    ctx,
                );
                it.h = h;
            }
        }

        // Cross-size de referência da linha = max das alturas dos itens, MAS se o
        // container tem `height` explícito e a linha é única (no-wrap), o cross-size
        // é a ALTURA DO CONTENT do container (fiel ao Chrome). Em wrap, cada linha
        // usa seu próprio max (repartir o height entre linhas — corte documentado).
        let items_h = line.iter().fold(0.0f32, |a, it| a.max(it.h));
        let line_h = if !wrap && container_cross_h > items_h {
            container_cross_h
        } else {
            items_h
        };

        // justify-content sobre o espaço restante PÓS-grow (com grow>0 o free é 0
        // e o justify é neutro — correto). Em overflow, ver justify_offsets.
        let sum_main: f32 = line.iter().map(|it| it.main).sum();
        let free = content_w - sum_main - total_gap;
        let (leading, between) = justify_offsets(justify, free, n);

        let mut x = content_x + leading;
        for (j, it) in line.iter().enumerate() {
            if j > 0 {
                x += gap + between;
            }
            // align por item: `align-self` vence o `align-items` do container;
            // STRETCH real: item sem height explícito ganha a ALTURA DA LINHA
            // (forced_outer_h) — os cards `.col` preenchem a linha.
            let item_align = it.align_self.unwrap_or(align);
            let stretches = item_align == crate::style::AlignItems::Stretch
                && it.can_stretch
                && !it.is_text
                && line_h > it.h;
            let off_cross = if stretches {
                0.0
            } else {
                align_offset(item_align, line_h, it.h)
            };
            let item_y = line_y + off_cross;
            if it.is_text {
                let text = collect_text(dom, it.node);
                let color = cor_visivel(&css, css.color.unwrap_or(0x000000FF));
                list.items.push(DisplayItem::Text {
                    x,
                    y: item_y,
                    text: text.into(),
                    color,
                    size: font_size,
                    mono: false,
                    bold: css.bold.unwrap_or(false),
                    italic: italico(Some(&css), tag_de(dom, it.node), false),
                    letter_spacing: css.letter_spacing.unwrap_or(0.0),
                    decoration: decoration_code(css),
                });
            } else {
                // o main resolvido é IMPOSTO ao item (grow/shrink venceram o
                // width); stretch impõe a altura da linha.
                let forced_h = if stretches { Some(line_h) } else { None };
                // `layout_block_reusing`, não `layout_block`: um item cujo
                // conteúdo não mudou (epoch igual) e cuja imposição de
                // distribuição (`Some(it.main)`/`forced_h`) bateu com a de um
                // frame anterior bate no cache — o CONTAINER continua sempre
                // recalculado (é ele quem decide `it.main`/`forced_h` de novo a
                // cada passada), só o item individual reusa. Margens não
                // participam do modelo de caixa de um item flex do jeito que
                // participam do fluxo de bloco (não colapsam com irmãos), então
                // a closure devolve zero — o valor nem é lido por quem chama.
                layout_block_reusing(
                    dom,
                    it.node,
                    x,
                    item_y,
                    content_w,
                    container_content_h,
                    || (0.0, 0.0),
                    Some(it.main),
                    forced_h,
                    true,
                    // Item de flex-row: mesma razão do flex-column, ver `coluna.rs`.
                    &BlockFormattingContext::new(),
                    ctx,
                    list,
                );
            }
            x += it.main;
        }
        line_y += line_h + row_gap + line_align_between;
    }
    // desconta o último row_gap (só ENTRE linhas, não após a última) e o
    // leading do align-content (a altura devolvida é a do CONTEÚDO, não a do
    // espaço que o align-content pode ter deixado antes da 1ª linha).
    let total_h = (line_y - row_gap - line_align_between - content_y - line_align_leading).max(0.0);
    total_h
}
