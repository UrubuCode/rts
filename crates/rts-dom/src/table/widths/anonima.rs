//! O mínimo de uma caixa ANÓNIMA (CSS 2.1 §9.2.1.1), extraído de `mod.rs` para
//! ficar abaixo do tecto de 500 linhas depois de 02bc7088d somar a travessia
//! pela árvore de caixas.
//!
//! `min_content_na_arvore` (em `mod.rs`) saltava uma caixa sem nó com um
//! `continue` — herdado do mesmo padrão em `layout::medida::intrinsic_content_width`
//! antes do fix irmão daquele ficheiro. Numa célula como
//! `<td><span>aaaa<div>b</div>cccc</span></td>`, uma vez que algo mais fundo
//! na célula seja um contentor de FLUXO que efetivamente parte (o `<span>`
//! por si só, dentro de uma `<td>`, não parte hoje — `inner_of(TableCell) ==
//! InnerDisplay::Table`, fora do escopo deste lote — mas um `<div
//! style="display:flow-root">` lá dentro parte), os textos "aaaa"/"cccc"
//! deixavam de contar para o mínimo da coluna.

use super::{em_linha, min_content_na_arvore};
use crate::boxes::{BoxId, BoxTree};
use crate::layout::LayoutCtx;
use crate::Dom;

/// A largura MÍNIMA de uma caixa anónima: sem nó, sem `width` e sem moldura
/// próprios — o seu conteúdo é o RUN que a gerou, medido pela mesma regra
/// "sem quebra soma, com quebra empilha pelo máximo" que
/// `min_content_na_arvore` já aplica aos filhos de um elemento normal. Uma
/// caixa anónima é SEMPRE de bloco
/// (`boxes::context::formatting_context`), por isso um filho seu que
/// seja ele próprio anónimo (`boxes::build` não produz um hoje, mas nada
/// impede de vir a produzir) entra pelo máximo, nunca pela soma.
pub(super) fn min_content_anonima(
    dom: &Dom,
    tree: &BoxTree,
    caixa: BoxId,
    font: f32,
    ctx: &LayoutCtx,
    sem_quebra: bool,
    mono: bool,
) -> f32 {
    let mut m = 0.0f32;
    let mut linha = 0.0f32;
    for &filho in tree.children(caixa) {
        let Some(c) = tree.node_of(filho) else {
            let w = min_content_anonima(dom, tree, filho, font, ctx, sem_quebra, mono);
            m = m.max(w);
            continue;
        };
        if crate::layout::is_out_of_flow(dom, c) {
            continue;
        }
        // SEMPRE `true`: dentro de uma caixa anónima já não há "topo" —
        // `floor_width` só distingue o nó que o CHAMADOR de `min_content`
        // pediu (ver o comentário do parâmetro em `mod.rs`).
        let w = min_content_na_arvore(dom, tree, c, Some(filho), font, ctx, sem_quebra, mono, true);
        if sem_quebra && em_linha(dom, c) {
            linha += w;
        } else {
            m = m.max(w);
        }
    }
    m.max(linha)
}
