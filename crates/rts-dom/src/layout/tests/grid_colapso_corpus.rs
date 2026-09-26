//! `repeat(auto-fit, …)` com trilhas COLAPSADAS e distribuição de conteúdo —
//! a família `css/css-grid/alignment/grid-content-distribution-with-collapsed-tracks-*`
//! do WPT, com o HTML exacto de três dos reftests.
//!
//! A régua está no próprio teste: cada item verde tem de cobrir um quadrado
//! vermelho posto por `position:absolute` no sítio onde o Chrome o põe, por
//! isso o `top`/`left` de cada quadrado É o rect esperado. Até 1cc3b7843 estes
//! testes passavam VAZIOS — os quadrados vermelhos (filhos absolutos de um
//! absoluto) não eram pintados — e a grelha estava errada o tempo todo.

use super::*;

/// Os rects dos itens da `.grid`, relativos à própria grelha.
fn itens_relativos(html: &str) -> Vec<(f32, f32, f32, f32)> {
    let dom = parse_html_to_dom(html);
    let ctx = LayoutCtx {
        viewport_w: 800.0,
        viewport_h: 600.0,
        measurer: &ApproxMeasurer,
    };
    let list = layout_document(&dom, &ctx);
    let geo = list.geometry_now();
    let grelha = dom.resolve(dom.query(".grid").unwrap()).unwrap();
    let g = geo.rects[&grelha];
    dom.node(grelha)
        .children
        .iter()
        .filter(|&&c| matches!(dom.node(c).kind, NodeKind::Element { .. }))
        .map(|c| {
            let r = geo.rects[c];
            (r.x - g.x, r.y - g.y, r.w, r.h)
        })
        .collect()
}

const ESTILO_BLOCO: &str = ".block { position: absolute; z-index: -1; background: green; width: 200px; height: 200px; }";

/// 2×2 visíveis de uma repetição de 8: o espaço livre (150px) reparte-se por
/// TRÊS vãos à volta de duas trilhas, e não por nove à volta de oito.
#[test]
fn space_evenly_reparte_so_pelas_trilhas_que_nao_colapsaram() {
    let html = format!(
        r#"<style>{ESTILO_BLOCO}
.block > div {{ position: absolute; background: red; width: 25px; height: 25px; }}
.grid {{ z-index: 1; display: grid; width: 200px; height: 200px;
  grid-template-columns: repeat(auto-fit, 25px); grid-template-rows: repeat(auto-fit, 25px);
  align-content: space-evenly; justify-content: space-evenly; }}
.grid > div {{ background: green; }}
</style>
<p>Test passes if there is a filled green square and <strong>no red</strong>.</p>
<div class="block">
    <div style="top: 50px; left: 50px;"></div>
    <div style="top: 125px; left: 125px;"></div>
</div>
<div class="grid">
    <div style="grid-row: 2; grid-column: 3;"></div>
    <div style="grid-row: 3; grid-column: 4;"></div>
</div>"#
    );
    assert_eq!(
        itens_relativos(&html),
        vec![(50.0, 50.0, 25.0, 25.0), (125.0, 125.0, 25.0, 25.0)]
    );
}

/// A calha ao lado de uma trilha colapsada colapsa com ela: com `gap:10px` a
/// repetição é de 7 (e não de 8), sobram 2 trilhas e UMA calha entre elas.
#[test]
fn a_calha_ao_lado_de_uma_trilha_colapsada_colapsa_com_ela() {
    let html = format!(
        r#"<style>{ESTILO_BLOCO}
.block > div {{ position: absolute; background: red; width: 20px; height: 20px; }}
.grid {{ z-index: 1; display: grid; width: 200px; height: 200px;
  grid-template-columns: repeat(auto-fit, 20px); grid-template-rows: repeat(auto-fit, 20px);
  grid-row-gap: 10px; grid-column-gap: 10px;
  align-content: space-evenly; justify-content: space-evenly; }}
.grid > div {{ background: green; }}
</style>
<p>Test passes if there is a filled green square and <strong>no red</strong>.</p>
<div class="block">
    <div style="top: 50px; left: 50px;"></div>
    <div style="top: 130px; left: 130px;"></div>
</div>
<div class="grid">
    <div style="grid-row: 1; grid-column: 1;"></div>
    <div style="grid-row: 2; grid-column: 2;"></div>
</div>"#
    );
    assert_eq!(
        itens_relativos(&html),
        vec![(50.0, 50.0, 20.0, 20.0), (130.0, 130.0, 20.0, 20.0)]
    );
}

/// 4×4 visíveis de 10 com calhas de 5px: as colunas 3, 4, 6, 8, 9 e 10
/// colapsam no meio e na ponta, e os itens caem nas quatro posições
/// equidistantes (25, 70, 115, 160).
#[test]
fn trilhas_colapsadas_no_meio_nao_abrem_calha_nem_recebem_espaco() {
    let html = format!(
        r#"<style>{ESTILO_BLOCO}
.block > div {{ position: absolute; background: red; width: 15px; height: 15px; }}
.grid {{ z-index: 1; display: grid; width: 200px; height: 200px;
  grid-template-columns: repeat(auto-fit, 15px); grid-template-rows: repeat(auto-fit, 15px);
  grid-row-gap: 5px; grid-column-gap: 5px;
  align-content: space-evenly; justify-content: space-evenly; }}
.grid > div {{ background: green; }}
</style>
<p>Test passes if there is a filled green square and <strong>no red</strong>.</p>
<div class="block">
    <div style="top: 25px; left: 70px;"></div>
    <div style="top: 25px; left: 115px;"></div>
    <div style="top: 70px; left: 115px;"></div>
    <div style="top: 115px; left: 115px;"></div>
    <div style="top: 160px; left: 25px;"></div>
    <div style="top: 160px; left: 160px;"></div>
</div>
<div class="grid">
    <div class="item" style="grid-row: 1; grid-column: 2;"></div>
    <div class="item" style="grid-row: 1; grid-column: 5;"></div>
    <div class="item" style="grid-row: 3; grid-column: 5;"></div>
    <div class="item" style="grid-row: 4; grid-column: 5;"></div>
    <div class="item" style="grid-row: 6; grid-column: 1;"></div>
    <div class="item" style="grid-row: 6; grid-column: 7;"></div>
</div>"#
    );
    assert_eq!(
        itens_relativos(&html),
        vec![
            (70.0, 25.0, 15.0, 15.0),
            (115.0, 25.0, 15.0, 15.0),
            (115.0, 70.0, 15.0, 15.0),
            (115.0, 115.0, 15.0, 15.0),
            (25.0, 160.0, 15.0, 15.0),
            (160.0, 160.0, 15.0, 15.0),
        ]
    );
}
