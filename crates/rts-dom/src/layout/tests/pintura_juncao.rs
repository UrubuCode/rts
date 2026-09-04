//! A junção DIAGONAL das bordas (`claude-border-juncao`,
//! `claude-triangulo-de-borda`): com lados adjacentes de cores diferentes cada
//! lado sai como um `DisplayItem::Quad` do canto exterior ao interior; com as
//! cores iguais (ou um lado só) continuam as barras rectangulares. A geometria
//! afirmada é a do Blink: a caixa `#grossa` da fixture (160×100 com bordas
//! 20/30/20/30) e o triângulo (0×0 com três lados a zero).

use crate::table::tests::geometria;
use crate::layout::DisplayItem;

fn quads(html: &str) -> Vec<([(f32, f32); 4], u32)> {
    let (_dom, list) = geometria(html, 1280.0);
    let mut out = Vec::new();
    list.walk(|item, dx, dy| {
        if let DisplayItem::Quad { pts, color } = item {
            out.push((pts.map(|(x, y)| (x + dx, y + dy)), *color));
        }
    });
    out
}

#[test]
fn lados_de_cores_diferentes_sao_trapezios_ate_ao_canto_interior() {
    let q = quads(
        r#"<style>body{margin:0}#g{width:100px;height:60px;border-style:solid;
        border-width:20px 30px 20px 30px;border-color:#c00 #0a0 #00c #fc0}</style><div id="g"></div>"#,
    );
    assert_eq!(q.len(), 4, "quatro lados, quatro trapézios: {q:?}");
    // topo: exterior (0,0)-(160,0), interior (130,20)-(30,20)
    assert_eq!(q[0].0, [(0.0, 0.0), (160.0, 0.0), (130.0, 20.0), (30.0, 20.0)]);
    // esquerda: exterior (0,100)-(0,0), interior (30,20)-(30,80)
    assert_eq!(q[3].0, [(0.0, 100.0), (0.0, 0.0), (30.0, 20.0), (30.0, 80.0)]);
}

#[test]
fn triangulo_de_css_e_um_lado_so_com_os_vizinhos_transparentes() {
    // `border-color: transparent transparent #c00 transparent` — só o fundo
    // pinta, e o trapézio dele é o triângulo inteiro (largura 0, os lados
    // esquerdo/direito de 60px a encontrarem-se no topo).
    let q = quads(
        r#"<style>body{margin:0}#t{width:0;height:0;border-style:solid;
        border-width:0 60px 100px 60px;border-color:transparent transparent #c00 transparent}</style><div id="t"></div>"#,
    );
    assert_eq!(q.len(), 1, "só o lado do fundo pinta: {q:?}");
    assert_eq!(q[0].0, [(120.0, 100.0), (0.0, 100.0), (60.0, 0.0), (60.0, 0.0)]);
}

#[test]
fn cores_iguais_por_lado_continuam_barras() {
    let q = quads(
        r#"<style>body{margin:0}#b{width:100px;height:60px;border-bottom:2px solid #ccc;
        border-top:2px solid #ccc}</style><div id="b"></div>"#,
    );
    assert!(q.is_empty(), "sem cor diferente entre vizinhos não há trapézio: {q:?}");
}
