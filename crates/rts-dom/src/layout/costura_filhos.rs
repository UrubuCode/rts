//! As duas perguntas que a COSTURA faz antes de reusar o desenho de um
//! container trocando só os filhos sujos (`fragmento::costurar`).
//!
//! Vivem fora de `fragmento.rs` porque aquele ficheiro já passou do teto e não
//! cresce, e porque são exatamente as duas perguntas cujo erro não se vê: uma
//! costura que devia ter sido recusada repinta o desenho anterior, internamente
//! consistente e errado (I3 de `docs/ui/html-engine/box-tree.md`).

use crate::boxes::{BoxId, BoxKind, BoxTree};
use crate::dom::NodeIdx;

use super::fragmento::ChildRef;

/// `true` só quando a caixa recém-construída tem a mesma sequência de filhos
/// que a caixa que produziu o desenho antigo — CAIXA contra CAIXA, cada uma na
/// árvore que a emitiu.
///
/// A versão anterior comparava `tree.children(pai)` com a lista de
/// `ChildRef`s. As duas respondem a perguntas diferentes: a primeira tem uma
/// caixa de TEXTO por cada nó de texto, espaço em branco incluído, e a segunda
/// só tem os blocos que viraram fragmento. Com HTML indentado nunca casavam, e
/// a costura morria em silêncio em toda página real.
///
/// **Rejeitado: traduzir as caixas para nós para fazer casar** — o cuidado
/// nomeado em PLAN §9 BT-1. Comparar nós deixa de ver uma mudança na estrutura
/// de CAIXAS que não mexe na de nós: um wrapper anónimo que aparece, um inline
/// que se parte em dois. Por isso a comparação é pelo `BoxKind`, que é a
/// identidade da caixa entre construções (o nó que a gera, ou o nó de que
/// herda quando é anónima), mais a posição dela entre as caixas do seu nó — e
/// DESCE às anónimas, cujo conteúdo é uma partição que pode mudar sozinha.
pub(in crate::layout) fn mesma_sequencia_de_filhos(
    antiga: &BoxTree,
    pai_antigo: BoxId,
    nova: &BoxTree,
    pai_novo: BoxId,
) -> bool {
    let (a, b) = (antiga.children(pai_antigo), nova.children(pai_novo));
    a.len() == b.len()
        && a
            .iter()
            .zip(b)
            .all(|(&x, &y)| mesma_caixa(antiga, x, nova, y))
}

fn mesma_caixa(antiga: &BoxTree, x: BoxId, nova: &BoxTree, y: BoxId) -> bool {
    let tipo = antiga.kind(x);
    if tipo != nova.kind(y) {
        return false;
    }
    match tipo {
        BoxKind::Anonymous { .. } => mesma_sequencia_de_filhos(antiga, x, nova, y),
        // A generated box is named whole by its `BoxKind` — originating element
        // AND which pseudo — and has no children, so equal kinds at the same
        // position are the same box. One that appeared or went away changes
        // the LENGTH of the sequence above and is refused there, which the old
        // comparison (with no generated boxes in the tree) could not do.
        //
        // What this cannot see is a change of its CONTENT: the text is not in
        // the tree at all, it is asked of the cascade. That is not a structure
        // question, and the pseudo is painted into its originating element's
        // own items — which `costurar` never reuses when that element is the
        // root of the `touch_*` (self-dirty) or inside it (no dirty-children
        // marks). A `counter()` fed by a DESCENDANT is the case neither covers,
        // and it predates the tree.
        BoxKind::Generated { .. } => true,
        BoxKind::Element(no) | BoxKind::Text { node: no, .. } => {
            // Um nó que passou a gerar outra quantidade de caixas (um inline
            // partido, ou que deixou de o estar) mudou de estrutura mesmo com
            // o mesmo `BoxKind` na mesma posição.
            let (de_a, de_b) = (antiga.boxes_of(no), nova.boxes_of(no));
            de_a.len() == de_b.len()
                && de_a.iter().position(|&c| c == x) == de_b.iter().position(|&c| c == y)
        }
    }
}

/// `true` só quando todo filho SUJO tem um fragmento próprio entre os
/// `children` — é o único desenho que a costura sabe refazer.
///
/// Um filho sujo sem `ChildRef` (texto solto, um inline, um nó dentro de uma
/// caixa anónima) desenha dentro dos itens do PRÓPRIO container, e a costura
/// reusa esses itens tal como estavam. A comparação antiga com a lista de
/// elementos do DOM garantia isto por tabela, e só para filhos ELEMENTO;
/// comparando caixas deixou de garantir, e a pergunta passa a ser explícita.
pub(in crate::layout) fn sujeira_coberta(
    tree: &BoxTree,
    sujos: &[NodeIdx],
    children: &[ChildRef],
) -> bool {
    sujos.iter().all(|&sujo| {
        children
            .iter()
            .any(|child| tree.node_of(child.caixa) == Some(sujo))
    })
}
