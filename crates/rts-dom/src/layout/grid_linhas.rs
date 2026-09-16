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

/// O SPAN que um eixo pede quando a sua colocação não é resolúvel sozinha —
/// `grid-column: span 2`, que é `Span(2)` no início e `Auto` no fim.
///
/// Só é consultado quando [`axis_placement`] devolveu `None`, e nessa altura
/// as únicas formas com um `Span` lá dentro são as que têm a outra ponta
/// `auto` (as duas com uma `Line` resolvem-se sozinhas). Devolver 1 aqui era o
/// que fazia o item de `span 2` ocupar UMA célula: o span não se perdia no
/// parse — perdia-se entre o parse e a colocação.
fn axis_span(start: GridLine, end: GridLine) -> usize {
    match (start, end) {
        (GridLine::Span(n), _) | (_, GridLine::Span(n)) => (n as usize).max(1),
        _ => 1,
    }
}

/// Marca `r0..r1 × c0..c1` como ocupado.
fn mark(taken: &mut HashSet<(usize, usize)>, r0: usize, c0: usize, r1: usize, c1: usize) {
    for r in r0..r1 {
        for c in c0..c1 {
            taken.insert((r, c));
        }
    }
}

/// O retângulo `rows × cols` a partir de `(r, c)` está todo livre?
fn cabe(taken: &HashSet<(usize, usize)>, r: usize, c: usize, rows: usize, cols: usize) -> bool {
    for rr in r..r + rows {
        for cc in c..c + cols {
            if taken.contains(&(rr, cc)) {
                return false;
            }
        }
    }
    true
}

/// A primeira LINHA a partir de `inicio` onde um item de `rows × cols` cabe na
/// coluna `c0` — para quem tem a coluna presa e a linha por achar (spec §8.5
/// passo 4).
///
/// `inicio` é a linha do CURSOR de auto-colocação e não zero, que é o que
/// separa o empacotamento esparso (o default) do `dense`: com `dense` a busca
/// recomeça do topo e o item tapa um buraco deixado para trás; sem ele, não.
/// Começar sempre em zero dava o comportamento `dense` a quem não o pediu.
///
/// Termina sempre: as linhas não têm limite, portanto há sempre uma vazia.
fn primeira_linha_livre(
    taken: &HashSet<(usize, usize)>,
    c0: usize,
    cols: usize,
    rows: usize,
    inicio: usize,
) -> usize {
    let mut r = inicio;
    while !cabe(taken, r, c0, rows, cols) {
        r += 1;
    }
    r
}

/// A primeira COLUNA da linha `r0` onde um item de `rows × cols` cabe — para
/// quem tem a linha presa e a coluna por achar (spec §8.5 passo 3).
///
/// Sem lugar livre dentro de `ncols`, devolve 0 e o item SOBREPÕE-SE. A spec
/// mandaria crescer a grelha implícita, e é a mesma recusa da colocação
/// automática: o número de colunas já foi usado para expandir
/// `repeat(auto-fill, …)` e para dimensionar as trilhas, e alargá-lo a meio da
/// colocação invalidaria os índices de quem já está colocado. Sobrepor é legal
/// em grid (§8.5 permite-o a itens explicitamente colocados); mover as trilhas
/// não é recuperável.
fn primeira_coluna_livre(
    taken: &HashSet<(usize, usize)>,
    r0: usize,
    rows: usize,
    cols: usize,
    ncols: usize,
) -> usize {
    let ncols = ncols.max(1);
    for c in 0..=ncols.saturating_sub(cols) {
        if cabe(taken, r0, c, rows, cols) {
            return c;
        }
    }
    0
}

/// Primeira posição livre para um item de `col_span × row_span` a partir de
/// `start_idx` (linear, row-major, `ncols` colunas por linha) — o flow `row`
/// (default). O span não atravessa a última coluna: a spec (§8.5) manda o
/// cursor saltar para a linha seguinte em vez de partir o item.
///
/// Termina sempre: as LINHAS não têm limite, e `col_span` está limitado a
/// `ncols` por quem chama, portanto há sempre uma linha vazia onde cabe.
fn free_row_major(
    taken: &HashSet<(usize, usize)>,
    ncols: usize,
    start_idx: usize,
    col_span: usize,
    row_span: usize,
) -> (usize, usize) {
    let ncols = ncols.max(1);
    let col_span = col_span.clamp(1, ncols);
    let row_span = row_span.max(1);
    let mut idx = start_idx;
    loop {
        let (r, c) = (idx / ncols, idx % ncols);
        if c + col_span <= ncols && cabe(taken, r, c, row_span, col_span) {
            return (r, c);
        }
        idx += 1;
    }
}

