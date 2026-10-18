//! Lote `flex-aspect-ratio-collapse` (tema aspect-ratio/tamanho intrínseco de
//! substituídos): dois reftests do WPT batidos à mão contra a spec, ambos
//! confirmados com `claude-raster`/`claude-paint-dump` nos ficheiros REAIS do
//! WPT antes deste corpus nascer (`scripts/wpt_reftests.md` tem o comando).
//!
//! `visibility:collapse` (horiz-001/002/003, wrap) fica DE FORA deste lote —
//! medido contra os 3 corpus `tests/css/claude-*visibility-collapse*`
//! (Chrome/Edge 152 real, 2026-09-04): um item colapsado mantém a largura
//! CHEIA no eixo principal, os DOIS gaps e o `flex-grow`, sem strut nenhum —
//! o oposto do que os reftests do WPT (e a spec) pedem. Implementar o strut
//! passaria esses 5 reftests de auto-consistência mas REGREDIRIA os 3
//! corpus medidos contra o Chrome real, que esta régua não pode fazer
//! silenciosamente (CLAUDE.md, "régua é o Chrome"). Mesma razão pela qual
//! `gap-collapse` já tinha sido rejeitado por outro lote.

use crate::table::tests::{geometria_com, rect};

/// `flex-svg-no-intrinsic-column-001` (WPT): um `<img>` com `src` mas sem
/// NENHUMA dimensão decodificável (aqui, um `data:image/svg+xml` sem
/// `width`/`height`/`viewBox` — `svg_data_url_dims` devolve `None`) cai no
/// default de CSS Images §5 (300×150), e não em `(0,0)` como antes — mas só
/// quando HÁ `src`: `imagem_sem_dimensao_nenhuma_continua_sem_caixa`
/// (`inline_box/tests/imagem.rs`) fixa que um `<img>` SEM `src` nenhum
/// continua em `(0,0)`, e é o par que prova a distinção.
///
/// `align-items: stretch` (default) num flex-COLUMN estica a largura do item
/// à do contentor (150) — a largura do item vem do STRETCH, a altura do
/// default (150, sem razão para derivar de outra coisa).
#[test]
fn img_com_src_sem_ratio_cai_no_default_300x150_e_a_largura_estica_na_coluna() {
    let html = "<div style='display:flex;flex-direction:column;width:150px'>\
                   <img src=\"data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg'/>\">\
                 </div>";
    let (dom, list) = geometria_com(html, 1280.0, |_d| {});
    let r = rect(&dom, &list, "img", 0);
    assert_eq!(
        (r.w, r.h),
        (150.0, 150.0),
        "largura esticada pela coluna; altura = default 300×150, sem razão a preservar"
    );
}

/// `flex-aspect-ratio-intrinsic-padding-001` (WPT): um `<img>` com
/// `padding:20px` e razão 2:1 (lida do `data:image/svg+xml` com `width`/
/// `height`, sem descodificar nada — `svg_data_url_dims`), esticado a 240 de
/// largura por uma coluna flex, deve derivar a altura do CONTENT-BOX
/// (240-40=200 de largura de conteúdo → 100 de altura pela razão) e só
/// DEPOIS somar o padding (240×140) — não do border-box (o que daria uma
/// altura errada, 240×0,5=120).
#[test]
fn img_com_padding_deriva_a_razao_do_content_box_nao_do_border_box() {
    let html = "<div style='display:flex;flex-direction:column;width:240px'>\
                   <img style='padding:20px' src=\"data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' width='200' height='100'><rect fill='green' width='200' height='100'/></svg>\">\
                 </div>";
    let (dom, list) = geometria_com(html, 1280.0, |_d| {});
    let r = rect(&dom, &list, "img", 0);
    assert_eq!(
        (r.w, r.h),
        (240.0, 140.0),
        "200×100 de conteúdo (razão 2:1 sobre a largura de conteúdo 200) + 20px de padding por lado"
    );
}

/// `aspect-ratio` with a DEFINITE height derives the width in a shrink-to-fit
/// box (inline-block/float/abspos): CSS Sizing 4 §5 transfers the known height
/// through the ratio. Before the fix the shrink-to-fit width came from the
/// (empty) content and collapsed to 0 (WPT `css-sizing/block-aspect-ratio-036`:
/// `display:inline-block; height:100px; aspect-ratio:0.7` → 70 wide).
#[test]
fn aspect_ratio_derives_width_from_definite_height_in_shrink_to_fit() {
    use crate::table::tests::geometria;
    let ib = r#"<body style="margin:0"><div id="t" style="display:inline-block;height:100px;aspect-ratio:0.7"></div></body>"#;
    let (dom, list) = geometria(ib, 800.0);
    let r = rect(&dom, &list, "#t", 0);
    assert_eq!((r.w, r.h), (70.0, 100.0), "inline-block height×ratio");
    let flt = r#"<body style="margin:0"><div id="t" style="float:left;height:100px;aspect-ratio:1/2"></div></body>"#;
    let (dom, list) = geometria(flt, 800.0);
    assert_eq!(rect(&dom, &list, "#t", 0).w, 50.0, "float height×ratio");
    // A plain block keeps width:auto = stretch (aspect-ratio does not override).
    let blk = r#"<body style="margin:0"><div id="t" style="height:100px;aspect-ratio:0.7"></div></body>"#;
    let (dom, list) = geometria(blk, 800.0);
    assert_eq!(rect(&dom, &list, "#t", 0).w, 800.0, "bloco: width:auto continua stretch");
}

