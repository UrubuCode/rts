//! LAYOUT DE TABELA — `display: table` e os seus quatro papéis internos.
//!
//! ## Por que é um algoritmo à parte
//!
//! Todos os outros modos deste motor decidem a caixa de um filho olhando para
//! esse filho. Uma tabela não pode: a largura de uma célula é decidida pela
//! COLUNA, e a coluna é decidida por células de outras linhas que ainda não
//! foram visitadas. Por isso a tabela não é um `layout_children_*` a mais — é
//! uma passada que constrói a grade INTEIRA antes de posicionar o que quer que
//! seja. A repartição de larguras, que é a parte difícil, está em [`widths`].
//!
//! ## O que este módulo NÃO faz, e porquê está dito
//!
//! - `border-collapse: collapse` funde as bordas adjacentes escolhendo a mais
//!   grossa; aqui a fusão é o caso simples — os vãos vão a zero e cada célula
//!   pinta a sua borda. Duas bordas de 1px lado a lado ficam com 2px de tinta
//!   onde o Chrome pinta 1px. Muda a cor de um traço, não a posição de nada.
//! - `<caption>`, `<col>` e `<colgroup>` não dimensionam colunas — a largura de
//!   um `<col>` é lida do atributo `width`, o resto do seu CSS não.
//! - `rowspan` reserva a célula na grade (nenhuma outra ocupa aquele lugar) e a
//!   sua altura é cobrada à ÚLTIMA linha que atravessa. É onde o browser a cobra
//!   quando as linhas não têm altura própria, que é o caso normal.

mod grid;
pub(in crate::table) mod widths;

#[cfg(test)]
pub(crate) mod tests;
#[cfg(test)]
mod tests_listas;

use crate::layout::{DisplayItem, DisplayList, LayoutCtx, Rect};
use crate::style::{ComputedStyle, DisplayKind};
use crate::{Dom, NodeIdx};
use grid::collect;

/// Uma célula colocada na grade. `col` é a coluna onde começa, já resolvida
/// contra os `rowspan` que vêm de cima.
struct Cell {
    node: NodeIdx,
    col: usize,
    colspan: usize,
    rowspan: usize,
}

/// Uma linha, com as suas células.
///
/// `node` é `None` para uma linha ANÓNIMA — a que o CSS gera à volta de células
/// que não têm um `table-row` por pai (§17.2.1). Não é um caso de canto: é o que
/// a Wikipédia escreve com dois `<div style="display:table-cell">` dentro de um
/// `<div style="display:table">`, e sem a linha anónima essas células empilham
/// à largura toda em vez de ficarem lado a lado. Uma linha anónima não recebe
/// caixa registada nem pinta nada, porque não existe elemento a que pertença.
struct Row {
    node: Option<NodeIdx>,
    cells: Vec<Cell>,
}

/// A grade inteira, construída antes de qualquer posicionamento.
struct Grid {
    rows: Vec<Row>,
    /// `(nó do grupo, primeira linha, nº de linhas)` — só os grupos que existem
    /// no markup; linhas soltas dentro do `<table>` não criam grupo.
    groups: Vec<(NodeIdx, usize, usize)>,
    /// Blocos que não são parte da grade (`<caption>` e afins), na ordem em que
    /// aparecem. Vão empilhados ACIMA da grade.
    outros: Vec<NodeIdx>,
    cols: usize,
}

/// Os parâmetros da tabela, já resolvidos em pontos: os três valores que
/// decidem a grade antes de qualquer célula ser medida.
///
/// Duas fontes, e a precedência é a do browser: **o CSS vence o atributo HTML**.
/// Os atributos (`cellspacing`, `border`) não são legado a tolerar — são
/// apresentação que o HTML define e que o browser honra quando não há CSS a
/// dizer o contrário, e páginas reais (a Wikipédia inteira) ainda os escrevem.
struct TableStyle {
    /// Vão horizontal e vertical entre células. Zero quando as bordas fundem.
    spacing_h: f32,
    spacing_v: f32,
    /// `table-layout: fixed` — as larguras vêm da primeira linha e nenhuma
    /// célula é medida.
    fixed: bool,
}

