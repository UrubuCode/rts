//! Pintura de caixas que a grade cria SEM `layout_block` — linhas, grupos de
//! linhas, e as duas propriedades visuais que `table-column`/
//! `table-column-group` ainda alcançam apesar de não gerarem caixa nenhuma
//! (CSS 2.1 §17.2.1): o `background` (§17.5.1, atrás de cada célula da
//! coluna) e a `border` (§17.4, só com `border-collapse:collapse`).
//!
//! Extraído de `table/mod.rs` para não ultrapassar o teto de 500 linhas do
//! módulo — `layout_table` já tinha a grade inteira; isto é só a pintura das
//! caixas que ela não entrega ao `layout_block`.

use super::*;

/// Pinta fundo e borda de uma caixa que não passou pelo `layout_block` (linha ou
/// grupo de linhas), inserindo os itens em `at` para ficarem ATRÁS do que já lá
/// está. Sem isto, um `<tr>` com `background` não pintava nada: a linha nunca é
/// um bloco, e era o `layout_block` que fazia esta parte para todos os outros.
pub(super) fn pinta_caixa(
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

/// O fundo de cada `table-column`/`table-column-group` que atravessa a célula
/// `[col, col+colspan)`, na ordem de `g.col_bg` (grupo antes da sua coluna —
/// ver `grid.rs::coleta_grupo_de_colunas`). Chamada ANTES de `layout_block` da
/// célula: um `push` simples (não `insert_item`) já basta, porque o ponto de
/// inserção É a fronteira da lista neste instante — nada foi pintado ainda
/// para esta célula.
///
/// CORTE dito: a ordem completa da spec (CSS 2.1 §17.5.1) é tabela < grupo-de-
/// colunas < coluna < grupo-de-linhas < linha < célula; aqui a coluna só é
/// garantida atrás da CÉLULA (o que os 12 fixtures `*-applies-to-005`/`-006`
/// pedem — nenhum declara fundo de `<tr>`/`<tbody>` a competir). Contra um
/// fundo de LINHA/GRUPO — que se insere DEPOIS, por `pinta_caixa` acima, no
/// mesmo `idx_fundo` reservado antes desta célula — a coluna acaba à FRENTE
/// dele, invertido: um caso a resolver no dia em que um fixture o exigir.
pub(super) fn pinta_fundo_de_coluna(
    dom: &Dom,
    rect: Rect,
    col: usize,
    colspan: usize,
    col_bg: &[(usize, usize, NodeIdx)],
    list: &mut DisplayList,
) {
    for &(ini, fim, node) in col_bg {
        if ini < col + colspan && fim > col {
            emite_fundo_simples(dom, node, rect, list);
        }
    }
}

/// A borda de uma coluna/grupo-de-colunas, à volta de `rect` — SEM o `has_box()`
/// que `pinta_caixa` exige (um `table-column`/`table-column-group` nunca passa
/// nele: é exatamente o que não gera caixa). Reusa `border_items`, a mesma
/// função que pinta a borda de QUALQUER caixa — a diferença entre uma coluna e
/// um `<tr>` é só QUANDO e QUE retângulo se usa, não como a borda em si se
/// desenha.
pub(super) fn pinta_borda_de_coluna(dom: &Dom, id: NodeIdx, rect: Rect, list: &mut DisplayList) {
    let Some(css) = dom.computed_style_idx(id) else {
        return;
    };
    let radius = css.corner_radius.unwrap_or(0.0);
    for item in crate::layout::border_items(
        &css,
        rect,
        radius,
        1.0,
        crate::painteffects::filtro(css.filter.as_deref().unwrap_or("")),
    ) {
        list.items.push(item);
    }
}

/// Só o RETÂNGULO de fundo (cor, gradiente, imagem) de `id` — sem borda, sem
/// sombra. A BORDA de uma coluna é outra função (`pinta_borda_de_coluna`,
/// acima) porque vive noutro RETÂNGULO: o fundo pinta-se por CÉLULA, atrás
/// dela (`pinta_fundo_de_coluna`, acima); a borda pinta-se uma vez pelo
/// intervalo INTEIRO de colunas, depois de todas as linhas (só tem efeito com
/// `border-collapse:collapse` — CSS 2.1 §17.4 — ver o corte em
/// `pinta_borda_de_coluna`). Reusa `fundo_imagem::background_pixels_items`
/// para a imagem — mesma composição de ladrilhos que qualquer caixa,
/// `pub(crate)` desde este lote por essa razão.
fn emite_fundo_simples(dom: &Dom, id: NodeIdx, rect: Rect, list: &mut DisplayList) {
    let Some(css) = dom.computed_style_idx(id) else {
        return;
    };
    if let Some(g) = css.gradient {
        list.items.push(DisplayItem::GradientRect {
            rect,
            c0: g.c0,
            c1: g.c1,
            angle_deg: g.angle_deg,
            radius: 0.0,
        });
    } else if let Some(bg) = css.bg {
        list.items.push(DisplayItem::SolidRect {
            rect,
            color: bg,
            radius: crate::layout::Corners::ZERO,
        });
    }
    let filhos = list.children.len();
    for item in crate::layout::fundo_imagem::background_pixels_items(
        dom, id, &css, rect, 0.0, 0.0, 0.0, 0.0, filhos, filhos,
    ) {
        list.items.push(item);
    }
}
