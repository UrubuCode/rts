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

/// Onde UM item do grid vive: a célula inicial e o span, em índices de trilha
/// 0-based com o fim EXCLUSIVO. É o único que o resto do layout de grid
/// consome, o que é o que permite às três colocações (nome, linha, auto)
/// coexistirem sem um segundo caminho de posicionamento.
pub(in crate::layout) struct GridCell {
    pub child: NodeIdx,
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

/// A colocação explícita de UM eixo a partir das duas extremidades já
/// parseadas — `None` quando o eixo não tem informação suficiente para
/// resolver sozinho (as duas pontas `auto`, ou um `span` sem âncora), caso em
/// que o item cai na colocação automática desse eixo (spec §8.5 passo 3: aqui
/// simplificado para "ambos os eixos automáticos", já que este motor auto-
/// coloca em duas dimensões de uma vez).
fn axis_placement(start: GridLine, end: GridLine, explicit: usize) -> Option<(usize, usize)> {
    use GridLine::*;
    let (a, b): (i32, i32) = match (start, end) {
        (Line(a), Line(b)) => {
            let a = resolve_abs(a, explicit)?;
            let b = resolve_abs(b, explicit)?;
            if b > a {
                (a, b)
            } else {
                // fim antes (ou igual a) do início: a spec troca as pontas;
                // aqui vira span 1 a partir do início, que é a leitura segura.
                (a, a + 1)
            }
        }
        (Line(a), Span(n)) => {
            let a = resolve_abs(a, explicit)?;
            (a, a + n as i32)
        }
        (Span(n), Line(b)) => {
            let b = resolve_abs(b, explicit)?;
            let a = (b - n as i32).max(1);
            (a, b.max(a + 1))
        }
        (Line(a), Auto) => {
            let a = resolve_abs(a, explicit)?;
            (a, a + 1)
        }
        (Auto, Line(b)) => {
            let b = resolve_abs(b, explicit)?;
            let a = (b - 1).max(1);
            (a, a + 1)
        }
        // `span`+`span` não está na gramática que `grid_lines::GridLine::parse`
        // aceita (não há como um valor produzir dois `Span`), e as duas `auto`
        // são "sem placement nenhum" — os dois casos voltam para automático.
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

/// Coloca os filhos e devolve `(células, nº de colunas final)`. Três fases,
/// nesta ordem (spec §8.5): nomeados (`grid-area`) primeiro — senão um
/// automático ocuparia a célula antes de o nomeado a reclamar —, depois os
/// com colocação NUMÉRICA explícita nos dois eixos, depois os automáticos
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
    children: &[NodeIdx],
    areas: Option<&GridAreas>,
    explicit_cols: usize,
    explicit_rows: usize,
    auto_flow: GridAutoFlow,
) -> (Vec<GridCell>, usize) {
    let mut cells: Vec<GridCell> = Vec::with_capacity(children.len());
    let mut taken: HashSet<(usize, usize)> = HashSet::new();
    let mut ncols = explicit_cols.max(1);

    let mut numeric: Vec<NodeIdx> = Vec::new();
    let mut auto: Vec<NodeIdx> = Vec::new();
    for &child in children {
        let css = dom.computed_style_idx(child);
        let name = css.as_ref().and_then(|s| s.grid_area.clone());
        if let Some(a) = name.and_then(|n| areas.and_then(|ar| ar.area(&n))) {
            ncols = ncols.max(a.c1);
            mark(&mut taken, a.r0, a.c0, a.r1, a.c1);
            cells.push(GridCell { child, r0: a.r0, c0: a.c0, r1: a.r1, c1: a.c1 });
            continue;
        }
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
        let css = dom.computed_style_idx(child).unwrap_or_default();
        let colp = axis_placement(
            css.grid_column_start.unwrap_or(GridLine::Auto),
            css.grid_column_end.unwrap_or(GridLine::Auto),
            explicit_cols,
        );
        let rowp = axis_placement(
            css.grid_row_start.unwrap_or(GridLine::Auto),
            css.grid_row_end.unwrap_or(GridLine::Auto),
            explicit_rows,
        );
        match (colp, rowp) {
            (Some((c0, c1)), Some((r0, r1))) => {
                ncols = ncols.max(c1);
                mark(&mut taken, r0, c0, r1, c1);
                cells.push(GridCell { child, r0, c0, r1, c1 });
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
        let (r, c) = if auto_flow.coluna {
            let start = if auto_flow.dense { 0 } else { col_cursor };
            free_col_major(&taken, row_bound, start)
        } else {
            let start = if auto_flow.dense { 0 } else { cursor };
            free_row_major(&taken, ncols, start)
        };
        mark(&mut taken, r, c, r + 1, c + 1);
        cells.push(GridCell { child, r0: r, c0: c, r1: r + 1, c1: c + 1 });
        cursor = r * ncols.max(1) + c + 1;
        col_cursor = c;
        // flow `column`: as colunas implícitas contam para o `ncols` final,
        // que `grid.rs` usa para estender `grid-auto-columns` e dimensionar.
        if auto_flow.coluna {
            ncols = ncols.max(c + 1);
        }
    }

    (cells, ncols)
}
