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
    assert_eq!((r.w, r.h), (150.0, 150.0), "largura esticada pela coluna; altura = default 300×150, sem razão a preservar");
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
    assert_eq!((r.w, r.h), (240.0, 140.0), "200×100 de conteúdo (razão 2:1 sobre a largura de conteúdo 200) + 20px de padding por lado");
}
