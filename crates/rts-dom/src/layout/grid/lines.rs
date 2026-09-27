//! COLOCAÇÃO por linha do grid (`grid-column/row-start/end`, spans e negativos)
//! e o AUTO-FLOW (`row`/`column`, `dense`) — o consumidor de geometria que
//! faltava a `style::grid_lines` (o cabeçalho de lá diz "guardadas, SEM
//! geometria": este módulo é o ponto de enxerto que aponta).
//!
//! `GridCell` e a colocação viviam em `grid.rs`, que já estava no teto de 500
//! linhas (lote D deixou-o em 502) — não cabia crescer lá, e a colocação é uma
//! fatia coesa por si: consome `ComputedStyle` + `GridAreas`, devolve só a
//! lista de células. `grid.rs` mede/posiciona; este módulo decide ONDE cada
//! item vive.

use std::collections::HashSet;

use crate::style::grid_lines::{GridAutoFlow, GridLine};
use crate::style::GridAreas;
use crate::{Dom, NodeIdx};
use crate::boxes::BoxId;

/// Item de grid antes da colocação. A geometria usa a caixa, enquanto a
/// colocação e o estilo continuam a consultar o nó que a gerou.
#[derive(Clone, Copy)]
pub(in crate::layout) struct GridItem {
    pub node: NodeIdx,
    pub box_id: BoxId,
}

/// Onde UM item do grid vive: a célula inicial e o span, em índices de trilha
/// 0-based com o fim EXCLUSIVO. É o único que o resto do layout de grid
/// consome, o que é o que permite às três colocações (nome, linha, auto)
/// coexistirem sem um segundo caminho de posicionamento.
pub(in crate::layout) struct GridCell {
    pub child: NodeIdx,
    pub box_id: BoxId,
    pub r0: usize,
    pub c0: usize,
    pub r1: usize,
    pub c1: usize,
}

impl GridCell {
    pub fn rows(&self) -> usize {
        (self.r1 - self.r0).max(1)
    }
}

/// Resolve UMA extremidade numérica (não `span`) para 1-based, com negativos a
/// contar do fim do eixo EXPLÍCITO (spec §8.3: a linha `-1` é a última linha
/// da grelha explícita — `explicit` trilhas têm `explicit+1` linhas).
fn resolve_abs(n: i32, explicit: usize) -> Option<i32> {
    if n > 0 {
        Some(n)
    } else if n < 0 {
        let total_lines = explicit as i32 + 1;
        let r = total_lines + n + 1;
        (r >= 1).then_some(r)
    } else {
        None // `0` não é da gramática (grid_lines::GridLine::parse já o recusa)
    }
}

/// The 1-based line a NAME denotes on one axis, from the only named lines this
/// engine knows: the implicit `<area>-start`/`<area>-end` of
/// `grid-template-areas` (§7.3.2). A bare `foo` means `foo-start` on a start
/// edge and `foo-end` on an end edge (§8.3.1). `grid-template-columns: [a]`
/// line names are not parsed by the track parser, so such a name resolves to
/// nothing here, and neither does an `nth` beyond 1 (each implicit name exists
/// once). The spec would then place on an IMPLICIT line past the grid; this
/// answers `None`, which puts that edge back to `auto`.
fn named_line(name: &str, nth: i32, end_edge: bool, cols: bool, areas: Option<&GridAreas>) -> Option<i32> {
    if nth != 1 && nth != -1 {
        return None;
    }
    let areas = areas?;
    let (area, end) = if let Some(a) = name.strip_suffix("-start").and_then(|n| areas.area(n)) {
        (a, false)
    } else if let Some(a) = name.strip_suffix("-end").and_then(|n| areas.area(n)) {
        (a, true)
    } else {
        (areas.area(name)?, end_edge)
    };
    let (s, e) = if cols { (area.c0, area.c1) } else { (area.r0, area.r1) };
    Some(if end { e as i32 + 1 } else { s as i32 + 1 })
}

/// One edge after name resolution: an absolute line, a span count, a span TO
/// a named line, or nothing (`auto`, or a name that resolved to no line).
enum Edge {
    Abs(i32),
    Span(u32),
    SpanTo(i32),
    Auto,
}

