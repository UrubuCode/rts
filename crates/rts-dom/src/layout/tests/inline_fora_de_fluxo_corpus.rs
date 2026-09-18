//! As três fixtures `tests/css/claude-{float,absoluto}-*-inline*.html` contra
//! o Edge/Blink (`.esperado.json`, medido 2026-09-18): CSS 2.1 §9.2.1.1 parte
//! um inline à volta de uma caixa de bloco EM FLUXO, e um float ou uma caixa
//! absoluta não estão em fluxo (§9.3). Partir o inline à volta deles abre
//! caixas anónimas que o Blink não tem — o texto de depois desce uma linha.

use super::*;
use crate::table::tests::geometria;

/// Tolerância do corpus (`tests/css/README.md`): 1px.
const TOL: f32 = 1.0;

/// O rect que a ponte devolve (`boundingRect`) e o que o `.esperado.json`
/// mede — `rect_of`, e não a tabela de hit-test, que deixa de fora os blocos
/// que partiram um inline (`layout/rect_cliente.rs`).
fn rect(dom: &crate::Dom, list: &crate::layout::DisplayList, sel: &str, _n: usize) -> Rect {
    let idx = dom.resolve(dom.query(sel).expect(sel)).expect("nó vivo");
    list.rect_of(idx).unwrap_or_else(|| panic!("{sel} sem geometria"))
}

fn afirma(dom: &crate::Dom, list: &crate::layout::DisplayList, sel: &str, esperado: (f32, f32, f32, f32)) {
    let r = rect(dom, list, sel, 0);
    let got = (r.x, r.y, r.w, r.h);
    let bate = (got.0 - esperado.0).abs() <= TOL
        && (got.1 - esperado.1).abs() <= TOL
        && (got.2 - esperado.2).abs() <= TOL
        && (got.3 - esperado.3).abs() <= TOL;
    assert!(bate, "{sel}: esperado {esperado:?} (Blink), obtido {got:?}");
}

/// O `y`/`h` do span — o que diz se ficou numa linha só. O `x`/`w` dependem da
/// largura do texto monoespaçado, que este motor aproxima.
fn afirma_linha(dom: &crate::Dom, list: &crate::layout::DisplayList, sel: &str, x: f32, y: f32, h: f32) {
    let r = rect(dom, list, sel, 0);
    assert!(
        (r.x - x).abs() <= TOL && (r.y - y).abs() <= TOL && (r.h - h).abs() <= TOL,
        "{sel}: esperado x={x} y={y} h={h} (Blink), obtido x={} y={} h={}",
        r.x,
        r.y,
        r.h
    );
}

fn html(estilo_do_filho: &str, id: &str, meio: &str) -> String {
    format!(
        r#"<!DOCTYPE html><html><head><style>
  body {{ margin: 0; font: 16px/20px monospace; }}
  #quebra {{ background: #ff0; }}
  #{id} {{ {estilo_do_filho} }}
  #bloco {{ height: 25px; background: #f0f; }}
  #seguinte {{ background: #00f; height: 10px; }}
</style></head><body>
  <div id="contentor"><span id="quebra">antes<div id="{id}"></div>{meio}</span></div>
  <div id="seguinte"></div>
</body></html>"#
    )
}

#[test]
fn float_dentro_do_inline_nao_o_parte() {
    let src = html("float: left; width: 50px; height: 30px; background: #0f0;", "flutua", "depois");
    let (dom, list) = geometria(&src, 1280.0);
    afirma(&dom, &list, "#contentor", (0.0, 0.0, 1280.0, 20.0));
    afirma(&dom, &list, "#flutua", (0.0, 0.0, 50.0, 30.0));
    // Uma linha só, encurtada à volta do float: começa em x=50.
    afirma_linha(&dom, &list, "#quebra", 50.0, 0.0, 19.0);
    afirma(&dom, &list, "#seguinte", (0.0, 20.0, 1280.0, 10.0));
}

#[test]
fn absoluto_dentro_do_inline_nao_o_parte() {
    let src = html("position: absolute; width: 40px; height: 30px; background: #0f0;", "fora", "depois");
    let (dom, list) = geometria(&src, 1280.0);
    afirma(&dom, &list, "#contentor", (0.0, 0.0, 1280.0, 20.0));
    afirma_linha(&dom, &list, "#quebra", 0.0, 0.0, 19.0);
    // `#fora` NÃO é afirmado, e a fixture está em `esperado-a-falhar.txt`: o
    // Blink dá-lhe `(0,20)` — a posição estática de um absoluto que era de
    // BLOCO é a seguir à caixa de linha — e este motor dá `y≈1`, porque
    // `posicao_estatica_bloco` toma o `<span>` do DOM como contentor e não há
    // caixa de linha guardada que diga onde a linha acaba (lote IFC).
    afirma(&dom, &list, "#seguinte", (0.0, 20.0, 1280.0, 10.0));
}

#[test]
fn so_o_bloco_em_fluxo_parte_o_inline_e_o_float_fica_na_primeira_corrida() {
    let src = html(
        "float: left; width: 50px; height: 30px; background: #0f0;",
        "flutua",
        r#"meio<div id="bloco"></div>depois"#,
    );
    let (dom, list) = geometria(&src, 1280.0);
    afirma(&dom, &list, "#contentor", (0.0, 0.0, 1280.0, 65.0));
    afirma(&dom, &list, "#quebra", (0.0, 0.0, 1280.0, 64.0));
    afirma(&dom, &list, "#flutua", (0.0, 0.0, 50.0, 30.0));
    afirma(&dom, &list, "#bloco", (0.0, 20.0, 1280.0, 25.0));
    afirma(&dom, &list, "#seguinte", (0.0, 65.0, 1280.0, 10.0));
}


/// `tests/css/claude-float-a-meio-da-linha.html`: o float filho DIRECTO a
/// meio de texto, à esquerda e à direita, e o que não cabe no resto da linha
/// e desce para o topo da seguinte (CSS 2.1 §9.5.1).
#[test]
fn float_a_meio_da_linha_fica_no_topo_dela_ou_desce_se_nao_cabe() {
    let src = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/css/claude-float-a-meio-da-linha.html"
    ))
    .expect("fixture");
    let (dom, list) = geometria(&src, 1280.0);
    afirma(&dom, &list, "#a", (0.0, 0.0, 1280.0, 20.0));
    afirma(&dom, &list, "#fa", (0.0, 0.0, 50.0, 30.0));
    afirma(&dom, &list, "#b", (0.0, 30.0, 1280.0, 20.0));
    afirma(&dom, &list, "#fb", (1220.0, 30.0, 60.0, 10.0));
    afirma(&dom, &list, "#c", (0.0, 60.0, 120.0, 40.0));
    afirma(&dom, &list, "#fc", (0.0, 80.0, 80.0, 10.0));
    afirma(&dom, &list, "#fim", (0.0, 110.0, 1280.0, 10.0));
}
