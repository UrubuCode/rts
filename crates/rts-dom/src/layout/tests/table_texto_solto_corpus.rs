//! `tests/css/claude-table-texto-solto-sem-celula.html` contra o Blink (Edge
//! 152, 2026-09-04): uma `display:table` cujo único filho é texto solto mede a
//! mesma linha (20px, o `line-height` do `body`) que o bloco irmão — o texto
//! entra numa célula anónima e herda a fonte do pai.

use crate::table::tests::{geometria, rect};

const HTML: &str = r#"<style>
  body { margin: 0; font: 16px/20px monospace; }
  div { width: 120px; background: #0c0; }
  #tbl { display: table; }
</style>
<div id="tbl">abc</div>
<div id="controlo">abc</div>"#;

#[test]
fn texto_solto_numa_table_e_uma_celula_anonima_com_a_fonte_do_pai() {
    let (dom, list) = geometria(HTML, 1280.0);
    let r = |s: &str| { let r = rect(&dom, &list, s, 0); (r.x, r.y, r.w, r.h) };
    assert_eq!(r("#tbl"), (0.0, 0.0, 120.0, 20.0), "a linha de texto da tabela");
    assert_eq!(r("#controlo"), (0.0, 20.0, 120.0, 20.0), "o bloco irmão fica por baixo");
}