fn resolve_edge(l: &GridLine, end_edge: bool, cols: bool, explicit: usize, areas: Option<&GridAreas>) -> Edge {
    match l {
        GridLine::Auto => Edge::Auto,
        GridLine::Line(n) => resolve_abs(*n, explicit).map_or(Edge::Auto, Edge::Abs),
        GridLine::Span(n) => Edge::Span(*n),
        GridLine::Named(id, nth) => {
            named_line(id, nth.unwrap_or(1), end_edge, cols, areas).map_or(Edge::Auto, Edge::Abs)
        }
        // `span <name>` counts to the first line of that name beyond the other
        // edge; with no such line the spec would use implicit lines, and one
        // track is the reading that stays inside what this engine resolves.
        GridLine::SpanNamed(id, n) => {
            named_line(id, *n as i32, !end_edge, cols, areas).map_or(Edge::Span(1), Edge::SpanTo)
        }
    }
}

/// A colocação explícita de UM eixo a partir das duas extremidades já
/// parseadas — `None` quando o eixo não tem informação suficiente para
/// resolver sozinho (as duas pontas `auto`, ou um `span` sem âncora), caso em
/// que o item cai na colocação automática desse eixo (spec §8.5 passo 3: aqui
/// simplificado para "ambos os eixos automáticos", já que este motor auto-
/// coloca em duas dimensões de uma vez).
fn axis_placement(
    start: &GridLine,
    end: &GridLine,
    explicit: usize,
    areas: Option<&GridAreas>,
    cols: bool,
) -> Option<(usize, usize)> {
    use Edge::*;
    let s = resolve_edge(start, false, cols, explicit, areas);
    let e = resolve_edge(end, true, cols, explicit, areas);
    let (a, b): (i32, i32) = match (s, e) {
        (Abs(a), Abs(b)) => {
            if b > a {
                (a, b)
            } else if b < a {
                // §8.3.1: start after end swaps the two lines.
                (b, a)
            } else {
                (a, a + 1)
            }
        }
        (Abs(a), SpanTo(l)) => (a, if l > a { l } else { a + 1 }),
        (SpanTo(l), Abs(b)) => (if l < b { l } else { (b - 1).max(1) }, b),
        (Abs(a), Span(n)) => (a, a + n as i32),
        (Span(n), Abs(b)) => {
            let a = (b - n as i32).max(1);
            (a, b.max(a + 1))
        }
        (Abs(a), Auto) => (a, a + 1),
        (Auto, Abs(b)) => {
            let a = (b - 1).max(1);
            (a, a + 1)
        }
        // Two spans, or a span with no anchor, or both `auto`: no definite
        // position on this axis, and the item goes back to auto-placement.
        _ => return None,
    };
    Some(((a - 1).max(0) as usize, (b - 1).max(0) as usize))
}

/// Marca `r0..r1 × c0..c1` como ocupado.
fn mark(taken: &mut HashSet<(usize, usize)>, r0: usize, c0: usize, r1: usize, c1: usize) {
    for r in r0..r1 {
        for c in c0..c1 {
            taken.insert((r, c));
        }
    }
}

/// Primeira célula livre a partir de `start_idx` (linear, row-major, `ncols`
/// colunas por linha) — o flow `row` (default).
fn free_row_major(taken: &HashSet<(usize, usize)>, ncols: usize, start_idx: usize) -> (usize, usize) {
    let mut idx = start_idx;
    loop {
        let (r, c) = (idx / ncols, idx % ncols);
        if !taken.contains(&(r, c)) {
            return (r, c);
        }
        idx += 1;
    }
}

/// Primeira célula livre em ordem COLUNA-MAJOR a partir de `start_col` — o
/// flow `column`: preenche uma coluna inteira antes de passar à próxima,
/// crescendo COLUNAS implícitas. `row_bound` é fixo — as linhas EXPLÍCITAS
/// (ou 1, quando não há `grid-template-rows`: é o que faz o 2º item de uma
/// grelha sem linhas declaradas abrir logo uma 2ª coluna, em vez de empilhar
/// na mesma).
fn free_col_major(taken: &HashSet<(usize, usize)>, row_bound: usize, start_col: usize) -> (usize, usize) {
    let row_bound = row_bound.max(1);
    let mut c = start_col;
    loop {
        for r in 0..row_bound {
            if !taken.contains(&(r, c)) {
                return (r, c);
            }
        }
        c += 1;
        // guarda contra uma grelha patologicamente cheia — nunca alcançado
        // por uma página real.
        if c > 100_000 {
            return (0, c);
        }
    }
}

