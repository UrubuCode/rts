//! Um bloco que ESTABELECE o seu próprio contexto de formatação (`flex`,
//! `overflow` ≠ visible, `flow-root`, uma tabela, …) não pode sobrepor a
//! margin-box de um `float` anterior — CSS 2.1 §9.5: "the border box of ...
//! an element that establishes a new block formatting context ... must not
//! overlap ... any floats". É o OPOSTO de um bloco NORMAL, cuja caixa se
//! sobrepõe de propósito e só as linhas lá dentro desviam (o comentário no
//! ramo `child_block` de `vertical.rs` já o diz, e continua certo — para
//! quem NÃO estabelece BFC).
//!
//! `bloco.rs::establishes_block_formatting_context` já classificava
//! `display:flex` (e `overflow:hidden`, `flow-root`, …) como raiz de BFC;
//! faltava alguém CONSULTAR isso antes de posicionar a caixa contra o `bfc`
//! ambiente — é o que este módulo faz, no mesmo ponto em que `clear` já
//! desce o cursor (mesma forma, causa distinta).
//!
//! Medido (`claude-flex-bfc-evita-float`): `#flutua{float:left;width:80px}`
//! seguido de `#flex{display:flex;width:256px}` num `body{width:320px}` — não
//! cabe ao lado (320−80=240 < 256), tem de descer para y=40 (o fundo do
//! float). O motor sobrepunha-o em y=0, como se `display:flex` não
//! estabelecesse BFC nenhum para este efeito.
//!
//! CORTE dito: só a largura RESOLVÍVEL sem medir o conteúdo — a mesma que
//! `child_outer_width` já usa para floats — decide se cabe. Uma largura
//! `auto` que devia ENCOLHER para caber ao lado do float (em vez de
//! descer) — o outro ramo do §9.5, "shrink to avoid" — não é este caso: hoje,
//! como antes deste lote, ela toma a largura natural do conteúdo e nunca
//! desce nem encolhe pelo float. Só o caso de largura FIXA que HOJE se
//! sobrepõe está coberto.

use super::*;

/// O `y` para onde `child` (que estabelece BFC) tem de descer se a largura
/// que ocuparia em `y_provisorio` não coubesse na banda livre — o fundo dos
/// floats dos dois lados, como um `clear:both` implícito (CSS 2.1 §9.5: sem
/// espaço para "shrink to avoid" numa largura fixa, resta empurrar). `None`
/// quando cabe, quando `child` não estabelece BFC, ou quando não há float
/// nenhum aberto no `bfc` ambiente.
pub(in crate::layout) fn empurra_para_baixo(
    dom: &Dom,
    child: NodeIdx,
    child_css: &ComputedStyle,
    y_provisorio: f32,
    content_x: f32,
    content_w: f32,
    font_size: f32,
    bfc: &BlockFormattingContext,
    ctx: &LayoutCtx,
) -> Option<f32> {
    if bfc.is_empty() || !super::bloco::establishes_block_formatting_context(dom, child, child_css) {
        return None;
    }
    let w = child_outer_width(dom, child, content_w, font_size, ctx);
    let (_, banda_w) = bfc.banda_livre(y_provisorio, 0.0, content_x, content_w);
    if w <= banda_w + 0.01 {
        return None;
    }
    bfc.fundo_lado(true, true)
}
