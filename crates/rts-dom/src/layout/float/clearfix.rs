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
pub(in crate::layout) fn clearfix_bottom(
    dom: &Dom,
    id: NodeIdx,
    bfc: &BlockFormattingContext,
) -> Option<f32> {
    let pseudo = dom.pseudo_box(id, crate::style::PseudoElement::After)?;
    let is_block = matches!(
        pseudo.css.effective_display(),
        Some(crate::style::DisplayKind::Block | crate::style::DisplayKind::Flex | crate::style::DisplayKind::Grid)
    );
    if !is_block {
        return None;
    }
    let (left, right) = pseudo.css.clear?.sides();
    if !left && !right {
        return None;
    }
    bfc.side_bottom(left, right)
}
