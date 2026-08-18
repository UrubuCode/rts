//! Os testes de comportamento do FLUXO DE BLOCO: o colapso de margens verticais
//! e a ordem de pintura de um elemento fora do fluxo.
//!
//! Vivem num módulo próprio e não dentro do `layout.rs` porque aquele ficheiro
//! passou há muito o teto e é editado por mais do que uma pessoa ao mesmo
//! tempo; e não dentro de `table/tests.rs` porque não têm nada de tabela — o
//! que partilham com ele são só os ajudantes, que montam um documento e leem a
//! geometria de um seletor.

use crate::layout::{DisplayItem, Rect};
use crate::table::tests::{geometria, rect};

/// As três regras do colapso de margens verticais (CSS 2.1 §8.3.1), pelo efeito
/// na posição do segundo bloco. A do meio é a que estava errada.
#[test]
fn o_colapso_de_margens_nao_e_o_maximo_quando_uma_delas_e_negativa() {
    let caso = |estilo: &str| -> f32 {
        let html = format!(
            "<div style='height:30px'>a</div><div style='height:30px; {estilo}'>b</div>"
        );
        let (dom, list) = geometria(&html, 600.0);
        rect(&dom, &list, "div", 1).y
    };
    // Duas positivas: a maior vence (0 e 10 → 10).
    assert!((caso("margin-top:10px") - 40.0).abs() < 0.5, "positiva: {}", caso("margin-top:10px"));
    // Sinais mistos: SOMAM. Uma margem negativa puxa o bloco para cima, e é o
    // `margin-top` negativo dos gutters `.row` do Bootstrap; com `max(a,b)` a
    // negativa era simplesmente ignorada e o bloco ficava em 30.
    assert!((caso("margin-top:-10px") - 20.0).abs() < 0.5, "negativa: {}", caso("margin-top:-10px"));
    // E uma negativa maior que a altura anterior puxa para cima na mesma.
    assert!((caso("margin-top:-40px") - (-10.0)).abs() < 0.5, "muito negativa: {}", caso("margin-top:-40px"));
}

/// Um elemento fora do fluxo (`position:fixed`) pinta DEPOIS do conteúdo, não
/// antes — senão fica atrás dele.
///
/// O sintoma numa página real é o dropdown a desaparecer por trás do artigo. A
/// causa não era a ordem em que ele é calculado (essa estava certa: a passada
/// out-of-flow corre no fim) mas o deslocamento dos índices: ao inserir o fundo
/// da sua própria caixa no índice 0 da lista de topo, o fixed empurrava para
/// depois de si a subárvore onde vive a página inteira.
#[test]
fn um_fixed_pinta_por_cima_do_fluxo_e_nao_por_baixo() {
    let html = "<div style='background:#111; height:30px'>fluxo</div>                <div style='position:fixed; top:0; left:0; width:50px; height:20px; background:#900'>t</div>";
    let (_, list) = geometria(html, 600.0);
    let ordem: Vec<Rect> = list
        .materialized()
        .iter()
        .filter_map(|i| match i {
            DisplayItem::SolidRect { rect, .. } => Some(*rect),
            _ => None,
        })
        .collect();
    let ultimo = ordem.last().expect("algum fundo pintado");
    assert!((ultimo.w - 50.0).abs() < 0.5, "o último a pintar devia ser o fixed: {ordem:?}");
}
