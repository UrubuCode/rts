//! Colocação por LINHA de grid (`style::grid_lines`)
//!
//! Extraído de `newprops_tests.rs` sem alterar um teste; o arranjo de layout
//! partilhado vive no `super`.

use super::*;

// ── LOTE B: colocação por LINHA de grid (ver `style::grid_lines`) ────────────

#[test]
fn grid_line_aceita_as_quatro_formas_que_o_corpus_escreve() {
    // As 13 folhas, juntas, escrevem exatamente estas quatro. Nenhuma escreve
    // uma linha com NOME, que é a razão para `GridLine` não ter variante para
    // isso — ver o cabeçalho do módulo.
    use crate::style::grid_lines::GridLine;
    assert_eq!(
        parse_inline("grid-column-start: auto").grid_column_start,
        Some(GridLine::Auto)
    );
    assert_eq!(
        parse_inline("grid-column-start: 7").grid_column_start,
        Some(GridLine::Line(7))
    );
    assert_eq!(
        parse_inline("grid-column-end: -1").grid_column_end,
        Some(GridLine::Line(-1))
    );
    assert_eq!(
        parse_inline("grid-row-end: span 2").grid_row_end,
        Some(GridLine::Span(2))
    );
}

#[test]
fn linha_zero_nao_e_uma_linha_de_grid() {
    // A spec numera as linhas a partir de 1 e usa os negativos para contar do
    // fim; `0` não é nenhuma delas. Guardá-lo como `Line(0)` daria a quem vier a
    // colocar os itens um índice que não existe — pior que não declarado.
    use crate::style::grid_lines::GridLine;
    assert_eq!(parse_inline("grid-column-start: 0").grid_column_start, None);
    // e um `span 0` também não: um item ocupa pelo menos uma pista.
    assert_eq!(
        parse_inline("grid-column-end: span 0").grid_column_end,
        None
    );
    // `span3` sem separador não é um span — é lixo, e lixo não vira `Span(3)`.
    assert_eq!(parse_inline("grid-column-end: span3").grid_column_end, None);
    assert_eq!(
        parse_inline("grid-row: span 1 / span 1").grid_row_start,
        Some(GridLine::Span(1))
    );
}

#[test]
fn shorthand_de_grid_column_parte_nas_duas_pontas() {
    // `1 / -1` é como as folhas dizem "todas as colunas", e `span 6 / span 6` é
    // a forma que o Tailwind emite. As duas têm de chegar às DUAS pontas — uma
    // primeira versão que só lesse a primeira perdia o `-1` em silêncio.
    use crate::style::grid_lines::GridLine;
    let todas = parse_inline("grid-column: 1 / -1");
    assert_eq!(todas.grid_column_start, Some(GridLine::Line(1)));
    assert_eq!(todas.grid_column_end, Some(GridLine::Line(-1)));
    let seis = parse_inline("grid-column: span 6 / span 6");
    assert_eq!(seis.grid_column_start, Some(GridLine::Span(6)));
    assert_eq!(seis.grid_column_end, Some(GridLine::Span(6)));
    // sem barra, o `end` fica por declarar (não copia o `start`: a spec só o
    // copia para um <custom-ident>, e este módulo não tem idents).
    let um = parse_inline("grid-column: 5");
    assert_eq!(um.grid_column_start, Some(GridLine::Line(5)));
    assert_eq!(um.grid_column_end, None);
}

#[test]
fn colocacao_por_linha_e_por_nome_de_area_nao_se_apagam() {
    // O `ComputedStyle` tem os dois sistemas de colocação da spec ao mesmo
    // tempo. A condição deste lote é que o novo não escreva por cima do que já
    // existia: quem vier a colocar os itens precisa dos dois para decidir.
    let s = parse_inline("grid-area: cabecalho; grid-column-start: 2");
    assert!(s.grid_area.is_some(), "o nome da área continua lá");
    assert_eq!(s.get_property("grid-column-start"), "2");
}

#[test]
fn grid_column_nao_declarado_computa_auto_e_nao_polui_o_style_inline() {
    // As duas semânticas opostas de `style::initial`, nesta família. O shorthand
    // é o caso perigoso: responder `auto / auto` no `el.style` de todo elemento
    // do documento é exatamente o erro que aquele cabeçalho descreve.
    let s = parse_inline("color: red");
    assert_eq!(s.computed_value("grid-column-start", None), "auto");
    assert_eq!(
        s.get_property("grid-column"),
        "",
        "el.style só tem o declarado"
    );
    // declarada uma ponta só, o shorthand responde as duas.
    let d = parse_inline("grid-column-start: 2");
    assert_eq!(d.get_property("grid-column"), "2 / auto");
}

#[test]
fn grid_line_declarado_nao_move_a_caixa_hoje() {
    // O que este lote NÃO promete, fixado para não ser confundido com o que
    // promete. Se um dia o layout ler os quatro campos, este teste cai — e essa
    // é a intenção: é o marcador do ponto de enxerto, não uma defesa dele.
    let sem = layout("<div style='width:100px;height:10px'>a</div>", 800.0);
    let com = layout(
        "<div style='width:100px;height:10px;grid-column-start:7'>a</div>",
        800.0,
    );
    assert_eq!(
        format!("{:?}", itens(&sem)),
        format!("{:?}", itens(&com)),
        "guardar a linha não muda a geometria — ver o cabeçalho de grid_lines"
    );
}
