//! GRID: dimensionar as tracks (`fr`, `auto`, fixas), colocar os itens nas
//! células — por área nomeada ou automaticamente — e alinhá-los lá dentro.
//!
//! Movido de `layout.rs` na modularização; nenhuma linha de lógica foi
//! alterada — a reconstrução deste ficheiro é byte a byte a do original.

use super::*;
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
    let cells = place_grid_items(dom, &children, areas.as_deref(), ncols);

    // A largura INTRÍNSECA por coluna — só medida quando alguma trilha é
    // intrínseca, porque medir custa uma travessia por item e a esmagadora
    // maioria das grades é só `fr` e px.
    let precisa_medir = col_tracks
        .iter()
        .any(|t| matches!(t, crate::style::GridTrack::Auto));
    let conteudo: Option<Vec<f32>> = precisa_medir.then(|| {
        let mut w = vec![0.0f32; ncols];
        for c in &cells {
            // Um item que ATRAVESSA colunas não dita nenhuma delas sozinho: a
            // repartição do que ele pede pelas colunas que ocupa é a mesma
            // pergunta da tabela com `colspan`, e aqui não vale a complicação —
            // o que uma grade real tem em trilha intrínseca é a barra lateral,
            // que ocupa uma coluna só.
            if c.c1 - c.c0 != 1 || c.c0 >= ncols {
                continue;
            }
            w[c.c0] = w[c.c0].max(intrinsic_outer_width(dom, c.child, font_size, ctx));
        }
        w
    });
    let col_sizes = resolve_tracks(
        &col_tracks,
        content_w,
        col_gap,
        conteudo.as_deref(),
        &resolve,
    );
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
    let auto_row = css.grid_auto_rows;
    let has_explicit_row_track = |r: usize| explicit_rows.get(r).is_some() || auto_row.is_some();
    let mut row_sizes: Vec<f32> = (0..nrows)
        .map(|r| {
            let track = explicit_rows.get(r).copied().or(auto_row);
            match track {
                Some(crate::style::GridTrack::Fixed(d)) => {
                    resolve_height(Some(d), container_content_h, &resolve)
                        .unwrap_or(content_row_h[r])
                }
                _ => content_row_h[r], // Auto/None/Fr → conteúdo por ora (ajuste abaixo)
            }
        })
        .collect();
    // Se o container tem ALTURA definida e as linhas NÃO têm track explícita (auto),
    // as linhas DIVIDEM a altura do container (uma row auto num grid de altura fixa
    // preenche o espaço — é o que dá a track de 240 pro logo centrar). Distribui o
    // espaço livre igualmente entre as linhas auto (aproximação; fr real seria por
    // peso — mas grid sem template-rows usa 1fr implícito quando há altura).
    if let Some(ch) = container_content_h {
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

    // ── POSICIONA cada item na sua célula ────────────────────────────────────────
    let justify = css
        .grid_justify_items
        .unwrap_or(crate::style::AlignItems::Stretch);
    let align = css.align_items.unwrap_or(crate::style::AlignItems::Stretch);
    // x acumulado de cada coluna, y de cada linha.
    let mut col_x = vec![content_x; ncols + 1];
    for c in 0..ncols {
        col_x[c + 1] = col_x[c] + col_sizes[c.min(col_sizes.len() - 1)] + col_gap;
    }
    let mut row_y = vec![content_y; nrows + 1];
    for r in 0..nrows {
        row_y[r + 1] = row_y[r] + row_sizes[r] + row_gap;
    }
    for cell in &cells {
        let child = cell.child;
        let cell_x = col_x[cell.c0];
        let cell_y = row_y[cell.r0];
        let cell_w = span_size(&col_sizes, cell.c0, cell.c1, col_gap);
        let cell_h = span_size(&row_sizes, cell.r0, cell.r1.min(nrows), row_gap);
        // mede o tamanho natural do item (shrink) p/ o alinhamento não-stretch.
        let stretch_x = justify == crate::style::AlignItems::Stretch;
        let stretch_y = align == crate::style::AlignItems::Stretch;
        let (nat_w, nat_h) = measure_block(dom, child, cell_w, Some(cell_h), None, None, true, ctx);
        let iw = if stretch_x { cell_w } else { nat_w.min(cell_w) };
        let ih = if stretch_y { cell_h } else { nat_h.min(cell_h) };
        let x = cell_x + cell_align_offset(justify, cell_w, iw);
        let y = cell_y + cell_align_offset(align, cell_h, ih);
        // pinta o item: stretch no eixo → forced size; senão shrink-to-fit.
        let forced_w = if stretch_x { None } else { Some(iw) };
        let forced_h = if stretch_y { Some(cell_h) } else { None };
        layout_block(
            dom,
            child,
            x,
            y,
            cell_w,
            Some(cell_h),
            forced_w,
            forced_h,
            !stretch_x,
            &[],
            ctx,
            list,
        );
    }
    // altura total = soma das linhas + gaps.
    let total_h: f32 = row_sizes.iter().sum::<f32>() + (nrows.saturating_sub(1)) as f32 * row_gap;
    total_h.max(0.0)
}

