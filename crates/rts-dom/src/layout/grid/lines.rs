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



use crate::boxes::BoxId;
use crate::style::grid_lines::GridLine;
use crate::style::{GridAreas, LineNames};
use crate::{Dom, NodeIdx};

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

/// Combines explicit line names with implicit area names into an ordered list of lines.
pub(in crate::layout) fn effective_lines(
    explicit: &[LineNames],
    track_count: usize,
    areas: Option<&GridAreas>,
    cols: bool,
    is_subgrid: bool,
) -> Vec<LineNames> {
    let mut num_lines = (track_count + 1).max(explicit.len());
    if !is_subgrid {
        if let Some(areas) = areas {
            for (_, area) in areas.iter() {
                let end_line = if cols { area.c1 + 1 } else { area.r1 + 1 };
                num_lines = num_lines.max(end_line);
            }
        }
    }
    let mut out: Vec<LineNames> = vec![Vec::new(); num_lines];
    for (i, names) in explicit.iter().enumerate() {
        if i < out.len() {
            out[i].extend(names.iter().cloned());
        }
    }
    if let Some(areas) = areas {
        let max_idx = out.len().saturating_sub(1);
        for (name, area) in areas.iter() {
            let (mut start_idx, mut end_idx) = if cols {
                (area.c0, area.c1)
            } else {
                (area.r0, area.r1)
            };
            if is_subgrid {
                start_idx = start_idx.min(max_idx);
                end_idx = end_idx.min(max_idx);
            }
            let start_name = format!("{}-start", name);
            let end_name = format!("{}-end", name);
            if start_idx < out.len() && !out[start_idx].contains(&start_name) {
                out[start_idx].push(start_name);
            }
            if end_idx < out.len() && !out[end_idx].contains(&end_name) {
                out[end_idx].push(end_name);
            }
        }
    }
    out
}

/// The 1-based line a NAME denotes on one axis (CSS Grid 1 §8.3.1).
/// Matches against implicit area -start/-end suffixes first (§8.3.1), then falls back to explicit line names.
fn named_line(name: &str, nth: i32, end_edge: bool, lines: &[LineNames]) -> Option<i32> {
    if nth == 0 {
        return None;
    }
    let mut matches: Vec<usize> = Vec::new();
    if !name.ends_with("-start") && !name.ends_with("-end") {
        let suffixed = if end_edge {
            format!("{}-end", name)
        } else {
            format!("{}-start", name)
        };
        for (idx, line) in lines.iter().enumerate() {
            if line.iter().any(|l| l == &suffixed) {
                matches.push(idx);
            }
        }
    }
    if matches.is_empty() {
        for (idx, line) in lines.iter().enumerate() {
            if line.iter().any(|l| l == name) {
                matches.push(idx);
            }
        }
    }
    if matches.is_empty() {
        return None;
    }
    if nth > 0 {
        let k = (nth - 1) as usize;
        matches.get(k).map(|&idx| idx as i32 + 1)
    } else {
        let k = (-nth - 1) as usize;
        if k < matches.len() {
            Some(matches[matches.len() - 1 - k] as i32 + 1)
        } else {
            None
        }
    }
}

pub(in crate::layout) fn find_named_line_forward(
    name: &str,
    after_line: i32,
    nth: u32,
    lines: &[LineNames],
) -> Option<i32> {
    if nth == 0 {
        return None;
    }
    let start_idx = after_line.max(0) as usize;
    let suffixed = format!("{}-end", name);
    let mut count = 0;
    for (idx, line) in lines.iter().enumerate().skip(start_idx) {
        if line.iter().any(|l| l == &suffixed || l == name) {
            count += 1;
            if count == nth {
                return Some(idx as i32 + 1);
            }
        }
    }
    None
}

