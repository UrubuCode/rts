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

mod widths;

#[cfg(test)]
mod tests;

use crate::layout::{DisplayItem, DisplayList, LayoutCtx, Rect};
use crate::style::{ComputedStyle, DisplayKind};
use crate::{Dom, NodeIdx};

/// Uma célula colocada na grade. `col` é a coluna onde começa, já resolvida
/// contra os `rowspan` que vêm de cima.
struct Cell {
    node: NodeIdx,
    col: usize,
    colspan: usize,
    rowspan: usize,
}

/// Uma linha (`<tr>`), com as suas células e o grupo a que pertence.
struct Row {
    node: NodeIdx,
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

/// Os parâmetros de tabela que o CSS ainda não nos dá.
///
/// **Este struct é o ponto único a ligar quando `border-collapse`,
/// `border-spacing` e `table-layout` existirem no `ComputedStyle`.** Hoje vêm
/// dos atributos HTML equivalentes, que a Wikipédia e qualquer página anterior a
/// 2010 ainda escrevem, e dos defaults do Chrome. Espalhar estes três valores
/// pelo algoritmo seria a alternativa, e transformaria uma troca de quatro
/// linhas numa caça.
struct TableStyle {
    /// Vão horizontal e vertical entre células. Zero quando as bordas fundem.
    spacing_h: f32,
    spacing_v: f32,
    /// `table-layout: fixed`. **Hoje é sempre `false`**, e está dito em vez de
    /// escondido: a propriedade não existe no `ComputedStyle`, portanto o ramo
    /// fixo de [`widths::resolve_fixo`] não é alcançável por nenhuma página. O
    /// algoritmo está escrito e testado à parte porque a alternativa — deixá-lo
    /// para quando a propriedade chegar — significaria descobrir só então que a
    /// forma da função não servia.
    fixed: bool,
}

impl TableStyle {
    fn of(dom: &Dom, id: NodeIdx) -> TableStyle {
        // `cellspacing` é o atributo que corresponde a `border-spacing`; sem ele,
        // 2px, que é o valor da folha de estilo de todo o browser.
        let spacing = dom
            .node(id)
            .attr("cellspacing")
            .and_then(|v| v.trim().trim_end_matches("px").parse::<f32>().ok())
            .unwrap_or(2.0)
            .max(0.0);
        TableStyle { spacing_h: spacing, spacing_v: spacing, fixed: false }
    }
}

/// O `display` de um nó já com o default da UA aplicado, ou `None` para nós que
/// não são elementos.
fn display_of(dom: &Dom, id: NodeIdx) -> Option<DisplayKind> {
    crate::layout::used_display(dom, id)
}

/// Constrói a grade a partir da árvore. Um `<tbody>` implícito não existe no
/// nosso DOM (o parser não o cria), por isso as linhas podem estar tanto dentro
/// de um grupo como soltas — os dois casos caem no mesmo laço.
fn collect(dom: &Dom, table: NodeIdx) -> Grid {
    let mut g = Grid { rows: Vec::new(), groups: Vec::new(), outros: Vec::new(), cols: 0 };
    // `ocupado[coluna]` = quantas linhas ainda faltam para o `rowspan` que passa
    // por ali terminar. É o que faz uma célula da linha de baixo saltar a coluna
    // que a de cima ainda ocupa — sem isto, `rowspan` desalinha tudo o que vem
    // depois, que é o modo mais visível de uma tabela estar errada.
    let mut ocupado: Vec<usize> = Vec::new();

    for &child in &dom.node(table).children {
        match display_of(dom, child) {
            Some(DisplayKind::TableRow) => {
                add_row(dom, child, &mut g, &mut ocupado);
            }
            Some(DisplayKind::TableRowGroup) => {
                let inicio = g.rows.len();
                for &r in &dom.node(child).children {
                    if display_of(dom, r) == Some(DisplayKind::TableRow) {
                        add_row(dom, r, &mut g, &mut ocupado);
                    }
                }
                g.groups.push((child, inicio, g.rows.len() - inicio));
            }
            Some(DisplayKind::None) => {}
            // Texto solto entre `<tr>` (o whitespace do markup) não é nada.
            None => {}
            _ => {
                if !crate::layout::is_out_of_flow(dom, child) {
                    g.outros.push(child);
                }
            }
        }
    }
    g
}

fn add_row(dom: &Dom, row: NodeIdx, g: &mut Grid, ocupado: &mut Vec<usize>) {
    // Consome uma linha de cada `rowspan` pendente antes de colocar as células.
    for o in ocupado.iter_mut() {
        *o = o.saturating_sub(1);
    }
    let mut cells = Vec::new();
    let mut col = 0usize;
    for &c in &dom.node(row).children {
        if display_of(dom, c) != Some(DisplayKind::TableCell) {
            continue;
        }
        while ocupado.get(col).copied().unwrap_or(0) > 0 {
            col += 1;
        }
        let colspan = atributo_span(dom, c, "colspan");
        let rowspan = atributo_span(dom, c, "rowspan");
        if ocupado.len() < col + colspan {
            ocupado.resize(col + colspan, 0);
        }
        for o in &mut ocupado[col..col + colspan] {
            *o = rowspan;
        }
        cells.push(Cell { node: c, col, colspan, rowspan });
        col += colspan;
    }
    g.cols = g.cols.max(col);
    g.rows.push(Row { node: row, cells });
}

/// `colspan`/`rowspan`: pelo menos 1, e com teto. O teto não é decoração — um
/// `rowspan="100000000"` (que existe em páginas reais e em ataques de DoS de
/// layout) alocaria a grade inteira em memória.
fn atributo_span(dom: &Dom, id: NodeIdx, nome: &str) -> usize {
    dom.node(id)
        .attr(nome)
        .and_then(|v| v.trim().parse::<usize>().ok())
        .unwrap_or(1)
        .clamp(1, 1000)
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
    let ts = TableStyle::of(dom, table);
    let cols = medir_colunas(dom, &g, font, ctx, ts.spacing_h);
    cols.iter().map(|c| c.max).sum::<f32>() + (g.cols + 1) as f32 * ts.spacing_h
}

fn medir_colunas(
    dom: &Dom,
    g: &Grid,
    font: f32,
    ctx: &LayoutCtx,
    spacing: f32,
) -> Vec<widths::MinMax> {
    let mut medidas = Vec::new();
    for row in &g.rows {
        for c in &row.cells {
            medidas.push((c.col, c.colspan, widths::cell_min_max(dom, c.node, font, ctx)));
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
    _css: &ComputedStyle,
    font_size: f32,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) -> f32 {
    let g = collect(dom, id);
    let ts = TableStyle::of(dom, id);
    let mut y = content_y;

    // `<caption>` e outros blocos avulsos: empilham acima da grade, à largura da
    // tabela. Ficam fora do algoritmo de colunas de propósito — não têm coluna.
    for &o in &g.outros {
        let (_, h) = crate::layout::layout_block(
            dom, o, content_x, y, content_w, None, None, None, false, ctx, list,
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
        if let Some(primeira) = g.rows.first() {
            for c in &primeira.cells {
                let mm = widths::cell_min_max(dom, c.node, font_size, ctx);
                if c.colspan == 1 && c.col < g.cols {
                    declaradas[c.col] = Some(mm.max);
                }
            }
        }
        widths::resolve_fixo(&declaradas, disponivel)
    } else {
        widths::resolve_colunas(&medir_colunas(dom, &g, font_size, ctx, ts.spacing_h), disponivel)
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
            let (_, h) = crate::layout::measure_block(
                dom, c.node, w, None, Some(w), None, false, ctx,
            );
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
        crate::layout::reserve_node_order(list, row.node);
        for c in &row.cells {
            if c.col >= g.cols {
                continue;
            }
            let w = largura_de(c.col, c.colspan);
            let fim = (ri + c.rowspan - 1).min(g.rows.len() - 1);
            let h: f32 = alturas[ri..=fim].iter().sum::<f32>()
                + (fim - ri) as f32 * ts.spacing_v;
            crate::layout::layout_block(
                dom, c.node, col_x[c.col], y, w, Some(h), Some(w), Some(h), false, ctx, list,
            );
        }
        let rect = Rect::new(content_x, y, content_w, alturas[ri]);
        crate::layout::record_node_rect(list, row.node, rect);
        pinta_caixa(dom, row.node, rect, idx_fundo, list);
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
        pinta_caixa(dom, node, rect, list.items.len(), list);
    }
    y - content_y
}

/// Pinta fundo e borda de uma caixa que não passou pelo `layout_block` (linha ou
/// grupo de linhas), inserindo os itens em `at` para ficarem ATRÁS do que já lá
/// está. Sem isto, um `<tr>` com `background` não pintava nada: a linha nunca é
/// um bloco, e era o `layout_block` que fazia esta parte para todos os outros.
fn pinta_caixa(dom: &Dom, id: NodeIdx, rect: Rect, at: usize, list: &mut DisplayList) {
    let Some(css) = dom.computed_style_idx(id) else { return };
    if !css.has_box() {
        return;
    }
    let radius = css.corner_radius.unwrap_or(0.0);
    let mut em = Vec::new();
    if let Some(bg) = css.bg {
        em.push(DisplayItem::SolidRect { rect, color: bg, radius });
    }
    em.extend(crate::layout::border_items(&css, rect, radius, 1.0));
    for (i, item) in em.into_iter().enumerate() {
        crate::layout::insert_item(list, at + i, item);
    }
}