impl TableStyle {
    fn of(dom: &Dom, id: NodeIdx, css: &ComputedStyle, font: f32, ctx: &LayoutCtx) -> TableStyle {
        let resolve = crate::style::ResolveCtx {
            parent_content_w: ctx.viewport_w,
            node_font_size: font,
            root_font_size: crate::layout::DEFAULT_FONT_SIZE,
            viewport_w: ctx.viewport_w,
            viewport_h: ctx.viewport_h,
        };
        // `border-collapse: collapse` ANULA o vão — não o reduz, anula-o. É por
        // isso que a pergunta vem primeiro: com bordas fundidas não há
        // `border-spacing` nenhum a resolver, e resolvê-lo para o deitar fora
        // deixaria o leitor à espera de que ele contasse para alguma coisa.
        if css.border_collapse == Some(crate::style::BorderCollapse::Collapse) {
            return TableStyle {
                spacing_h: 0.0,
                spacing_v: 0.0,
                fixed: Self::fixo(css),
            };
        }
        let (h, v) = match css.border_spacing {
            Some(s) => (
                s.h.resolve(&resolve).unwrap_or(0.0).max(0.0),
                s.v.resolve(&resolve).unwrap_or(0.0).max(0.0),
            ),
            // Sem CSS: o atributo `cellspacing`, e sem ele os 2px da folha de
            // estilo do browser para uma tabela `separate`.
            None => {
                let a = dom
                    .node(id)
                    .attr("cellspacing")
                    .and_then(|v| v.trim().trim_end_matches("px").parse::<f32>().ok())
                    .unwrap_or(2.0)
                    .max(0.0);
                (a, a)
            }
        };
        TableStyle {
            spacing_h: h,
            spacing_v: v,
            fixed: Self::fixo(css),
        }
    }

    fn fixo(css: &ComputedStyle) -> bool {
        css.table_layout == Some(crate::style::TableLayout::Fixed)
    }
}

/// A largura MAX-CONTENT de uma tabela: a soma das colunas mais os vãos. É o que
/// faz uma `<table>` sem `width` ser *shrink-to-fit* — encolher ao conteúdo em
/// vez de ocupar o pai, que é a diferença mais visível entre uma tabela e um
/// `<div>`.
pub(crate) fn max_content_width(dom: &Dom, table: NodeIdx, font: f32, ctx: &LayoutCtx) -> f32 {
    let g = collect(dom, table);
    if g.cols == 0 {
        return 0.0;
    }
    let css = dom.computed_style_idx(table).unwrap_or_default();
    let ts = TableStyle::of(dom, table, &css, font, ctx);
    let cols = medir_colunas(dom, &g, font, ctx, ts.spacing_h);
    let vaos = (g.cols + 1) as f32 * ts.spacing_h;
    let soma_maximos = cols.iter().map(|c| c.max).sum::<f32>() + vaos;
    // Uma percentagem de coluna é relativa à largura útil da própria tabela. Se
    // o mínimo intrínseco da coluna não cabe nessa percentagem, a tabela precisa
    // crescer até satisfazer `percentagem * largura_util >= mínimo`. Ignorar esta
    // relação fazia uma tabela aninhada com `width:1%` e mínimo de 100px responder
    // apenas à soma dos máximos, encolhendo para 300px em vez de ocupar a célula.
    let minimo_percentual = cols
        .iter()
        .filter_map(|c| c.percentagem.filter(|p| *p > 0.0).map(|p| c.min * 100.0 / p))
        .fold(0.0f32, f32::max)
        + vaos;
    soma_maximos.max(minimo_percentual)
}

fn medir_colunas(
    dom: &Dom,
    g: &Grid,
    font: f32,
    ctx: &LayoutCtx,
    spacing: f32,
) -> Vec<widths::Coluna> {
    let mut medidas = Vec::new();
    for row in &g.rows {
        for c in &row.cells {
            medidas.push((
                c.col,
                c.colspan,
                widths::cell_min_max(dom, c.node, font, ctx),
            ));
        }
    }
    widths::colunas_min_max(&medidas, g.cols, spacing)
}

