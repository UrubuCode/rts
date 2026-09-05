//! Que elementos precisam de um `forced_outer_w` EXPLÍCITO para que
//! `align-items: stretch` os encha no eixo cruzado de uma coluna
//! (`coluna.rs`) — a mesma pergunta que `flex.rs` já faz no eixo horizontal
//! (`replaced_transferido.rs`).
//!
//! Um bloco comum já ocupa `content_w` sozinho (`measure_block` com
//! `width:auto` mede-o à largura disponível): o stretch não precisa de
//! IMPOR nada. Um elemento REPLACED com tamanho intrínseco PRÓPRIO, porém,
//! mede-se pelo natural quando `width` está ausente — `<img>` pelos pixels,
//! `<input type=checkbox/radio>` pelo quadrado de 13px
//! (`layout/input.rs::CAIXA_DE_MARCA`) — e por isso fica curto sem a
//! imposição. `<input>` (fora de checkbox/radio) e `<table>` continuam de
//! fora: um campo de texto já reserva 180px OU `avail_w` sozinho
//! (`medida_do_input`), e uma tabela já ocupa a largura toda pelo algoritmo
//! de colunas — nenhum dos dois precisa de ajuda.
//!
//! Achado no lote `flex-desvios-pequenos` (WPT
//! `stretch-flex-item-checkbox-input`/`-radio-input`): a exclusão de
//! `<input>` era total (comentário "table/input ficam de fora" em
//! `coluna.rs`), quando só o CAMPO DE TEXTO (que já se preenche sozinho)
//! precisava de ficar fora — o quadrado de marca, não.

use crate::dom::{Dom, NodeIdx, NodeKind};

/// `true` para `<img>` e `<input type=checkbox|radio>` — os replaced cujo
/// tamanho natural (sem `width` declarado) não enche o contentor sozinho.
pub(in crate::layout) fn precisa_de_forced_w_no_stretch(dom: &Dom, id: NodeIdx) -> bool {
    let NodeKind::Element { tag } = &dom.node(id).kind else {
        return false;
    };
    if tag == "img" {
        return true;
    }
    if tag == "input" {
        let tipo = dom.node(id).attr("type").map(str::to_ascii_lowercase);
        return matches!(tipo.as_deref(), Some("checkbox") | Some("radio"));
    }
    false
}