/// Primeira posição livre em ordem COLUNA-MAJOR a partir de `start_col` — o
/// flow `column`: preenche uma coluna inteira antes de passar à próxima,
/// crescendo COLUNAS implícitas. `row_bound` é fixo — as linhas EXPLÍCITAS
/// (ou 1, quando não há `grid-template-rows`: é o que faz o 2º item de uma
/// grelha sem linhas declaradas abrir logo uma 2ª coluna, em vez de empilhar
/// na mesma) — e é ele que limita `row_span`, pelo mesmo motivo que `ncols`
/// limita `col_span` no flow `row`.
fn free_col_major(
    taken: &HashSet<(usize, usize)>,
    row_bound: usize,
    start_col: usize,
    col_span: usize,
    row_span: usize,
) -> (usize, usize) {
    let row_bound = row_bound.max(1);
    let row_span = row_span.clamp(1, row_bound);
    let col_span = col_span.max(1);
    let mut c = start_col;
    loop {
        for r in 0..=(row_bound - row_span) {
            if cabe(taken, r, c, row_span, col_span) {
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

/// Coloca os filhos e devolve `(células, nº de colunas final)`. DUAS fases
/// (spec §8.5): primeiro quem tem célula própria — nomeada por `grid-area` ou
/// numérica nos dois eixos —, senão um automático ocuparia a célula antes de o
/// explícito a reclamar; depois os automáticos, em ordem de DOCUMENTO
/// (row-major ou column-major conforme `auto_flow`, `dense` reinicia a busca
/// do início em vez de continuar do cursor).
///
/// `ncols` PODE crescer aqui: um `grid-area`/`grid-column` que aponta para lá
/// da última coluna explícita cria colunas IMPLÍCITAS — é o que
/// `grid-auto-columns` dimensiona depois, em `grid.rs`. Só cresce na primeira
/// fase: a automática já corre com o `ncols` final, porque o flow
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

    // Uma passagem SÓ, em ordem de documento, a decidir o destino de cada
    // filho; a colocação em si é que fica em duas fases. As três eram três
    // laços, e o terceiro recebia os itens que o segundo desistiu de colocar
    // DEPOIS de todos os automáticos — ou seja, um `grid-column: span 2` no
    // meio da lista era colocado como se fosse o último filho. A ordem importa
    // (é ela que decide que célula cada item apanha), e é a de documento.
    #[derive(Clone, Copy)]
    enum Plano {
        /// As quatro linhas já resolvidas (área nomeada ou colocação numérica).
        Fixa(usize, usize, usize, usize),
        /// COLUNA resolvida, linha por achar: `(c0, c1, row_span)`.
        ColunaFixa(usize, usize, usize),
        /// LINHA resolvida, coluna por achar: `(r0, r1, col_span)`.
        LinhaFixa(usize, usize, usize),
        /// Sem colocação resolúvel: automática, com (col_span, row_span).
        Auto(usize, usize),
    }
    let mut planos: Vec<(NodeIdx, Plano)> = Vec::with_capacity(children.len());
    for &child in children {
        let css = dom.computed_style_idx(child);
        let name = css.as_ref().and_then(|s| s.grid_area.clone());
        if let Some(a) = name.and_then(|n| areas.and_then(|ar| ar.area(&n))) {
            planos.push((child, Plano::Fixa(a.r0, a.c0, a.r1, a.c1)));
            continue;
        }
        let css = css.unwrap_or_default();
        let (cs, ce) = (
            css.grid_column_start.unwrap_or(GridLine::Auto),
            css.grid_column_end.unwrap_or(GridLine::Auto),
        );
        let (rs, re) = (
            css.grid_row_start.unwrap_or(GridLine::Auto),
            css.grid_row_end.unwrap_or(GridLine::Auto),
        );
        match (
            axis_placement(cs, ce, explicit_cols),
            axis_placement(rs, re, explicit_rows),
        ) {
            (Some((c0, c1)), Some((r0, r1))) => planos.push((child, Plano::Fixa(r0, c0, r1, c1))),
            // UM eixo só. A spec (§8.5, passos 3 e 4) varre o eixo ABERTO a
            // partir do início e prende o item na linha dada do outro; este
            // motor mandava os dois eixos para automático, o que é dizer que
            // `grid-row: 2` não fazia nada. Seis referências deste corpus
            // (`row-auto-repeat-001-ref` e as que apontam para ela) são
            // exactamente isso — e uma referência errada faz o teste falhar
            // mesmo com o lado do teste certo.
            (Some((c0, c1)), None) => {
                planos.push((child, Plano::ColunaFixa(c0, c1, axis_span(rs, re))))
            }
            (None, Some((r0, r1))) => {
                planos.push((child, Plano::LinhaFixa(r0, r1, axis_span(cs, ce))))
            }
            (None, None) => planos.push((child, Plano::Auto(axis_span(cs, ce), axis_span(rs, re)))),
        }
    }

    // Fase 1: os que têm célula própria, antes dos automáticos — senão um
    // automático ocupava a célula antes de o explícito a reclamar (spec §8.5).
    // Uma coluna DEFINIDA conta para o `ncols` final mesmo quando a linha ainda
    // está por achar: é ela que cria as colunas implícitas, e a fase 2 precisa
    // do número já fechado.
    for (child, plano) in &planos {
        match *plano {
            Plano::Fixa(r0, c0, r1, c1) => {
                ncols = ncols.max(c1);
                mark(&mut taken, r0, c0, r1, c1);
                cells.push(GridCell { child: *child, r0, c0, r1, c1 });
            }
            Plano::ColunaFixa(_, c1, _) => ncols = ncols.max(c1),
            Plano::LinhaFixa(..) | Plano::Auto(..) => {}
        }
    }

    // Fase 2: os automáticos, em ordem de documento.
    let row_bound = explicit_rows.max(1);
    let mut cursor = 0usize;
    let mut col_cursor = 0usize;
    for (child, plano) in &planos {
        let (col_span, row_span) = match *plano {
            Plano::Fixa(..) => continue, // já colocado na fase 1
            // Coluna presa: desce pela primeira linha onde o item cabe NAQUELAS
            // colunas. Move o cursor (spec §8.5 passo 4), ao contrário do caso
            // de baixo.
            Plano::ColunaFixa(c0, c1, rs) => {
                let cs = (c1 - c0).max(1);
                let rs = rs.max(1);
                // Onde o cursor está: a sua linha, mais uma se ele já passou a
                // coluna de início do item (spec §8.5 passo 4 — "if the
                // cursor's column position is past the item's column-start
                // line, increment the row position"). `dense` recomeça do topo.
                let n = ncols.max(1);
                let inicio = if auto_flow.dense {
                    0
                } else if cursor % n > c0 {
                    cursor / n + 1
                } else {
                    cursor / n
                };
                let r = primeira_linha_livre(&taken, c0, cs, rs, inicio);
                mark(&mut taken, r, c0, r + rs, c0 + cs);
                cells.push(GridCell { child: *child, r0: r, c0, r1: r + rs, c1: c0 + cs });
                cursor = r * ncols.max(1) + c0 + cs;
                continue;
            }
            // Linha presa: a primeira coluna livre DAQUELA linha. NÃO move o
            // cursor — a spec só o faz quando a colocação é no eixo do fluxo.
            Plano::LinhaFixa(r0, r1, cs) => {
                let rs = (r1 - r0).max(1);
                let cs = cs.clamp(1, ncols.max(1));
                let c = primeira_coluna_livre(&taken, r0, rs, cs, ncols);
                mark(&mut taken, r0, c, r0 + rs, c + cs);
                cells.push(GridCell { child: *child, r0, c0: c, r1: r0 + rs, c1: c + cs });
                continue;
            }
            Plano::Auto(col_span, row_span) => (col_span, row_span),
        };
        // O span é LIMITADO ao eixo fechado (colunas no flow `row`, linhas no
        // flow `column`) em vez de o fazer crescer: a spec faz crescer a grelha
        // implícita, mas aqui o eixo fechado já foi contado — é ele que
        // `grid.rs` usou para expandir `repeat(auto-fill, …)` e para dimensionar
        // as trilhas — e alargá-lo a meio da colocação invalidaria os índices
        // dos itens já colocados.
        let (cs, rs) = if auto_flow.coluna {
            (col_span, row_span.min(row_bound))
        } else {
            (col_span.min(ncols), row_span)
        };
        let (r, c) = if auto_flow.coluna {
            let start = if auto_flow.dense { 0 } else { col_cursor };
            free_col_major(&taken, row_bound, start, cs, rs)
        } else {
            let start = if auto_flow.dense { 0 } else { cursor };
            free_row_major(&taken, ncols, start, cs, rs)
        };
        mark(&mut taken, r, c, r + rs, c + cs);
        cells.push(GridCell { child: *child, r0: r, c0: c, r1: r + rs, c1: c + cs });
        cursor = r * ncols.max(1) + c + cs;
        col_cursor = c;
        // flow `column`: as colunas implícitas contam para o `ncols` final,
        // que `grid.rs` usa para estender `grid-auto-columns` e dimensionar.
        if auto_flow.coluna {
            ncols = ncols.max(c + cs);
        }
    }

    (cells, ncols)
}
