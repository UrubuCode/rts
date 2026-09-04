//! `hyphens: manual` contra o Blink — os rects de
//! `tests/css/claude-hyphens-manual.html` medidos no Edge 152 a 1280×800
//! (2026-09-04). A ALTURA é a leitura: `#manual` quebra no `&shy;` (2 linhas,
//! 40px), `#nenhum` ignora-o (1 linha, 20px) e `#semshy` prova que sem hífen
//! suave a palavra não quebra (20px). As larguras de texto vêm do
//! `ApproxMeasurer`, por isso o que se afirma é a altura e o y, não um pixel
//! de largura do Chrome.

use crate::table::tests::{geometria, rect};

const HTML: &str = r#"<style>
  body { margin: 0; font: 16px/20px monospace; }
  div { width: 90px; background: #eee; margin-bottom: 5px; }
  #manual { hyphens: manual; }
  #nenhum { hyphens: none; }
</style>
<div id="manual">abcdefg&shy;hijklmn</div>
<div id="nenhum">abcdefg&shy;hijklmn</div>
<div id="semshy">abcdefghijklmn</div>"#;

#[test]
fn shy_quebra_com_manual_e_nao_com_none() {
    let (dom, list) = geometria(HTML, 1280.0);
    let manual = rect(&dom, &list, "#manual", 0);
    let nenhum = rect(&dom, &list, "#nenhum", 0);
    let semshy = rect(&dom, &list, "#semshy", 0);
    assert_eq!((manual.y, manual.h), (0.0, 40.0), "Blink: duas linhas com `manual`");
    assert_eq!((nenhum.y, nenhum.h), (45.0, 20.0), "Blink: uma linha com `none`");
    assert_eq!((semshy.y, semshy.h), (70.0, 20.0), "Blink: sem &shy; não quebra");
}

/// O hífen suave não pesa: a mesma palavra com e sem `&shy;`, num container
/// largo (nunca quebra), mede a mesma largura — o U+00AD não é medido nem
/// pintado quando a linha não quebra ali.
#[test]
fn shy_nao_ocupa_largura_quando_nao_quebra() {
    let html = |texto: &str| {
        format!(
            r#"<style>body{{margin:0;font:16px/20px monospace}}
            #a{{display:inline-block;white-space:nowrap}}</style><div id="a">{texto}</div>"#
        )
    };
    let (d1, l1) = geometria(&html("abcdefg&shy;hijklmn"), 1280.0);
    let (d2, l2) = geometria(&html("abcdefghijklmn"), 1280.0);
    assert_eq!(rect(&d1, &l1, "#a", 0).w, rect(&d2, &l2, "#a", 0).w);
}