/// An out-of-flow box with BOTH insets of one axis set and that size `auto` is
/// stretched between the insets even when an `aspect-ratio` is present — the
/// ratio then sizes the OTHER (auto) axis from it (CSS 2.1 §10.6.4 + Sizing 4
/// §5.1). `top:0;bottom:0;left:0;aspect-ratio:2/1` in a 300×50 block is 50 tall
/// (the inset height) and 100 wide (height×ratio); before, a ratio guard on the
/// height's stretch left it 0 tall / 0 wide (WPT `css-sizing/aspect-ratio/
/// abspos-004`).
#[test]
fn abspos_stretches_a_both_inset_axis_then_the_ratio_sizes_the_other() {
    use crate::table::tests::geometria;
    let html = r#"<body style="margin:0"><div style="width:300px;height:50px;position:relative">
      <div id="t" style="aspect-ratio:2/1;position:absolute;left:0;top:0;bottom:0"></div></div></body>"#;
    let (dom, list) = geometria(html, 800.0);
    assert_eq!(
        { let r = rect(&dom, &list, "#t", 0); (r.w, r.h) },
        (100.0, 50.0),
        "altura dos insets, largura pela razão"
    );
    // With a DEFINITE width, the ratio derives the height from it (CSS Sizing 4
    // §5.1) and wins over the inset stretch — `width:80;aspect-ratio:2/1` is 40
    // tall, not the 50 between `top:0`/`bottom:0` (the same rule as WPT
    // `abspos-005`, which this keeps green).
    let w = r#"<body style="margin:0"><div style="width:300px;height:50px;position:relative">
      <div id="t" style="width:80px;aspect-ratio:2/1;position:absolute;left:0;top:0;bottom:0"></div></div></body>"#;
    let (dom, list) = geometria(w, 800.0);
    assert_eq!({ let r = rect(&dom, &list, "#t", 0); (r.w, r.h) }, (80.0, 40.0));
}

/// An out-of-flow box with `aspect-ratio`, auto height and a height NOT
/// stretched between insets takes the height from its (shrink-to-fit) width
/// through the ratio (CSS Sizing 4 §5.1): abspos `aspect-ratio:1/1` around a
/// `width:100` child is 100 wide AND 100 tall, where the general shrink-to-fit
/// path leaves it 0 tall (WPT `css-sizing/aspect-ratio/abspos-007`).
#[test]
fn abspos_aspect_ratio_transfers_width_to_height() {
    use crate::table::tests::geometria;
    let html = r#"<body style="margin:0"><div style="position:relative;width:200px;height:200px">
      <div id="t" style="position:absolute;aspect-ratio:1/1"><div style="width:100px"></div></div></div></body>"#;
    let (dom, list) = geometria(html, 800.0);
    assert_eq!({ let r = rect(&dom, &list, "#t", 0); (r.w, r.h) }, (100.0, 100.0));
}

/// An out-of-flow box with a DEFINITE width and `aspect-ratio` gets its height
/// from the ratio, but the content's min-content height FLOORS it (the abspos
/// aspect-ratio minimum, CSS Sizing 4 §5.1): `width:100;aspect-ratio:2/1` with
/// a 100px-tall child is 100 tall, not the ratio's 50 (WPT
/// `css-sizing/aspect-ratio/abspos-012`). With no tall content the ratio wins.
#[test]
fn abspos_aspect_ratio_height_is_floored_by_content() {
    use crate::table::tests::geometria;
    let floored = r#"<body style="margin:0"><div style="position:relative">
      <div id="t" style="width:100px;aspect-ratio:2/1;position:absolute"><div style="height:100px"></div></div></div></body>"#;
    let (dom, list) = geometria(floored, 800.0);
    assert_eq!({ let r = rect(&dom, &list, "#t", 0); (r.w, r.h) }, (100.0, 100.0));
    let empty = r#"<body style="margin:0"><div style="position:relative">
      <div id="t" style="width:100px;aspect-ratio:2/1;position:absolute"></div></div></body>"#;
    let (dom, list) = geometria(empty, 800.0);
    assert_eq!({ let r = rect(&dom, &list, "#t", 0); (r.w, r.h) }, (100.0, 50.0));
}
