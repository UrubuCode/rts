//! O CLEARFIX: um `::after { display: block; clear: both }` no contentor.
//!
//! O pseudo-elemento de bloco não é um nó e este layout não lhe dá caixa; o
//! que ele FAZ, no entanto, é uma coisa só e mede-se: desce o fim do fluxo do
//! contentor até ao fundo dos floats que `clear` nomeia, e com isso o
//! contentor passa a conter os filhos flutuantes (CSS 2.1 §9.5.2). É a
//! referência de 20 reftests de flexbox do WPT e o padrão mais comum de
//! contenção de floats na web até ao `flow-root`. CORTE dito: `content`,
//! `height` e o fundo do próprio pseudo de bloco não são desenhados — só o
//! efeito do `clear`.

use super::*;

/// O `y` a que o fluxo do contentor `id` tem de descer por causa do seu
/// `::after` de bloco com `clear`, se existir e houver floats abertos no
/// `bfc` do lado pedido.
pub(in crate::layout) fn fundo_do_clearfix(
    dom: &Dom,
    id: NodeIdx,
    bfc: &BlockFormattingContext,
) -> Option<f32> {
    let caixa = dom.pseudo_box(id, crate::style::PseudoElement::After)?;
    let de_bloco = matches!(
        caixa.css.effective_display(),
        Some(crate::style::DisplayKind::Block | crate::style::DisplayKind::Flex | crate::style::DisplayKind::Grid)
    );
    if !de_bloco {
        return None;
    }
    let (esq, dir) = caixa.css.clear?.sides();
    if !esq && !dir {
        return None;
    }
    bfc.fundo_lado(esq, dir)
}
