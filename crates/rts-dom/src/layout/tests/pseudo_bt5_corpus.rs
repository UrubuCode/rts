//! BT-5 (issue #2731): `::before`/`::after` deixaram de ter uma cópia da
//! medição de padding/borda/margem e da pintura em cada papel (bloco em
//! `pseudo_bloco.rs`, item flex em `flex_pseudo.rs`) — as duas passaram a
//! chamar `pseudo_caixa.rs`. Este ficheiro fixa que, com a MESMA declaração
//! `width`/`height`/`padding`/`border`/`margin`/`box-sizing`, os dois papéis
//! respondem com a MESMA caixa outer — o que só é garantido se a aritmética
//! partilhada continuar de facto partilhada, e não uma cópia que apenas
//! parece igual.
//!
//! O CENÁRIO do lado do BLOCO reusa a receita de
//! `pintura_e_caixas_corpus.rs::pseudo_after_display_block_gera_caixa_de_bloco`
//! (`overflow:hidden` no dono estabelece um BFC, então a margem do pseudo não
//! escapa e a altura do dono É a altura outer do pseudo). O lado do FLEX
//! reusa `pseudo_flex_corpus.rs` (o contentor mede a caixa CROSS do maior
//! item, e com um único item — sem filhos reais — essa caixa É a altura outer
//! do pseudo).

use super::*;
use crate::table::tests::{geometria, rect};

/// `border-box`: os 100×40 declarados JÁ são borda+padding+conteúdo; só a
/// margem soma por fora. Outer esperado: `100+2*10=120` × `40+2*10=60`.
const DECLARACAO: &str = "width:100px;height:40px;padding:5px;border:2px solid #000;margin:10px;box-sizing:border-box;background:#f00";

#[test]
fn pseudo_de_bloco_com_caixa_explicita_mede_120x60() {
    let html = format!(
        r#"<style>
          body {{ margin: 0; }}
          #caixa {{ overflow: hidden; width: 300px; }}
          #caixa::after {{ content: "x"; display: block; {DECLARACAO} }}
        </style>
        <div id="caixa"></div>"#
    );
    let (dom, list) = geometria(&html, 1280.0);
    let r = rect(&dom, &list, "#caixa", 0);
    assert_eq!((r.h), 60.0, "outer do ::after de bloco: 40 + 2*10 de margem");
}

#[test]
fn pseudo_de_item_flex_com_a_mesma_caixa_explicita_mede_60_de_altura() {
    // Mesmíssima declaração do teste acima, só a MOLDURA muda (flex em vez de
    // fluxo vertical): sem filhos reais, a altura do contentor É a altura
    // outer do único item — o pseudo.
    let html = format!(
        r#"<style>
          body {{ margin: 0; }}
          #caixa {{ display: flex; align-items: flex-start; width: 300px; }}
          #caixa::after {{ content: "x"; {DECLARACAO} }}
        </style>
        <div id="caixa"></div>"#
    );
    let (dom, list) = geometria(&html, 1280.0);
    let r = rect(&dom, &list, "#caixa", 0);
    assert_eq!(r.h, 60.0, "outer do ::after de item flex: a MESMA conta do caminho de bloco");
}

/// A divergência LEGÍTIMA que a unificação tinha de preservar: sem `width`
/// declarado, um pseudo de bloco ENCHE o content-box do dono (CSS 2.1
/// §10.3.3) e um pseudo-item de flex ENCOLHE ao texto (Flexbox §9.2,
/// shrink-to-fit). As duas respostas têm de continuar DIFERENTES depois da
/// unificação — se ficassem iguais, uma das duas teria perdido o seu papel.
fn larguras_de_fundo(list: &crate::paint::DisplayList, cor: u32) -> Vec<f32> {
    let mut out = Vec::new();
    list.walk(|item, _, _| {
        if let crate::paint::DisplayItem::SolidRect { rect, color, .. } = item {
            if *color == cor {
                out.push(rect.w);
            }
        }
    });
    out
}

#[test]
fn largura_auto_diverge_entre_bloco_preenche_e_item_encolhe() {
    // Mesma pergunta ("largura do RETÂNGULO PINTADO do ::after") pelos dois
    // papéis, sem `width` declarado — é o único par que `pseudo_caixa.rs` não
    // decide sozinho, de propósito (ver o cabeçalho do ficheiro).
    let bloco_html = r#"<style>
      body { margin: 0; font: 16px/20px monospace; }
      #caixa { width: 300px; }
      #caixa::after { content: "x"; display: block; height: 10px; background: #0f0; }
    </style>
    <div id="caixa"></div>"#;
    let (_dom, list) = geometria(bloco_html, 1280.0);
    let fundos = larguras_de_fundo(&list, 0x00FF00FF);
    assert_eq!(fundos.len(), 1, "só o fundo do ::after de bloco é verde");
    // Bloco: ENCHE o content-box do dono (CSS 2.1 §10.3.3) — sem margem/
    // padding/borda declarados, isso é a largura inteira do dono, 300.
    assert_eq!(fundos[0], 300.0, "::after de bloco enche o dono: {}", fundos[0]);

    let item_html = r#"<style>
      body { margin: 0; font: 16px/20px monospace; }
      #caixa { display: flex; width: 300px; }
      #caixa::after { content: "x"; background: #0f0; }
    </style>
    <div id="caixa"></div>"#;
    let (_dom2, list2) = geometria(item_html, 1280.0);
    let fundos2 = larguras_de_fundo(&list2, 0x00FF00FF);
    assert_eq!(fundos2.len(), 1, "só o fundo do ::after de item flex é verde");
    // Item flex: ENCOLHE ao texto ("x" a 16px/monospace ~ 8px), bem abaixo
    // dos 300 do contentor — o oposto do bloco, e é suposto continuar assim.
    assert!(fundos2[0] < 50.0, "::after de item flex encolhe ao texto: {}", fundos2[0]);
}
