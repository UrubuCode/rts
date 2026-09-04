//! GRID: dimensionar as tracks (`fr`, `auto`, fixas), colocar os itens nas
//! células — por área nomeada ou automaticamente — e alinhá-los lá dentro.
//!
//! Movido de `layout.rs` na modularização; nenhuma linha de lógica foi
//! alterada — a reconstrução deste ficheiro é byte a byte a do original.

use super::*;
use super::grid_linhas::place_grid_items;
use super::grid_tracks;
/// Dispõe os filhos como FLEX COLUMN (`display:flex; flex-direction:column`): o
/// eixo PRINCIPAL é o vertical. Diferenças do block vertical: SEM margin-collapse
/// (flex não colapsa margens), `gap` entre itens (em column o espaçamento main é o
/// `row-gap`; o shorthand `gap:` seta ambos), `justify-content` distribui o espaço
/// livre VERTICAL (só quando o container tem altura explícita), `margin-top/bottom:
/// auto` de um item ABSORVE o espaço livre (spec flexbox §8.1 — é o `mb-auto`/
/// `mt-auto` do Bootstrap empurrando header/footer para as pontas), e `align-items`
/// atua no X: `stretch` (default) = item ocupa a largura; start/center/end = item
/// shrink-to-fit deslocado. Devolve a altura natural do content.
/// ⚠️ Cortes: `column-reverse` dispõe como `column` (sem inverter); `flex-wrap` em
/// column (multi-coluna) trata como coluna única; `flex-grow/shrink/basis` ainda
/// fora (fatia própria).
/// GRID real (css-grid track-sizing simplificado): resolve as trilhas de coluna
/// (px/%/fr/auto) e de linha, faz auto-placement dos itens célula-a-célula
/// (row-by-row), e posiciona cada item na sua célula com `justify-items`
/// (horizontal) / `align-items` (vertical). Suporta o subset do MDN:
/// grid-template-columns/rows, grid-auto-rows, gap, repeat(N,...), minmax(→max),
/// fr. NÃO suporta: grid-column/row-span explícito, areas, auto-fill/fit reais,
/// dense. Um item sem placement explícito preenche a próxima célula livre.
#[allow(clippy::too_many_arguments)]
pub(in crate::layout) fn layout_children_grid(
    dom: &Dom,
    id: NodeIdx,
    content_x: f32,
    content_y: f32,
    content_w: f32,
    container_content_h: Option<f32>,
    css: &ComputedStyle,
    font_size: f32,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) -> f32 {
    let resolve = ResolveCtx {
        parent_content_w: content_w,
        node_font_size: font_size,
        root_font_size: crate::style::root_font_size(),
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    let col_gap = css
        .gap
        .or(css.row_gap)
        .and_then(|d| d.resolve(&resolve))
        .unwrap_or(0.0)
        .max(0.0);
    let row_gap = css
        .row_gap
        .or(css.gap)
        .and_then(|d| d.resolve(&resolve))
        .unwrap_or(0.0)
        .max(0.0);

    // ── COLUNAS: resolve as trilhas ──────────────────────────────────────────────
    // Sem grid-template-columns explícito → 1 coluna 1fr (o container-do-logo do
    // google: single-column grid). Com N colunas do grid_columns legado (repeat) →
    // N trilhas 1fr.
    let areas = css.grid_template_areas.clone();
    let col_tracks: Vec<crate::style::GridTrack> = match &css.grid_template_columns {
        Some(t) => (**t).clone(),
        // Sem trilhas declaradas mas COM áreas, é a matriz que diz quantas colunas
        // existem — cair no default de 1 coluna empilharia lado e conteúdo, que é
        // exatamente o sintoma que as áreas existem para resolver.
        None => {
            let n = match &areas {
                Some(a) => a.cols,
                None => css.grid_columns.unwrap_or(1).max(1) as usize,
            };
            vec![crate::style::GridTrack::Fr(1.0); n]
        }
    };
    // `repeat(auto-fill|auto-fit, …)`: o Nº de repetições é decidido AGORA,
    // contra `content_w` — antes da colocação, porque a colocação já precisa
    // de saber quantas colunas existem (CSS Grid 1 §7.2.3.3, "the number of
    // times to repeat the track list"). Ver `layout::grid_tracks`.
    let (col_tracks, col_collapsible) =
        grid_tracks::expand_auto_repeats(col_tracks, content_w, col_gap);
    // O número de colunas vem da LISTA de trilhas e não dos tamanhos: os
    // tamanhos ainda não estão decididos, porque uma trilha intrínseca precisa de
    // saber que itens lhe calham — e para isso é preciso ter colocado os itens.
    // A ordem é: quantas colunas → colocar os itens → medir → dimensionar.
    let ncols = col_tracks.len().max(1);

    // ── ITENS: os filhos renderizáveis (auto-placement row-by-row) ───────────────
    let mut children: Vec<NodeIdx> = Vec::new();
    for &child in &dom.node(id).children {
        if let NodeKind::Element { tag } = &dom.node(child).kind {
            if is_non_rendered_tag(tag) {
                continue;
            }
        }
        if is_out_of_flow(dom, child) {
            continue;
        }
        if !is_block_level(dom, child) && collect_text(dom, child).trim().is_empty() {
            continue;
        }
        children.push(child);
    }
    if children.is_empty() {
        return 0.0;
    }
    let explicit_rows_n = css.grid_template_rows.as_ref().map(|t| t.len()).unwrap_or(0);
    let auto_flow = css.grid_auto_flow.unwrap_or(crate::style::grid_lines::GridAutoFlow {
        coluna: false,
        dense: false,
    });
    let (cells, ncols_colocados) =
        place_grid_items(dom, &children, areas.as_deref(), ncols, explicit_rows_n, auto_flow);
    // COLUNAS IMPLÍCITAS: um `grid-area`/`grid-column` que aponta lá da última
    // coluna explícita fez `place_grid_items` devolver mais colunas do que as
    // declaradas — estende `col_tracks` com `grid-auto-columns` (por omissão
    // `auto`) para as colunas extra, e re-conta `ncols` a partir daqui.
    let mut col_tracks = col_tracks;
    let mut col_collapsible = col_collapsible;
    while col_tracks.len() < ncols_colocados {
        col_tracks.push(
            css.grid_auto_columns
                .clone()
                .unwrap_or(crate::style::GridTrack::Auto),
        );
        col_collapsible.push(false);
    }
    let ncols = col_tracks.len().max(1);

    // A largura INTRÍNSECA por coluna — só medida quando alguma trilha PEDE
    // conteúdo (`Auto` ou `Intrinsic`, que pode precisar do min-content além
    // do max-content), porque medir custa uma travessia por item e a
    // esmagadora maioria das grades é só `fr` e px.
    let precisa_min = col_tracks
        .iter()
        .any(|t| matches!(t, crate::style::GridTrack::Intrinsic { .. }));
    let precisa_medir = precisa_min
        || col_tracks
            .iter()
            .any(|t| matches!(t, crate::style::GridTrack::Auto));
    let (conteudo, conteudo_min): (Option<Vec<f32>>, Option<Vec<f32>>) = if precisa_medir {
        let mut wmax = vec![0.0f32; ncols];
        let mut wmin = vec![0.0f32; ncols];
        for c in &cells {
            // Um item que ATRAVESSA colunas não dita nenhuma delas sozinho: a
            // repartição do que ele pede pelas colunas que ocupa é a mesma
            // pergunta da tabela com `colspan`, e aqui não vale a complicação —
            // o que uma grade real tem em trilha intrínseca é a barra lateral,
            // que ocupa uma coluna só.
            if c.c1 - c.c0 != 1 || c.c0 >= ncols {
                continue;
            }
            wmax[c.c0] = wmax[c.c0].max(intrinsic_outer_width(dom, c.child, font_size, ctx));
            if precisa_min {
                wmin[c.c0] = wmin[c.c0].max(crate::table::min_content(dom, c.child, font_size, ctx));
            }
        }
        (Some(wmax), precisa_min.then_some(wmin))
    } else {
        (None, None)
    };
    let mut col_sizes = grid_tracks::resolve_tracks(
        &col_tracks,
        content_w,
        col_gap,
        conteudo.as_deref(),
        conteudo_min.as_deref(),
        &resolve,
    );
    // `auto-fit`: as repetições sem NENHUM item colapsam a 0 (§7.2.3.3) — o
    // que `auto-fill` distingue de `auto-fit` é só isto, e só depois de saber
    // que colunas os itens realmente ocupam.
    if col_collapsible.iter().any(|&c| c) {
        let mut occupied = vec![false; ncols];
        for cell in &cells {
            for c in cell.c0..cell.c1.min(ncols) {
                occupied[c] = true;
            }
        }
        grid_tracks::collapse_empty_auto_fit_tracks(&mut col_sizes, &col_collapsible, &occupied);
    }
    // O computed style do Blink pode consultar o LayoutObject para propriedades
    // dependentes de used values. Guardamos a mesma resolução no container para o
    // DOM a serializar sem executar um segundo algoritmo de track sizing.
    if css.grid_template_columns.is_some() {
        list.grid_column_tracks.insert(id, col_sizes.clone());
    }
    // Uma linha DECLARADA pela matriz existe mesmo sem item nela (ela ainda empurra
    // as linhas seguintes pelo gap), daí o max com `areas.rows`.
    let nrows = cells
        .iter()
        .map(|c| c.r1)
        .max()
        .unwrap_or(1)
        .max(areas.as_ref().map(|a| a.rows).unwrap_or(0))
        .max(1);

    // ── LINHAS: altura de cada linha ─────────────────────────────────────────────
    // grid-template-rows explícito (px/%/fr/auto), senão grid-auto-rows, senão a
    // altura do conteúdo mais alto da linha. `fr`/`%` de linha precisam da altura
    // do container (container_content_h).
    let explicit_rows: Vec<crate::style::GridTrack> = css
        .grid_template_rows
        .as_ref()
        .map(|t| (**t).clone())
        .unwrap_or_default();
    // mede a altura de conteúdo de cada linha (o item mais alto medido em shrink).
    // Um item que ATRAVESSA linhas reparte a sua altura IGUALMENTE pelas linhas do
    // span. O algoritmo da spec (§12.5) distribui pela contribuição de cada trilha;
    // a repartição igual foi escolhida por não precisar de uma segunda medição e por
    // errar sempre para MAIS espaço, nunca para item cortado.
    let mut content_row_h = vec![0.0f32; nrows];
    for cell in &cells {
        let cw = span_size(&col_sizes, cell.c0, cell.c1, col_gap);
        let (_, h) = measure_block(
            dom,
            cell.child,
            cw,
            container_content_h,
            None,
            None,
            true,
            ctx,
        );
        let each = h / cell.rows() as f32;
        for r in cell.r0..cell.r1.min(nrows) {
            content_row_h[r] = content_row_h[r].max(each);
        }
    }
    let auto_row = css.grid_auto_rows.clone();
    // Só `Fixed`/`Bounded` conta como "já dimensionada": `fr` existe PARA tomar
    // espaço livre, mas caía no mesmo `explicit_rows.get(r).is_some()` que uma
    // `Fixed` e ficava fora do laço de reparto abaixo — a linha do meio de
    // `grid-template-rows: 60px 1fr 40px` media pelo CONTEÚDO (0 numa div vazia)
    // e o rodapé subia para y=60 em vez de y=360
    // (`tests/css/claude-grid-areas.html`, `#corpo.h`/`#lateral.h`/`#rodape.y`).
    // As colunas nunca tiveram este bug: passam por `resolve_tracks`, que já
    // distingue os quatro casos; as linhas tinham um segundo algoritmo à parte.
    let is_fixed_row_track = |t: &Option<crate::style::GridTrack>| {
        matches!(
            t,
            Some(crate::style::GridTrack::Fixed(_))
                | Some(crate::style::GridTrack::Bounded { .. })
        )
    };
    let row_track = |r: usize| explicit_rows.get(r).cloned().or_else(|| auto_row.clone());
    let has_explicit_row_track = |r: usize| is_fixed_row_track(&row_track(r));
    let mut row_sizes: Vec<f32> = (0..nrows)
        .map(|r| {
            let track = row_track(r);
            match track {
                Some(crate::style::GridTrack::Fixed(d)) => {
                    resolve_height(Some(d), container_content_h, &resolve)
                        .unwrap_or(content_row_h[r])
                }
                _ => content_row_h[r], // Auto/None/Fr → conteúdo por ora (ajuste abaixo)
            }
        })
        .collect();
    // Se o container tem ALTURA definida e as linhas NÃO têm track FIXA, as linhas
    // DIVIDEM a altura do container entre si — uma row `auto` ou `fr` num grid de
    // altura fixa preenche o espaço (dá a track de 240 pro logo centrar, e a de
    // 300 pro `1fr` do meio da fixture de áreas). Reparte o espaço livre em
    // partes iguais (aproximação; `fr` por peso fica por fazer — nenhuma fixture
    // do corpus tem mais de uma trilha flexível por eixo hoje).
    // `align-content` explícito (não-stretch: `stretch`/`normal` não parseiam
    // em `JustifyContent` — ver o cabeçalho da tabela — e por isso caem em
    // `None`, que é exatamente o ramo que preserva o preenchimento acima) SUBSTITUI
    // o preenchimento por espaço-livre-em-linhas-auto pela distribuição das
    // LINHAS como blocos, via `row_align_leading`/`row_align_between` abaixo —
    // as linhas mantêm o tamanho do conteúdo em vez de esticar.
    let mut row_align_leading = 0.0f32;
    let mut row_align_between = 0.0f32;
    if let Some(v) = css.align_content {
        if let Some(ch) = container_content_h {
            let used: f32 = row_sizes.iter().sum::<f32>() + (nrows.saturating_sub(1)) as f32 * row_gap;
            let free = (ch - used).max(0.0);
            let (leading, between) = crate::layout::coluna::justify_offsets(v, free, nrows);
            row_align_leading = leading;
            row_align_between = between;
        }
    } else if let Some(ch) = container_content_h {
        let auto_rows: Vec<usize> = (0..nrows).filter(|&r| !has_explicit_row_track(r)).collect();
        if !auto_rows.is_empty() {
            let fixed: f32 = (0..nrows)
                .filter(|r| has_explicit_row_track(*r))
                .map(|r| row_sizes[r])
                .sum();
            let total_gap = (nrows.saturating_sub(1)) as f32 * row_gap;
            let free = (ch - fixed - total_gap).max(0.0);
            let each = free / auto_rows.len() as f32;
            for r in auto_rows {
                row_sizes[r] = row_sizes[r].max(each);
            }
        }
    }
    // `justify-content` da grelha: quando as trilhas (sem `fr`, todas fixas)
    // não enchem `content_w`, distribui o sobrante entre/antes das colunas —
    // o mesmo `justify_offsets` do flex, reusado (spec §8.4 partilha o
    // vocabulário com o flex). Sem trilha `fr`/`auto` para o comer, o
    // sobrante fica por gastar até aqui; com `justify-content` ausente
    // (`None`) o comportamento é `start` (sem offset), que é o que já
    // acontecia.
    let mut col_justify_leading = 0.0f32;
    let mut col_justify_between = 0.0f32;
    if let Some(j) = css.justify {
        let used: f32 = col_sizes.iter().sum::<f32>() + (ncols.saturating_sub(1)) as f32 * col_gap;
        let free = (content_w - used).max(0.0);
        let (leading, between) = crate::layout::coluna::justify_offsets(j, free, ncols);
        col_justify_leading = leading;
        col_justify_between = between;
    }

    // ── POSICIONA cada item na sua célula ────────────────────────────────────────
    let justify = css
        .grid_justify_items
        .unwrap_or(crate::style::AlignItems::Stretch);
    let align = css.align_items.unwrap_or(crate::style::AlignItems::Stretch);
    // x acumulado de cada coluna, y de cada linha (com o offset de
    // `justify-content`/`align-content` já embutido).
    let mut col_x = vec![content_x + col_justify_leading; ncols + 1];
    for c in 0..ncols {
        col_x[c + 1] = col_x[c] + col_sizes[c.min(col_sizes.len() - 1)] + col_gap + col_justify_between;
    }
    let mut row_y = vec![content_y + row_align_leading; nrows + 1];
    for r in 0..nrows {
        row_y[r + 1] = row_y[r] + row_sizes[r] + row_gap + row_align_between;
    }
    for cell in &cells {
        let child = cell.child;
        let cell_x = col_x[cell.c0];
        let cell_y = row_y[cell.r0];
        let cell_w = span_size(&col_sizes, cell.c0, cell.c1, col_gap);
        let cell_h = span_size(&row_sizes, cell.r0, cell.r1.min(nrows), row_gap);
        // `justify-self`/`align-self` do ITEM vencem `justify-items`/`align-items`
        // do container — mesma prioridade de `align-self` no flex.
        let item_css = dom.computed_style_idx(child).unwrap_or_default();
        let justify = item_css.justify_self.unwrap_or(justify);
        let align = item_css.align_self.unwrap_or(align);
        // mede o tamanho natural do item (shrink) p/ o alinhamento não-stretch.
        //
        // `stretch` só estica um eixo cujo tamanho é `auto` (spec §11.7 /
        // css-align §7.1: "stretch — if the item's used cross-size is
        // auto..."). Um `width`/`height` DECLARADO no item vence — o mesmo
        // corte que o flex já tinha (`can_stretch` em `flex.rs`) e que o grid
        // não tinha: sem isto, `#item1`/`#item3`/`#item4` de
        // `claude-grid-alinhamento.html` (que declaram `height:30px` mas
        // NENHUM `align-self`, logo caem no `align-items:stretch` default do
        // container) ganhavam a altura da CÉLULA (50px) em vez da declarada
        // (30px) — medido pelo orquestrador contra o Chrome.
        let stretch_x = justify == crate::style::AlignItems::Stretch && item_css.width.is_none();
        let stretch_y = align == crate::style::AlignItems::Stretch && item_css.height.is_none();
        let (nat_w, nat_h) = measure_block(dom, child, cell_w, Some(cell_h), None, None, true, ctx);
        let iw = if stretch_x { cell_w } else { nat_w.min(cell_w) };
        let ih = if stretch_y { cell_h } else { nat_h.min(cell_h) };
        let x = cell_x + cell_align_offset(justify, cell_w, iw);
        let y = cell_y + cell_align_offset(align, cell_h, ih);
        // pinta o item: stretch no eixo → forced size; senão shrink-to-fit.
        let forced_w = if stretch_x { None } else { Some(iw) };
        let forced_h = if stretch_y { Some(cell_h) } else { None };
        // `layout_block_reusing`: mesma razão do flex/coluna — o container
        // (aqui, a passada de posicionamento das células) recalcula sempre,
        // o item individual bate no cache por `FragmentKey` quando nada dele
        // ou da célula que o impõe mudou.
        layout_block_reusing(
            dom,
            child,
            x,
            y,
            cell_w,
            Some(cell_h),
            || (0.0, 0.0),
            forced_w,
            forced_h,
            false,
            !stretch_x,
            // Item de grid: mesma razão do flex, ver `coluna.rs`.
            &BlockFormattingContext::new(),
            ctx,
            list,
        );
    }
    // altura total = soma das linhas + gaps.
    let total_h: f32 = row_sizes.iter().sum::<f32>() + (nrows.saturating_sub(1)) as f32 * row_gap;
    total_h.max(0.0)
}

/// Soma os tamanhos das trilhas `start..end` mais os gaps entre elas — o tamanho de
/// uma célula, que para span 1 é a trilha e para span N inclui os gaps que o span
/// atravessa (um span de 2 colunas cobre o gap do meio, não o perde).
fn span_size(sizes: &[f32], start: usize, end: usize, gap: f32) -> f32 {
    if sizes.is_empty() {
        return 0.0;
    }
    let end = end.max(start + 1).min(sizes.len());
    let start = start.min(sizes.len() - 1);
    let n = end.saturating_sub(start);
    sizes[start..end].iter().sum::<f32>() + (n.saturating_sub(1)) as f32 * gap
}
/// Offset de alinhamento de um item de tamanho `item` dentro de uma célula de
/// tamanho `cell` (start=0, center=(cell-item)/2, end=cell-item; stretch=0).
fn cell_align_offset(a: crate::style::AlignItems, cell: f32, item: f32) -> f32 {
    match a {
        crate::style::AlignItems::Center => ((cell - item) / 2.0).max(0.0),
        crate::style::AlignItems::FlexEnd => (cell - item).max(0.0),
        _ => 0.0, // FlexStart / Stretch
    }
}
