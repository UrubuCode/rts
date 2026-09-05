//! O lote `medidor-ahem`: a fonte Ahem (WPT) tem métricas EXATAS (avanço
//! 1em, ascent 0,8em, descent 0,2em) e este motor não tem `@font-face`
//! (`PLAN.md` §5.T) — o que decide é o NOME `font-family: Ahem`, tratado
//! pelo `ApproxMeasurer` como um caso à parte de `is_mono_family`, com a
//! aritmética exata em vez da aproximação calibrada (`MONO_ADVANCE`/
//! `PROP_ADVANCE`). Ver `style::ahem` para a derivação e
//! `layout::medidor_texto` para o `_family` que a liga ao layout.
//!
//! Estes testes fixam que a largura SHRINK-TO-FIT (a que decide o tamanho de
//! um item flex/inline-block sem `width`) usa a Ahem exata, não a
//! aproximação — a mesma medida que `flexbox_flex-natural-mixed-basis-auto`
//! (documentado em `PLAN.md`, lote flex-basis-content-wrap) apontou como o
//! gap real: "a referência usa Ahem (1em por glifo) e o `ApproxMeasurer` não
//! o emula".

use super::*;

/// `font-family: Ahem` sozinho, sem `@font-face`: o WPT declara-a assim em
/// `align-items-baseline-row-horz.html` (`font: 30px/1 Ahem`) — o motor não
/// carrega o `.ttf`, mas já sabe o suficiente do NOME para medir exato.
///
/// 4 caracteres a 50px Ahem = 200px exato (CSS Fonts: 1em por glifo,
/// espaço incluído). O `PROP_ADVANCE` (0.46) desta caixa daria 92px.
#[test]
fn item_flex_com_texto_ahem_encolhe_ao_avanco_exato_de_1em() {
    let list = layout(
        "<div style='display:flex'>\
           <div style='font:50px/1 Ahem;background:#0f0'>XXXX</div>\
         </div>",
        600.0,
    );
    let r = first_rect(&list);
    assert_eq!(r.w, 200.0, "{r:?}");
}

/// A mesma pergunta, `Ahem, monospace` — a Ahem é a PRIMEIRA da lista e
/// decide (o motor não recusa o nome por não ter `@font-face`; ver
/// `style::ahem::is_ahem_family`), então não cai no fallback monospace
/// (que daria 4×50×0.5498=109.96, não 200).
#[test]
fn ahem_primeira_da_lista_nao_cai_no_fallback_monospace() {
    let list = layout(
        "<div style='display:flex'>\
           <div style='font:50px/1 Ahem, monospace;background:#0f0'>XXXX</div>\
         </div>",
        600.0,
    );
    let r = first_rect(&list);
    assert_eq!(r.w, 200.0, "{r:?}");
}

/// Uma família qualquer não sofre nenhuma mudança: continua na aproximação
/// proporcional de sempre — este corpus não é um efeito colateral geral.
/// `PROP_ADVANCE=0.46`: 4 × 50 × 0.46 = 92.
#[test]
fn familia_normal_continua_na_aproximacao_de_sempre() {
    let list = layout(
        "<div style='display:flex'>\
           <div style='font:50px/1 Arial;background:#0f0'>XXXX</div>\
         </div>",
        600.0,
    );
    let r = first_rect(&list);
    assert_eq!(r.w, 92.0, "{r:?}");
}

/// `line-height: normal` (não declarado) na Ahem é 1em exato
/// (ascent 0,8 + descent 0,2) — não o `1.125×size` calibrado contra a fonte
/// padrão do Chrome. Isola a altura de linha de um `<span>` inline com Ahem,
/// sem `line-height` explícito (o `font:Npx/1` dos testes do WPT já fixa
/// isso — este teste é o caso SEM essa declaração, que continuava a usar a
/// aproximação antes deste lote).
#[test]
fn line_height_normal_com_ahem_e_1em_exato() {
    let list = layout(
        "<div style='font-family:Ahem;font-size:50px;background:#0f0'>X</div>",
        600.0,
    );
    let r = first_rect(&list);
    // 50 × (0.8 + 0.2) = 50, contra os 56.25 (ceil(50×1.125)) da aproximação.
    assert_eq!(r.h, 50.0, "{r:?}");
}
