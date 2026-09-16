//! Os `g.outros` da tabela (`<caption>` e o que `grid.rs` desviou da grade por
//! estar fora do fluxo) QUANDO flutuam.
//!
//! `layout_table` empilhava todo `g.outros` incondicionalmente à largura
//! CHEIA da tabela — certo para um `<caption>`, errado para um `float`: a
//! caixa dele encosta ao lado pedido, na sua PRÓPRIA largura, em vez de
//! esticar. Isso NÃO significa que ele deixa a altura de fora — ver o
//! comentário de [`pinta_outros`] sobre a grade descer para lhe limpar a
//! base, que foi medido contra o Blink e não deduzido da spec de cabeça.
//!
//! Replicado aqui em vez de reusar `layout/vertical.rs` porque
//! `vertical.rs::child_outer_width`/`bfc::banda_livre` são
//! `pub(in crate::layout)`, e uma tabela nunca tem MAIS de um flutuado nesta
//! lista nos fixtures confirmados (`float-applies-to-005/006/015`), o que
//! torna a versão completa (exclusões, banda livre entre vários floats)
//! trabalho sem régua a pedi-lo.
//!
//! CORTE dito: a largura sai só de `width` (nunca `shrink-to-fit` de
//! conteúdo, nem `min-/max-width`, nem margem) — os três fixtures confirmados
//! declaram `width` explícito e margem zero. Um `float:left/right` sem
//! `width` aqui dentro ainda cai na largura da tabela inteira, que é o
//! comportamento de ANTES deste lote, não uma regressão nova.

use super::*;

/// `<caption>` e outros blocos avulsos da tabela (`g.outros`, montado por
/// `grid.rs`): empilham à largura CHEIA da tabela — exceto um `float`, que
/// encosta ao lado pedido na SUA PRÓPRIA largura (`largura_flutuante`) em vez
/// de esticar e empilhar. Devolve o `y` onde a GRADE deve começar.
///
/// **A grade sempre desce até à base do flutuado mais alto — nunca fica ao
/// lado dele.** Medido contra o Blink (`claude-table-outros-flutuante.html`):
/// a primeira versão deste código não avançava `y` nenhum para um flutuado,
/// pela leitura comum de CSS 2.1 §9.5.1 ("um flutuado não empurra os
/// irmãos") — e o Blink mede a grade 30px mais abaixo, exatamente a altura
/// do flutuado. A razão é §9.5 (não §9.5.1): a caixa de um elemento que
/// ESTABELECE O SEU PRÓPRIO contexto de formatação — uma tabela sempre
/// estabelece — nunca sobrepõe a margem de um flutuado; como a grade ocupa
/// sempre a largura INTEIRA da tabela, nunca há banda livre ao lado do
/// flutuado larga o suficiente, e ela desce sempre até limpar a base dele. A
/// regra de "não empurra os irmãos" vale para conteúdo que pode ENCOLHER
/// para caber ao lado (o texto de um `<p>`); a grade de uma tabela não pode.
#[allow(clippy::too_many_arguments)]
pub(super) fn pinta_outros(
    dom: &Dom,
    outros: &[NodeIdx],
    content_x: f32,
    y0: f32,
    content_w: f32,
    font_size: f32,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) -> f32 {
    let mut y = y0;
    // A base do flutuado mais alto visto até agora — separado de `y` porque
    // um flutuado NÃO avança `y` para o item seguinte de `outros` (dois
    // flutuados do mesmo lado ficam lado a lado, não empilhados), só a grade
    // no fim precisa de limpar o mais alto de todos.
    let mut base_flutuantes = y;
    for &o in outros {
        match lado_flutuante(dom, o) {
            crate::style::FloatSide::None => {
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
                    false,
                    &crate::layout::BlockFormattingContext::new(),
                    ctx,
                    list,
                );
                y += h;
                base_flutuantes = y;
            }
            lado => {
                let w = largura_flutuante(dom, o, content_w, font_size, ctx);
                let x = if lado == crate::style::FloatSide::Left {
                    content_x
                } else {
                    content_x + content_w - w
                };
                let (_, h) = crate::layout::layout_block(
                    dom,
                    o,
                    x,
                    y,
                    content_w,
                    None,
                    Some(w),
                    None,
                    false,
                    true,
                    &crate::layout::BlockFormattingContext::new(),
                    ctx,
                    list,
                );
                base_flutuantes = base_flutuantes.max(y + h);
            }
        }
    }
    base_flutuantes
}

/// `true` se `o` (um item de `g.outros`) tem `float:left`/`right` — o ramo que
/// `pinta_outros` escolhe entre empilhar (o `<caption>` comum) e encostar ao
/// lado.
fn lado_flutuante(dom: &Dom, o: NodeIdx) -> crate::style::FloatSide {
    dom.computed_style_idx(o)
        .and_then(|c| c.float_side)
        .unwrap_or(crate::style::FloatSide::None)
}

/// A largura OUTER do flutuado — só `width` resolvido contra `content_w` (o
/// corte do topo do módulo). `None` (auto) cai na largura da tabela inteira,
/// como um bloco comum sem `width` declarado.
fn largura_flutuante(
    dom: &Dom,
    o: NodeIdx,
    content_w: f32,
    font_size: f32,
    ctx: &LayoutCtx,
) -> f32 {
    let css = dom.computed_style_idx(o).unwrap_or_default();
    let resolve = crate::style::ResolveCtx {
        parent_content_w: content_w,
        node_font_size: font_size,
        root_font_size: crate::layout::DEFAULT_FONT_SIZE,
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    css.width
        .and_then(|d| d.resolve(&resolve))
        .unwrap_or(content_w)
}