/// Onde UM item do grid vive: a célula inicial e o span, em índices de trilha com
/// o fim exclusivo. É o resultado da colocação — nomeada ou automática — e o único
/// que o resto do layout de grid consome, o que é o que permite às duas colocações
/// coexistirem sem um segundo caminho de posicionamento.
struct GridCell {
    child: NodeIdx,
    r0: usize,
    c0: usize,
    r1: usize,
    c1: usize,
}

impl GridCell {
    fn rows(&self) -> usize {
        (self.r1 - self.r0).max(1)
    }
}

/// Coloca os filhos: quem tem `grid-area: <nome>` presente na matriz do container
/// vai para o retângulo daquele nome; o resto preenche a próxima célula LIVRE em
/// row-major.
///
/// Os nomeados são colocados ANTES (spec §8.5 passo 1) por uma razão concreta e não
/// por fidelidade: se os automáticos fossem primeiro, um item nomeado para a coluna
/// da direita encontraria a célula já ocupada e ou sobrepunha ou empurrava — que é o
/// empilhamento que as áreas existem para evitar.
fn place_grid_items(
    dom: &Dom,
    children: &[NodeIdx],
    areas: Option<&crate::style::GridAreas>,
    ncols: usize,
) -> Vec<GridCell> {
    let mut cells: Vec<GridCell> = Vec::with_capacity(children.len());
    // ocupação row-major, crescida sob demanda (o nº de linhas não é conhecido antes
    // de saber quantos itens sobram para a colocação automática).
    let mut taken: Vec<bool> = Vec::new();
    let mut mark = |taken: &mut Vec<bool>, r0: usize, c0: usize, r1: usize, c1: usize| {
        let need = r1 * ncols;
        if taken.len() < need {
            taken.resize(need, false);
        }
        for r in r0..r1 {
            for c in c0..c1.min(ncols) {
                taken[r * ncols + c] = true;
            }
        }
    };

    let mut auto: Vec<NodeIdx> = Vec::new();
    for &child in children {
        let name = dom
            .computed_style_idx(child)
            .and_then(|s| s.grid_area.clone());
        match name.and_then(|n| areas.and_then(|a| a.area(&n))) {
            Some(a) => {
                mark(&mut taken, a.r0, a.c0, a.r1, a.c1);
                cells.push(GridCell {
                    child,
                    r0: a.r0,
                    c0: a.c0,
                    r1: a.r1,
                    c1: a.c1.min(ncols),
                });
            }
            None => auto.push(child),
        }
    }

    // As linhas declaradas pela matriz contam como existentes mesmo sem item: um
    // automático não deve cair numa célula vazia RESERVADA (o `.` da matriz) antes
    // das linhas implícitas... mas cair nela é o comportamento da spec, então só as
    // células realmente ocupadas bloqueiam.
    let mut cursor = 0usize;
    for &child in &auto {
        while taken.get(cursor).copied().unwrap_or(false) {
            cursor += 1;
        }
        let (r, c) = (cursor / ncols, cursor % ncols);
        mark(&mut taken, r, c, r + 1, c + 1);
        cells.push(GridCell {
            child,
            r0: r,
            c0: c,
            r1: r + 1,
            c1: c + 1,
        });
        cursor += 1;
    }
    cells
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
/// A LARGURA (ou altura) de cada trilha de uma grade.
///
/// A ordem das três passadas é a regra, e não um detalhe de implementação: uma
/// trilha intrínseca (`auto`/`min-content`) é dimensionada pelo CONTEÚDO antes
/// de qualquer espaço livre ser repartido, porque o espaço livre só existe
/// depois de se saber o que o conteúdo pede. Inverter as duas é o que fazia a
/// grade do `<main>` da Wikipédia dar 948px à coluna de conteúdo e empurrar a
/// barra lateral para fora da janela.
///
/// `conteudo[i]` é a largura intrínseca dos itens da trilha `i` — `None` quando
/// quem chama não a mediu (nenhuma trilha intrínseca na lista, e aí ela não é
/// precisa).
fn resolve_tracks(
    tracks: &[crate::style::GridTrack],
    container: f32,
    gap: f32,
    conteudo: Option<&[f32]>,
    ctx: &ResolveCtx,
) -> Vec<f32> {
    use crate::style::GridTrack as T;
    let n = tracks.len().max(1);
    let total_gap = (n.saturating_sub(1)) as f32 * gap;
    let dim = |d: &crate::style::Dimension| -> f32 {
        match d {
            // % de trilha resolve contra o container (largura p/ colunas).
            crate::style::Dimension::Percent(p) => container * p / 100.0,
            other => other.resolve(ctx).unwrap_or(0.0),
        }
        .max(0.0)
    };

    // 1ª passada: a BASE de cada trilha — o que ela pede antes de haver sobra.
    let mut sizes = vec![0.0f32; tracks.len()];
    let mut sum_fr = 0.0f32;
    for (i, t) in tracks.iter().enumerate() {
        sizes[i] = match t {
            T::Fixed(d) => dim(d),
            T::Bounded { min, .. } => dim(min),
            T::Auto => conteudo
                .and_then(|c| c.get(i))
                .copied()
                .unwrap_or(0.0)
                .max(0.0),
            T::Fr(f) => {
                sum_fr += f.max(0.0);
                0.0
            }
        };
    }
    let free = (container - sizes.iter().sum::<f32>() - total_gap).max(0.0);

    // 2ª passada: o espaço livre. `fr` come-o todo quando existe — é o que a
    // unidade significa —, e nesse caso uma trilha limitada ou intrínseca fica
    // pela sua base.
    if sum_fr > 0.0 {
        for (i, t) in tracks.iter().enumerate() {
            if let T::Fr(f) = t {
                sizes[i] = free * f.max(0.0) / sum_fr;
            }
        }
        return sizes;
    }

    // 3ª passada, sem `fr`: primeiro as trilhas LIMITADAS crescem até ao seu
    // máximo (é o que `minmax` pede), e só o que sobrar depois disso é que
    // estica as intrínsecas — `align-content: stretch`, o default.
    let mut sobra = free;
    let limitadas: Vec<usize> = tracks
        .iter()
        .enumerate()
        .filter(|(_, t)| matches!(t, T::Bounded { .. }))
        .map(|(i, _)| i)
        .collect();
    if !limitadas.is_empty() && sobra > 0.0 {
        // Reparte por igual e não em proporção: a proporção seria contra as
        // bases, que num `minmax(0, x)` são todas zero.
        let quota = sobra / limitadas.len() as f32;
        for i in limitadas {
            if let T::Bounded { max, .. } = &tracks[i] {
                let teto = dim(max);
                let novo = (sizes[i] + quota).min(teto);
                sobra -= novo - sizes[i];
                sizes[i] = novo;
            }
        }
    }
    let autos: Vec<usize> = tracks
        .iter()
        .enumerate()
        .filter(|(_, t)| matches!(t, T::Auto))
        .map(|(i, _)| i)
        .collect();
    if !autos.is_empty() && sobra > 0.0 {
        let cada = sobra / autos.len() as f32;
        for i in autos {
            sizes[i] += cada;
        }
    }
    sizes
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
