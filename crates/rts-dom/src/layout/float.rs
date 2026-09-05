//! FLOATS: as exclusões que um float cria e a banda livre que sobra para a
//! linha.
//!
//! Movido de `layout.rs` na modularização; nenhuma linha de lógica foi
//! alterada — a reconstrução destes pedaços é byte a byte a do original.

use super::*;
/// Um float JÁ COLOCADO, visto pelo fluxo que o rodeia: a faixa vertical que
/// ocupa e a aresta que deixa livre.
///
/// É esta a diferença entre o modelo antigo e o do CSS. O antigo guardava "a
/// linha de floats corrente" e empurrava para baixo dela tudo o que não fosse
/// float; o CSS diz que um float é um ESPAÇO DE EXCLUSÃO que o conteúdo
/// seguinte consulta e CONTORNA. Medido no Chrome, na Wikipédia: a `<figure>`
/// com `float:right` fica em `y=5877` e o `<p>` seguinte em `y=5869` — ACIMA do
/// topo do float, com a largura cheia da coluna (752). O parágrafo não desceu e
/// não encolheu: sobrepôs-se ao float, e só as suas LINHAS ficaram curtas.
#[derive(Clone, Copy)]
pub(crate) struct Exclusao {
    pub(in crate::layout) top: f32,
    pub(in crate::layout) bottom: f32,
    pub(in crate::layout) side: crate::style::FloatSide,
    /// A aresta INTERIOR do float — a fronteira que o fluxo não pode passar.
    /// Num float `left` é o x onde o conteúdo pode começar (aresta direita do
    /// float); num `right`, o x onde tem de terminar (aresta esquerda).
    pub(in crate::layout) edge: f32,
}

/// A banda horizontal livre entre `y` e `y + altura`, dadas as exclusões.
/// Devolve `(x, largura)` já recortados ao content do container.
///
/// A altura entra na pergunta porque uma linha de texto só é estorvada pelo
/// float com que se CRUZA: a última linha ao lado de uma figura curta usa a
/// banda estreita, e a primeira linha abaixo dela usa a largura toda.
pub(in crate::layout) fn banda_livre(ex: &[Exclusao], y: f32, altura: f32, content_x: f32, content_w: f32) -> (f32, f32) {
    let (mut esq, mut dir) = (content_x, content_x + content_w);
    // Uma linha de altura zero ainda cruza o float que começa exatamente nela —
    // sem esta espessura mínima, `y == top` não intersectava nada e a primeira
    // linha ao lado de um float saía com a largura toda.
    let fim = y + altura.max(0.01);
    for e in ex {
        if e.bottom <= y || e.top >= fim {
            continue;
        }
        match e.side {
            crate::style::FloatSide::Left => esq = esq.max(e.edge),
            crate::style::FloatSide::Right => dir = dir.min(e.edge),
            crate::style::FloatSide::None => {}
        }
    }
    (esq, (dir - esq).max(0.0))
}

/// O fundo do float mais baixo — para onde desce quem tem `clear`, e onde o
/// container fecha para os conter.
pub(in crate::layout) fn fundo_dos_floats(ex: &[Exclusao]) -> Option<f32> {
    ex.iter()
        .map(|e| e.bottom)
        .fold(None, |a: Option<f32>, b| Some(a.map_or(b, |a| a.max(b))))
}

/// O `float` computado de um nó-elemento (None p/ não-elemento/sem estilo).
pub(in crate::layout) fn float_of(dom: &Dom, id: NodeIdx) -> crate::style::FloatSide {
    dom.computed_style_idx(id)
        .and_then(|c| c.float_side)
        .unwrap_or(crate::style::FloatSide::None)
}

/// `true` se este filho FECHA a corrida inline a que pertenceria no cálculo
/// do max-content (`medida::intrinsic_content_width`) — ou seja, se a linha
/// acaba nele em vez de continuar somando com o próximo irmão.
///
/// `is_block_level` sozinho não serve: responde `true` a um `inline-block`
/// (que cria caixa mas NÃO quebra a linha — o mesmo "não é inline?" que esta
/// árvore já pagou cinco vezes) e a um FLOAT (que blockifica mas EMPACOTA ao
/// lado dos vizinhos em vez de quebrar: max-content é "disponível infinito",
/// e com espaço infinito nenhum float desce de linha). O `<br>` é o oposto:
/// não é de bloco, não cria caixa, e quebra na mesma.
///
/// Sem a exceção do float, o max-content de um bloco só de floats tomava o
/// MAIOR em vez da SOMA (32 em vez de 160 para 5 floats de 32px lado a lado
/// — `flexbox-flex-wrap-horiz-002`/`-vert-001/002`, WPT, lote
/// `flex-basis-content-wrap`).
pub(in crate::layout) fn fecha_a_corrida(dom: &Dom, id: NodeIdx) -> bool {
    if let NodeKind::Element { tag } = &dom.node(id).kind {
        if tag == "br" {
            return true;
        }
    }
    if float_of(dom, id) != crate::style::FloatSide::None {
        return false;
    }
    is_block_level(dom, id) && !is_inline_block(dom, id)
}
