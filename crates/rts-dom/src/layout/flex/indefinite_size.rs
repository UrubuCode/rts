//! Uma dimensão (`width`/`height`) é INDEFINIDA para efeitos de stretch tanto
//! quando está AUSENTE quanto quando foi declarada `auto` explicitamente — a
//! spec (CSS Flexbox 3 §4.5: "if a cross size property... computes to auto")
//! não distingue as duas, mas boa parte deste layout usa `.is_none()` como o
//! teste, e isso só cobre a AUSENTE: `Option<Dimension>` guarda `auto` como
//! `Some(Dimension::Auto)`, não `None`
//! (`style/lengths.rs::parse_dimension`).
//!
//! Achado por `flexbox_align-self-stretch.html` (lote
//! `flex-align-justify-familia`): `height: auto` DECLARADO — para vencer o
//! `height: 3em` de um seletor anterior mais fraco — bloqueava `can_stretch`
//! em `flex.rs`, e o item ficava na altura do texto (18px) em vez de esticar
//! aos 96px do container (o `align-self: stretch` do mesmo item nunca chegava
//! a ser lido). Só o caminho de linha (`flex.rs`, ROW) é corrigido com isto;
//! os mesmos `.is_none()` em `coluna_wrap.rs`, `grid.rs` e `posicionado.rs`
//! são o corte IDÊNTICO que outro lote já tinha registado
//! (`grid.rs:355`/`coluna.rs:310`, citado em `flex-justify-logico` no PLAN) —
//! ficam por tocar, fora do âmbito deste.

use crate::style::Dimension;

/// `true` quando `d` não impõe um tamanho definido: ausente OU `auto`
/// explícito. É a pergunta que o stretch do eixo cruzado faz — não "a
/// propriedade foi escrita?", mas "o valor computado é `auto`?".
pub(in crate::layout) fn e_auto_ou_ausente(d: Option<Dimension>) -> bool {
    matches!(d, None | Some(Dimension::Auto))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ausente_e_indefinida() {
        assert!(e_auto_ou_ausente(None));
    }

    #[test]
    fn auto_declarado_e_indefinida() {
        assert!(e_auto_ou_ausente(Some(Dimension::Auto)));
    }

    #[test]
    fn um_comprimento_declarado_e_definido() {
        assert!(!e_auto_ou_ausente(Some(Dimension::Px(96.0))));
        assert!(!e_auto_ou_ausente(Some(Dimension::Percent(50.0))));
    }
}