/// Posiciona a tabela inteira dentro do content-box já resolvido pelo
/// `layout_block` da `<table>`, e devolve a ALTURA do conteúdo.
#[allow(clippy::too_many_arguments)]
pub(crate) fn layout_table(
    dom: &Dom,
    id: NodeIdx,
    content_x: f32,
    content_y: f32,
    content_w: f32,
    css: &ComputedStyle,
    font_size: f32,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) -> f32 {
    let g = collect(dom, id);
    let ts = TableStyle::of(dom, id, css, font_size, ctx);
    let mut y = content_y;

    // `<caption>` e outros blocos avulsos: empilham acima da grade, à largura da
    // tabela. Ficam fora do algoritmo de colunas de propósito — não têm coluna.
    for &o in &g.outros {
        let (_, h) = crate::layout::layout_block(
            dom,
            o,
            content_x,
            y,
            content_w,
            None,
            None,
            None,
            false,
            // `<caption>`/bloco avulso da tabela: sem float dentro de uma
            // tabela (a tabela já é BFC), um contexto novo é o mesmo `&[]`.
            &crate::layout::BlockFormattingContext::new(),
            ctx,
            list,
        );
        y += h;
    }
    if g.cols == 0 || g.rows.is_empty() {
        return y - content_y;
    }

    // ── Larguras das colunas ────────────────────────────────────────────────────
    // Os vãos são cols+1: `border-spacing` também existe entre a borda da tabela
    // e a primeira coluna (é o que separa a grade da moldura no default do
    // browser), e esquecê-lo faz a tabela inteira nascer 2px à esquerda.
    let vaos = (g.cols + 1) as f32 * ts.spacing_h;
    let disponivel = (content_w - vaos).max(0.0);
    let larguras = if ts.fixed {
        let mut declaradas = vec![None; g.cols];
        // Só a PRIMEIRA linha, e só o que ela DECLARA: uma coluna sem largura
        // declarada fica a `None` para receber a sua fatia do que sobra. Medir o
        // conteúdo aqui desfaria o algoritmo — o `fixed` existe exatamente para
        // não medir nada.
        if let Some(primeira) = g.rows.first() {
            for c in &primeira.cells {
                if c.colspan == 1 && c.col < g.cols {
                    declaradas[c.col] = widths::largura_declarada(dom, c.node, font_size, ctx);
                }
            }
        }
        widths::resolve_fixo(&declaradas, disponivel)
    } else {
        widths::resolve_colunas(
            &medir_colunas(dom, &g, font_size, ctx, ts.spacing_h),
            disponivel,
        )
    };

    // x de cada coluna, uma vez só (todas as linhas partilham).
    let mut col_x = Vec::with_capacity(g.cols);
    let mut acc = content_x + ts.spacing_h;
    for w in &larguras {
        col_x.push(acc);
        acc += w + ts.spacing_h;
    }
    let largura_de = |col: usize, span: usize| -> f32 {
        let fim = (col + span).min(g.cols);
        larguras[col..fim].iter().sum::<f32>() + (fim - col).saturating_sub(1) as f32 * ts.spacing_h
    };

    // ── Alturas: medir antes de posicionar ──────────────────────────────────────
    // Uma linha é tão alta quanto a sua célula mais alta, e a altura de uma
    // célula só se sabe depois de lhe dar a largura da coluna. Daí a passada de
    // medição: posicionar e depois esticar exigiria refazer o layout de cada
    // célula duas vezes na display list, o que duplicaria a tinta.
    let mut alturas = vec![0.0f32; g.rows.len()];
    let mut pendentes: Vec<(usize, usize, f32)> = Vec::new(); // (linha final, nº linhas, altura)
    for (ri, row) in g.rows.iter().enumerate() {
        for c in &row.cells {
            if c.col >= g.cols {
                continue;
            }
            let w = largura_de(c.col, c.colspan);
            let (_, h) =
                crate::layout::measure_block(dom, c.node, w, None, Some(w), None, false, ctx);
            if c.rowspan <= 1 {
                alturas[ri] = alturas[ri].max(h);
            } else {
                let ultima = (ri + c.rowspan - 1).min(g.rows.len() - 1);
                pendentes.push((ultima, ultima - ri + 1, h));
            }
        }
    }
    // As que atravessam linhas só cobram o que FALTA depois de somadas as linhas
    // que já têm altura própria — uma célula alta sobre duas linhas cheias não
    // deve esticar nada.
    for (ultima, n, h) in pendentes {
        let inicio = ultima + 1 - n;
        let ja: f32 = alturas[inicio..=ultima].iter().sum::<f32>() + (n - 1) as f32 * ts.spacing_v;
        if h > ja {
            alturas[ultima] += h - ja;
        }
    }

    // ── Posicionamento ──────────────────────────────────────────────────────────
    let mut row_y = Vec::with_capacity(g.rows.len());
    y += ts.spacing_v;
    for (ri, row) in g.rows.iter().enumerate() {
        row_y.push(y);
        // Reserva o índice ANTES das células: o fundo do `<tr>` pinta-se atrás
        // delas, como qualquer caixa pinta atrás dos filhos.
        let idx_fundo = list.items.len();
        // A fronteira das subárvores que já existem — ver `layout::insert_item`.
        let filhos_antes = list.children.len();
        if let Some(n) = row.node {
            crate::layout::reserve_node_order(list, n);
        }
        for c in &row.cells {
            if c.col >= g.cols {
                continue;
            }
            let w = largura_de(c.col, c.colspan);
            let fim = (ri + c.rowspan - 1).min(g.rows.len() - 1);
            let h: f32 = alturas[ri..=fim].iter().sum::<f32>() + (fim - ri) as f32 * ts.spacing_v;
            crate::layout::layout_block(
                dom,
                c.node,
                col_x[c.col],
                y,
                w,
                Some(h),
                Some(w),
                Some(h),
                false,
                // Célula de tabela: mesma razão do `<caption>` acima.
                &crate::layout::BlockFormattingContext::new(),
                ctx,
                list,
            );
        }
        // Uma linha ANÓNIMA não tem nó: não há caixa a registar nem fundo a
        // pintar, e forçar um dos dois inventaria geometria para um elemento
        // que não existe no documento.
        if let Some(n) = row.node {
            let rect = Rect::new(content_x, y, content_w, alturas[ri]);
            crate::layout::record_node_rect(list, n, rect);
            pinta_caixa(dom, n, rect, idx_fundo, filhos_antes, list);
        }
        y += alturas[ri] + ts.spacing_v;
    }

    // Os GRUPOS (`<tbody>`/`<thead>`) recebem a caixa que abrange as suas linhas.
    // Vão por último na lista mas com `insert` no índice da primeira linha do
    // grupo, para ficarem atrás dela — um `<thead>` com fundo tapava as células
    // se fosse acrescentado no fim.
    for &(node, inicio, n) in &g.groups {
        if n == 0 {
            continue;
        }
        let topo = row_y[inicio];
        let base = row_y[inicio + n - 1] + alturas[inicio + n - 1];
        let rect = Rect::new(content_x, topo, content_w, base - topo);
        crate::layout::record_node_rect(list, node, rect);
        pinta_caixa(dom, node, rect, list.items.len(), list.children.len(), list);
    }
    y - content_y
}