/// Coloca os filhos e devolve `(células, nº de colunas final)`. Two phases
/// (spec §8.5): items definite on both axes first — line numbers and area
/// names alike, since `grid-area: <name>` arrives as four named longhands —
/// so an auto item cannot take a cell before its owner claims it; then the automatic ones
/// (row-major ou column-major conforme `auto_flow`, `dense` reinicia a busca
/// do início em vez de continuar do cursor).
///
/// `ncols` PODE crescer aqui: um `grid-area`/`grid-column` que aponta para lá
/// da última coluna explícita cria colunas IMPLÍCITAS — é o que
/// `grid-auto-columns` dimensiona depois, em `grid.rs`. Só cresce nas duas
/// primeiras fases: a automática já corre com o `ncols` final, porque o flow
/// `column` precisa de um número de colunas fixo para saber quando "acabou"
/// uma coluna e passa à próxima.
pub(in crate::layout) fn place_grid_items(
    dom: &Dom,
    children: &[GridItem],
    areas: Option<&GridAreas>,
    explicit_cols: usize,
    explicit_rows: usize,
    auto_flow: GridAutoFlow,
) -> (Vec<GridCell>, usize) {
    let mut cells: Vec<GridCell> = Vec::with_capacity(children.len());
    let mut taken: HashSet<(usize, usize)> = HashSet::new();
    let mut ncols = explicit_cols.max(1);

    let mut numeric: Vec<GridItem> = Vec::new();
    let mut auto: Vec<GridItem> = Vec::new();
    for &child in children {
        let css = dom.computed_style_idx(child.node);
        let has_numeric = css
            .as_ref()
            .map(|s| {
                s.grid_column_start.is_some()
                    || s.grid_column_end.is_some()
                    || s.grid_row_start.is_some()
                    || s.grid_row_end.is_some()
            })
            .unwrap_or(false);
        if has_numeric {
            numeric.push(child);
        } else {
            auto.push(child);
        }
    }

    for child in numeric {
        let css = dom.computed_style_idx(child.node).unwrap_or_default();
        let auto_line = GridLine::Auto;
        let colp = axis_placement(
            css.grid_column_start.as_ref().unwrap_or(&auto_line),
            css.grid_column_end.as_ref().unwrap_or(&auto_line),
            explicit_cols,
            areas,
            true,
        );
        let rowp = axis_placement(
            css.grid_row_start.as_ref().unwrap_or(&auto_line),
            css.grid_row_end.as_ref().unwrap_or(&auto_line),
            explicit_rows,
            areas,
            false,
        );
        match (colp, rowp) {
            (Some((c0, c1)), Some((r0, r1))) => {
                ncols = ncols.max(c1);
                mark(&mut taken, r0, c0, r1, c1);
                cells.push(GridCell { child: child.node, box_id: child.box_id, r0, c0, r1, c1 });
            }
            // Um eixo só (o outro `auto`/indeterminado): a spec varre o eixo
            // aberto a partir da linha dada; este motor simplifica para
            // auto-colocação nos dois eixos — cobre o caso mais comum, que é
            // o eixo aberto estar mesmo ausente da declaração.
            _ => auto.push(child),
        }
    }

    let row_bound = explicit_rows.max(1);
    let mut cursor = 0usize;
    let mut col_cursor = 0usize;
    for child in auto {
        let (r, c) = if auto_flow.column {
            let start = if auto_flow.dense { 0 } else { col_cursor };
            free_col_major(&taken, row_bound, start)
        } else {
            let start = if auto_flow.dense { 0 } else { cursor };
            free_row_major(&taken, ncols, start)
        };
        mark(&mut taken, r, c, r + 1, c + 1);
        cells.push(GridCell { child: child.node, box_id: child.box_id, r0: r, c0: c, r1: r + 1, c1: c + 1 });
        cursor = r * ncols.max(1) + c + 1;
        col_cursor = c;
        // flow `column`: as colunas implícitas contam para o `ncols` final,
        // que `grid.rs` usa para estender `grid-auto-columns` e dimensionar.
        if auto_flow.column {
            ncols = ncols.max(c + 1);
        }
    }

    (cells, ncols)
}

/// The in-flow ITEMS of a grid container, in box-tree order: what the layout
/// places, and what the intrinsic measure (`aspect::intrinsic_floor`) places
/// again. Grid creates no anonymous boxes for its children — a grid item is
/// blockified — but the sequence still belongs to the BoxTree, so each item
/// carries this construction's `BoxId` rather than coming back through its
/// `NodeIdx`. One list, two readers: a second filter in the measure would
/// count an item the layout drops.
pub(in crate::layout) fn collect_items(
    dom: &Dom,
    tree: &crate::boxes::BoxTree,
    container: BoxId,
) -> Vec<GridItem> {
    use super::*;
    let mut items = Vec::new();
    for &box_id in tree.children_without_generated(container) {
        let Some(child) = tree.node_of(box_id) else {
            continue;
        };
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
        items.push(GridItem { node: child, box_id });
    }
    items
}