pub(in crate::layout) fn find_named_line_backward(
    name: &str,
    before_line: i32,
    nth: u32,
    lines: &[LineNames],
) -> Option<i32> {
    if nth == 0 || before_line <= 1 {
        return None;
    }
    let end_idx = (before_line - 1).min(lines.len() as i32) as usize;
    let suffixed = format!("{}-start", name);
    let mut count = 0;
    for idx in (0..end_idx).rev() {
        if lines[idx].iter().any(|l| l == &suffixed || l == name) {
            count += 1;
            if count == nth {
                return Some(idx as i32 + 1);
            }
        }
    }
    None
}

/// One edge after name resolution: an absolute line, a span count, a named span,
/// or nothing (`auto`, or a name that resolved to no line).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::layout) enum Edge {
    Abs(i32),
    Span(u32),
    SpanNamed(std::sync::Arc<str>, u32),
    Auto,
}

pub(in crate::layout) fn resolve_edge(
    l: &GridLine,
    end_edge: bool,
    explicit: usize,
    lines: &[LineNames],
    is_subgrid: bool,
) -> Edge {
    match l {
        GridLine::Auto => Edge::Auto,
        GridLine::Line(n) => resolve_abs(*n, explicit).map_or(Edge::Auto, Edge::Abs),
        GridLine::Span(n) => Edge::Span(*n),
        GridLine::Named(id, nth) => {
            let n = nth.unwrap_or(1);
            Edge::Abs(named_line(id, n, end_edge, lines).unwrap_or_else(|| {
                if is_subgrid {
                    if n > 0 { lines.len() as i32 } else { 1 }
                } else if n > 0 {
                    lines.len() as i32 + n
                } else {
                    2 + n
                }
            }))
        }
        GridLine::SpanNamed(id, n) => Edge::SpanNamed(id.clone(), *n),
    }
}

/// A colocação explícita de UM eixo a partir das duas extremidades já
/// parseadas — `None` quando o eixo não tem informação suficiente para
/// resolver sozinho (as duas pontas `auto`, ou um `span` sem âncora).
pub(in crate::layout) fn axis_placement(
    start: &GridLine,
    end: &GridLine,
    explicit: usize,
    lines: &[LineNames],
    is_subgrid: bool,
) -> Option<(usize, usize)> {
    use Edge::*;
    let s = resolve_edge(start, false, explicit, lines, is_subgrid);
    let e = resolve_edge(end, true, explicit, lines, is_subgrid);
    let (a, b): (i32, i32) = match (s, e) {
        (Abs(a), Abs(b)) => {
            if b > a {
                (a, b)
            } else if b < a {
                (b, a)
            } else {
                (a, a + 1)
            }
        }
        (Abs(a), SpanNamed(id, n)) => {
            let l = find_named_line_forward(&id, a, n, lines).unwrap_or(a + 1);
            (a, if l > a { l } else { a + 1 })
        }
        (SpanNamed(id, n), Abs(b)) => {
            let l = find_named_line_backward(&id, b, n, lines).unwrap_or((b - 1).max(1));
            (if l < b { l } else { (b - 1).max(1) }, b)
        }
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
    let mut c0 = (a - 1).max(0) as usize;
    let mut c1 = (b - 1).max(0) as usize;
    if is_subgrid && explicit > 0 {
        let span = c1.saturating_sub(c0).max(1);
        if c0 >= explicit {
            c1 = explicit;
            c0 = c1.saturating_sub(span);
        } else if c1 > explicit {
            c1 = explicit;
        }
    }
    Some((c0, c1))
}

pub(in crate::layout) use super::placement::place_grid_items;

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
        let is_element = matches!(dom.node(child).kind, NodeKind::Element { .. });
        if !is_element && collect_text(dom, child).trim().is_empty() {
            continue;
        }
        items.push(GridItem {
            node: child,
            box_id,
        });
    }
    items.sort_by_key(|it| {
        dom.computed_style_idx(it.node)
            .and_then(|c| c.order)
            .unwrap_or(0)
    });
    items
}
