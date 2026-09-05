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

/// Lote `medidor-ahem` (ronda 2): `quebra::wrap_runs` decide ONDE quebrar
/// pelo avanço EXATO da Ahem, não por `mono`/`PROP_ADVANCE` — o padrão que a
/// maioria dos testes do WPT usa (N carateres numa largura conhecida).
///
/// Duas palavras de 10 "A" a 10px Ahem = 100px cada, exatas. Um contentor de
/// 105px cabe a primeira (100 ≤ 105) mas NÃO cabe a segunda com o espaço
/// (100+10+100=210): a segunda desce de linha. Com `PROP_ADVANCE` (0.46) as
/// DUAS cabiam juntas (10×10×0.46=46 cada, 46+4.6+46≈97 < 105) — a quebra
/// aconteceria no sítio errado, o defeito que este lote fecha.
#[test]
fn wrap_ahem_usa_avanco_exato_para_decidir_onde_quebrar() {
    let list = layout(
        "<p style='width:105px;font:10px/1 Ahem'>AAAAAAAAAA AAAAAAAAAA</p>",
        600.0,
    );
    let t = all_texts(&list);
    assert_eq!(t.len(), 2, "duas palavras, cada uma o seu segmento: {t:?}");
    assert_ne!(t[0].2, t[1].2, "a 2ª palavra desce para a linha seguinte: {t:?}");
}

/// A mesma pergunta com uma família qualquer: as DUAS palavras cabem na
/// MESMA linha (a régua de que o teste acima depende para provar que Ahem
/// muda o resultado, e não é sempre assim).
#[test]
fn wrap_familia_normal_cabe_as_duas_palavras_na_mesma_linha() {
    let list = layout(
        "<p style='width:105px;font:10px/1 Arial'>AAAAAAAAAA AAAAAAAAAA</p>",
        600.0,
    );
    let t = all_texts(&list);
    assert_eq!(t.len(), 1, "uma palavra só — colapsaram no mesmo segmento: {t:?}");
}

/// Lote `medidor-ahem` (fecho pedido pelo coordenador): `width: Nch` num
/// bloco Ahem resolve para `N×font-size` exato — em Ahem `1ch = 1em` POR
/// CONSTRUÇÃO (o "0" que define `ch` é um glifo igual a todos os outros,
/// avança 1em), não `N×font-size×MONO_ADVANCE` (0.5498), a fração calibrada
/// contra uma fonte monoespaçada REAL que os três reftests do WPT
/// (`eol-spaces-bidi-003`, `white-space-pre-wrap-trailing-spaces-012/015`)
/// expunham: o mesmo bug em `Dimension::Ch`, achado ao investigar os 7
/// perdidos de `css-text` da ronda 2 (todos partilhavam `width:Nch`).
#[test]
fn largura_em_ch_num_bloco_ahem_e_exata() {
    let list = layout(
        "<div style='width:4ch;font:20px/1 Ahem;background:#0f0'>XXXX</div>",
        600.0,
    );
    let r = first_rect(&list);
    // 4 × 20 = 80, contra os 43.98 (4×20×0.5498) que MONO_ADVANCE daria.
    assert_eq!(r.w, 80.0, "{r:?}");
}

/// Uma família qualquer não muda: `ch` continua a resolver por
/// `MONO_ADVANCE`, como sempre — este fecho não é um efeito colateral geral.
#[test]
fn largura_em_ch_com_familia_normal_continua_por_mono_advance() {
    let list = layout(
        "<div style='width:4ch;font:20px/1 Arial;background:#0f0'>XXXX</div>",
        600.0,
    );
    let r = first_rect(&list);
    // 4 × 20 × 0.5498 = 43.984.
    assert!((r.w - 43.984).abs() < 0.01, "{r:?}");
}