/// Pinta fundo e borda de uma caixa que não passou pelo `layout_block` (linha ou
/// grupo de linhas), inserindo os itens em `at` para ficarem ATRÁS do que já lá
/// está. Sem isto, um `<tr>` com `background` não pintava nada: a linha nunca é
/// um bloco, e era o `layout_block` que fazia esta parte para todos os outros.
fn pinta_caixa(
    dom: &Dom,
    id: NodeIdx,
    rect: Rect,
    at: usize,
    filhos_antes: usize,
    list: &mut DisplayList,
) {
    let Some(css) = dom.computed_style_idx(id) else {
        return;
    };
    if !css.has_box() {
        return;
    }
    let radius = css.corner_radius.unwrap_or(0.0);
    let mut em = Vec::new();
    if let Some(bg) = css.bg {
        em.push(DisplayItem::SolidRect {
            rect,
            color: bg,
            radius: crate::layout::Corners::from_style(&css, 0.0),
        });
    }
    em.extend(crate::layout::border_items(
        &css,
        rect,
        radius,
        1.0,
        // A borda de uma célula respeita o `filter` dela como a de qualquer
        // outra caixa; passar a identidade aqui faria a mesma folha pintar
        // diferente consoante o elemento fosse ou não uma célula de tabela.
        crate::painteffects::filtro(css.filter.as_deref().unwrap_or("")),
    ));
    for (i, item) in em.into_iter().enumerate() {
        crate::layout::insert_item(list, at + i, filhos_antes, item);
    }
}
