//! O RAIO POR CANTO na display list.
//!
//! O modelo tinha um raio para os quatro cantos e o corpus tem 334 declarações
//! de canto isolado (`border-top-left-radius` e as sete companhias) mais 19
//! `border-radius` de dois ou mais valores. As primeiras saíam pintadas
//! QUADRADAS — um canto declarado sozinho nunca tocava o campo único, por uma
//! recusa deliberada de `style::radius`: escrevê-lo ali arredondaria os outros
//! três. As segundas saíam com os quatro cantos iguais.
//!
//! O que cada teste aqui fixa é uma dessas duas caras, mais a condição que o
//! lote não podia quebrar e a pergunta de que o desempenho do backend depende.

use crate::layout::{Corners, DisplayItem};
use crate::table::tests::geometria;

/// Os cantos do primeiro fundo pintado com a cor dada.
fn cantos(html: &str, cor: u32) -> Corners {
    let (_, l) = geometria(html, 600.0);
    l.materialized()
        .iter()
        .find_map(|i| match i {
            DisplayItem::SolidRect { color, radius, .. } if *color == cor => Some(*radius),
            _ => None,
        })
        .expect("o fundo devia ter sido pintado")
}

/// Um canto declarado sozinho arredonda ESSE canto e só ele. É a cara maior do
/// defeito: 334 declarações do corpus, todas pintadas quadradas até aqui.
#[test]
fn um_canto_declarado_sozinho_arredonda_so_esse_canto() {
    let c = cantos(
        "<div style='background:#abcdef; width:50px; height:50px; border-top-left-radius:8px'>x</div>",
        0xABCDEFFF,
    );
    assert_eq!(c.tl, 8.0, "o canto declarado: {c:?}");
    assert_eq!(
        (c.tr, c.br, c.bl),
        (0.0, 0.0, 0.0),
        "os outros três não mexem: {c:?}"
    );
}

/// `border-radius: 2px 2px 0 0` — a forma do cabeçalho de cartão do Bootstrap —
/// arredonda em cima e deixa em baixo quadrado. Antes dava os quatro iguais.
#[test]
fn o_shorthand_de_quatro_valores_da_quatro_cantos_diferentes() {
    let c = cantos(
        "<div style='background:#abcdef; width:50px; height:50px; border-radius:2px 2px 0 0'>x</div>",
        0xABCDEFFF,
    );
    assert_eq!((c.tl, c.tr), (2.0, 2.0), "em cima arredonda: {c:?}");
    assert_eq!((c.br, c.bl), (0.0, 0.0), "em baixo é quadrado: {c:?}");
}

/// A condição inegociável, vista do lado da PINTURA: um `border-radius` de um
/// valor continua a arredondar os quatro cantos por igual. Quem já lia o campo
/// único não pode receber resposta diferente por causa dos cantos.
#[test]
fn um_border_radius_de_um_valor_continua_a_arredondar_os_quatro() {
    let c = cantos(
        "<div style='background:#abcdef; width:50px; height:50px; border-radius:6px'>x</div>",
        0xABCDEFFF,
    );
    assert_eq!(c, Corners::same(6.0), "os quatro a 6: {c:?}");
}

/// Sem declaração nenhuma, os quatro cantos são zero — e é isso que deixa o
/// backend recortar o retângulo ao visível.
#[test]
fn sem_declaracao_os_quatro_cantos_sao_zero() {
    let c = cantos(
        "<div style='background:#abcdef; width:50px; height:50px'>x</div>",
        0xABCDEFFF,
    );
    assert_eq!(c, Corners::ZERO);
    assert!(!c.any(), "sem raio não há canto arredondado");
}

/// `any()` é a pergunta de que depende o recorte ao visível no backend, e é
/// sobre os QUATRO cantos.
///
/// Não há teste de layout que apanhe o erro que ela evita: responder pelo
/// primeiro canto faria o fundo de um `<div>` de dezenas de milhares de pontos
/// voltar inteiro ao tesselador sempre que o canto lido fosse zero — que é o
/// caso comum de um retângulo arredondado só de um lado. É uma regressão de
/// desempenho invisível para a geometria, por isso é fixada aqui.
#[test]
fn any_pergunta_pelos_quatro_cantos_e_nao_pelo_primeiro() {
    assert!(!Corners::ZERO.any());
    assert!(Corners::same(1.0).any());
    // O canto que arredonda é o ÚLTIMO: quem responder pelo primeiro diz que não
    // há raio e recorta um desenho arredondado.
    assert!(
        Corners {
            tl: 0.0,
            tr: 0.0,
            br: 0.0,
            bl: 3.0
        }
        .any(),
        "o último canto conta"
    );
    assert!(
        Corners {
            tl: 0.0,
            tr: 2.0,
            br: 0.0,
            bl: 0.0
        }
        .any(),
        "o segundo canto conta"
    );
}
