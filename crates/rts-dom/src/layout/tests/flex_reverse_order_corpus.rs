//! Lote `flex-reverse-order`: WPT `flexbox_rtl-order` (`coluna_wrap.rs`, a
//! ORDEM DAS COLUNAS sob `direction:rtl`) e WPT
//! `css-flexbox-img-expand-evenly` (`flex.rs`, `align-items:stretch` a
//! ENCOLHER um item cuja altura pré-stretch já veio maior do que a linha).

use crate::table::tests::{geometria, geometria_com, rect};

/// `direction:rtl` inverte a ORDEM DAS COLUNAS de um `flex-direction:column`
/// com `flex-wrap`, do mesmo jeito que `wrap-reverse` — as duas trocam o par
/// cross-start/cross-end (Flexbox §4.1 + Writing Modes) e por isso CANCELAM
/// quando as duas estão presentes (`XOR`, ver `coluna_wrap.rs`). Achado pelo
/// WPT `flexbox_rtl-order` (referência hand-authored, floats reordenados):
/// sem o XOR, `direction:rtl` sozinho não tinha efeito nenhum na ordem física
/// das colunas — só no alinhamento de um item DENTRO da sua própria coluna
/// (`coluna_rtl::cross_x`, já correcto antes deste lote).
#[test]
fn direction_rtl_sozinho_inverte_a_ordem_das_colunas_como_wrap_reverse() {
    // Mesmo agrupamento de `coluna_quebra_em_duas_colunas_quando_a_altura_nao_chega`
    // (`flex_column_wrap_corpus.rs`): col A = [i1,i2] (2×80=160 cabe em 160),
    // col B = [i3,i4]. SEM `column-reverse`, a posição DENTRO de cada coluna
    // fica na ordem do documento (i1 no topo de A, i3 no topo de B).
    const HTML: &str = r#"<style>
  body { margin: 0; font: 16px/20px monospace; }
  .c { display: flex; flex-direction: column; direction: rtl; flex-wrap: wrap; height: 160px; }
  .c > div { width: 100px; height: 80px; }
</style>
<body>
<div class="c"><div id="i1"></div><div id="i2"></div><div id="i3"></div><div id="i4"></div></div>
</body></html>"#;
    let (dom, list) = geometria(HTML, 1280.0);
    let r = |s: &str| { let r = rect(&dom, &list, s, 0); (r.x, r.y, r.w, r.h) };
    // `align-content:normal` estica as duas colunas para os 1280px do body
    // (640 cada) — mesma conta da fixture irmã. RTL sozinho (sem
    // `wrap-reverse`) põe a coluna B (a 2ª calculada) em PRIMEIRO no eixo
    // cruzado: B começa em x=0, A em x=640 — o oposto do que sairia sem o
    // XOR. Cada item, dentro da sua própria coluna de 640, continua a
    // espelhar-se para a borda DIREITA da coluna via `coluna_rtl::cross_x`
    // (mecanismo já existente antes deste lote, independente do XOR): 640
    // menos os 100 do item = 540 dentro da coluna.
    assert_eq!(r("#i3"), (540.0, 0.0, 100.0, 80.0), "coluna B (2a calculada) e a 1a fisicamente, RTL sozinho");
    assert_eq!(r("#i4"), (540.0, 80.0, 100.0, 80.0));
    assert_eq!(r("#i1"), (1180.0, 0.0, 100.0, 80.0), "coluna A (1a calculada) e a 2a fisicamente, RTL sozinho");
    assert_eq!(r("#i2"), (1180.0, 80.0, 100.0, 80.0));
}

/// `direction:rtl` MAIS `flex-wrap:wrap-reverse` juntos cancelam-se (XOR
/// falso): a ordem das colunas volta a ser a mesma do caso LTR sem
/// `wrap-reverse` nenhum — confirmado item a item contra a referência real
/// do WPT `flexbox_rtl-order` (`flex-flow: column-reverse wrap-reverse;
/// direction: rtl`), que é o caso `column-reverse` deste teste.
#[test]
fn direction_rtl_com_wrap_reverse_juntos_cancelam_e_nao_invertem_a_ordem() {
    const HTML: &str = r#"<style>
  body { margin: 0; font: 16px/20px monospace; }
  .c { display: flex; flex-direction: column-reverse; direction: rtl; flex-wrap: wrap-reverse; height: 160px; }
  .c > div { width: 100px; height: 80px; }
</style>
<body>
<div class="c"><div id="i1"></div><div id="i2"></div><div id="i3"></div><div id="i4"></div></div>
</body></html>"#;
    let (dom, list) = geometria(HTML, 1280.0);
    let r = |s: &str| { let r = rect(&dom, &list, s, 0); (r.x, r.y, r.w, r.h) };
    // A ORDEM FÍSICA das colunas é a mesma da fixture SEM `direction:rtl`
    // (`coluna_reversa_e_wrap_reverso_juntos_derivado_da_spec_nao_medido`,
    // `flex_column_wrap_corpus.rs`): o XOR devolve `false` quando as duas
    // trocas estão presentes, então `direction:rtl` não move a coluna A/B —
    // só `column-reverse`/`wrap-reverse` (já correctos antes deste lote)
    // decidem qual coluna fica em que posição e a ordem dentro dela. O que
    // MUDA é o espelho de CADA item dentro da sua coluna (`coluna_rtl::
    // cross_x`, sempre activo com RTL, independente do XOR): 640-100=540
    // dentro da coluna A (x=0..640) e 1180 dentro da B (x=640..1280).
    assert_eq!(r("#i4"), (1180.0, 0.0, 100.0, 80.0), "coluna B, invertida por column-reverse: i4 no topo");
    assert_eq!(r("#i3"), (1180.0, 80.0, 100.0, 80.0), "coluna B, invertida: i3 no fundo");
    assert_eq!(r("#i2"), (540.0, 0.0, 100.0, 80.0), "coluna A, invertida: i2 no topo");
    assert_eq!(r("#i1"), (540.0, 80.0, 100.0, 80.0), "coluna A, invertida: i1 no fundo");
}

/// `align-items:stretch` (o omisso) tem de ENCOLHER um item cuja altura
/// pré-stretch já veio maior do que a linha, não só crescer um menor — o
/// mesmo passo 7 do Flexbox §9.4 nas duas direcções. Achado pelo WPT
/// `css-flexbox-img-expand-evenly`: um `<img width>` sem `height`, razão de
/// aspecto 1:1, ficava com a altura NATURAL (=`width`) em vez de esticar aos
/// 20px da linha, porque `stretches` só disparava com `line_h > it.h`.
#[test]
fn stretch_encolhe_um_item_cuja_altura_pela_razao_de_aspecto_excede_a_linha() {
    let html = r#"<style>
  .f { display: flex; width: 200px; height: 20px; }
  .f > img { width: 48px; }
</style>
<div class="f"><img id="alvo"></div>"#;
    let (dom, list) = geometria_com(html, 1280.0, |d| {
        let id = d.query("#alvo").unwrap();
        // PNG 1:1 (o mesmo padrão de `imagens_corpus.rs`): sem `height`
        // declarado, a razão natural dava 48×48 (a MESMA altura da
        // `width`) antes do stretch decidir.
        d.set_pixel_data(id, vec![0, 0, 200, 255].repeat(16 * 16), 16, 16);
    });
    let r = rect(&dom, &list, "#alvo", 0);
    assert_eq!((r.w, r.h), (48.0, 20.0), "estica aos 20px da linha, não fica em 48 (a razão de aspecto 1:1 da largura declarada)");
}
