use super::vertical_align::*;
use crate::layout::{ApproxMeasurer, TextMeasurer};
use crate::style::VerticalAlign;

/// O `font_size` e o strut da fixture que calibrou as constantes:
/// `#linha { font-size:20px }`, `line-height:20px` herdada — mas o
/// STRUT deste módulo só usa `font_ascent`/`font_descent` (18/6.25 a
/// 20px), não o `line-height` declarado, porque é o strut da FONTE que
/// entra no envelope (CSS 2.1 §10.8.1), não a caixa de linha inteira.
const FONT_SIZE: f32 = 20.0;

/// Os sete átomos da fixture, na mesma ordem — usados por vários testes
/// para fechar o MESMO envelope que o Chrome mediu (linha de 50px,
/// baseline a 34.91 do topo).
fn fixture_atoms() -> [(f32, VerticalAlign); 7] {
    [
        (20.0, VerticalAlign::Baseline),
        (30.0, VerticalAlign::Top),
        (40.0, VerticalAlign::Middle),
        (50.0, VerticalAlign::Bottom),
        (25.0, VerticalAlign::TextTop),
        (20.0, VerticalAlign::Super),
        (20.0, VerticalAlign::Sub),
    ]
}

/// O envelope fecha em 50px de altura com a baseline a 34.91 do topo —
/// os dois números que `claude-vertical-align.esperado.json` mede
/// indiretamente (via `#base.y` e via `#fundo.y+#fundo.h`).
#[test]
fn envelope_da_fixture_fecha_em_50px_com_a_baseline_do_chrome() {
    let env = envelope(&fixture_atoms(), FONT_SIZE, &ApproxMeasurer);
    assert!((env.above - 34.91).abs() < 0.01, "acima={}", env.above);
    assert!((env.below - 15.09).abs() < 0.01, "abaixo={}", env.below);
    assert!(
        (env.height() - 50.0).abs() < 0.01,
        "altura={}",
        env.height()
    );
}

/// `baseline`: o fundo da caixa fica NA baseline (sem baseline própria,
/// CSS 2.1 §10.8.1) — `#base`, h=20, `y` esperado 14.91.
#[test]
fn baseline_poe_o_fundo_da_caixa_na_baseline() {
    let env = envelope(&fixture_atoms(), FONT_SIZE, &ApproxMeasurer);
    let y = item_top(
        VerticalAlign::Baseline,
        20.0,
        0.0,
        &env,
        FONT_SIZE,
        &ApproxMeasurer,
    );
    assert!((y - 14.91).abs() < 0.01, "y={y}");
}

/// `top`: alinha com o TOPO da linha — sempre `line_y`, independente do
/// envelope. `#topo`, h=30, `y` esperado 0.
#[test]
fn top_alinha_com_o_topo_da_linha() {
    let env = envelope(&fixture_atoms(), FONT_SIZE, &ApproxMeasurer);
    let y = item_top(
        VerticalAlign::Top,
        30.0,
        0.0,
        &env,
        FONT_SIZE,
        &ApproxMeasurer,
    );
    assert!((y - 0.0).abs() < 0.01, "y={y}");
}

/// `middle`: o CENTRO da caixa fica meio x-height acima da baseline.
/// `#meio`, h=40, `y` esperado 10.
#[test]
fn middle_centra_meio_x_height_acima_da_baseline() {
    let env = envelope(&fixture_atoms(), FONT_SIZE, &ApproxMeasurer);
    let y = item_top(
        VerticalAlign::Middle,
        40.0,
        0.0,
        &env,
        FONT_SIZE,
        &ApproxMeasurer,
    );
    assert!((y - 10.0).abs() < 0.01, "y={y}");
}

/// `bottom`: alinha com o FUNDO da linha — `linha_y + altura(env) -
/// altura_do_item`. `#fundo`, h=50, `y` esperado 0 (a linha inteira mede
/// 50, então o fundo do item COINCIDE com o topo da linha).
#[test]
fn bottom_alinha_com_o_fundo_da_linha() {
    let env = envelope(&fixture_atoms(), FONT_SIZE, &ApproxMeasurer);
    let y = item_top(
        VerticalAlign::Bottom,
        50.0,
        0.0,
        &env,
        FONT_SIZE,
        &ApproxMeasurer,
    );
    assert!((y - 0.0).abs() < 0.01, "y={y}");
}

/// `text-top`: o topo da caixa alinha com o topo da FONTE (o ascent do
/// strut) — não com o topo da linha. `#texto-topo`, h=25, `y` esperado
/// 16.91.
#[test]
fn text_top_alinha_com_o_ascent_do_strut() {
    let env = envelope(&fixture_atoms(), FONT_SIZE, &ApproxMeasurer);
    let y = item_top(
        VerticalAlign::TextTop,
        25.0,
        0.0,
        &env,
        FONT_SIZE,
        &ApproxMeasurer,
    );
    assert!((y - 16.91).abs() < 0.01, "y={y}");
}

/// `super`: sobe a caixa `SUPER_OFFSET_RATIO × font-size` acima de onde
/// `baseline` a poria. `#super`, h=20, `y` esperado 7.25.
#[test]
fn super_sobe_a_caixa_acima_da_posicao_de_baseline() {
    let env = envelope(&fixture_atoms(), FONT_SIZE, &ApproxMeasurer);
    let y = item_top(
        VerticalAlign::Super,
        20.0,
        0.0,
        &env,
        FONT_SIZE,
        &ApproxMeasurer,
    );
    assert!((y - 7.25).abs() < 0.01, "y={y}");
}

/// `sub`: desce a caixa `SUB_OFFSET_RATIO × font-size` abaixo de onde
/// `baseline` a poria. `#sub`, h=20, `y` esperado 19.91.
#[test]
fn sub_desce_a_caixa_abaixo_da_posicao_de_baseline() {
    let env = envelope(&fixture_atoms(), FONT_SIZE, &ApproxMeasurer);
    let y = item_top(
        VerticalAlign::Sub,
        20.0,
        0.0,
        &env,
        FONT_SIZE,
        &ApproxMeasurer,
    );
    assert!((y - 19.91).abs() < 0.01, "y={y}");
}

/// `text-bottom`: o fundo da caixa alinha com o fundo da FONTE (o descent
/// do strut) — o seu topo fica `height − descent` acima da baseline. Sem
/// fixture medida no Chrome para este valor, o teste isola UM átomo: o
/// envelope fecha exatamente na contribuição dele (`acima = altura −
/// descent`, maior que o ascent do strut sozinho a 20px), então a
/// baseline cai bem no ponto que a fórmula prevê e `y = 0`.
#[test]
fn text_bottom_alinha_com_o_descent_do_strut() {
    let env = envelope(
        &[(30.0, VerticalAlign::TextBottom)],
        FONT_SIZE,
        &ApproxMeasurer,
    );
    let y = item_top(
        VerticalAlign::TextBottom,
        30.0,
        0.0,
        &env,
        FONT_SIZE,
        &ApproxMeasurer,
    );
    assert!((y - 0.0).abs() < 0.01, "y={y}");
    assert!((env.above - 26.0).abs() < 0.01, "acima={}", env.above);
}

/// Um envelope de UM SÓ átomo nunca fica menor do que o strut sozinho —
/// uma linha vazia (só texto, sem inline-block nenhum) ainda tem a altura
/// da fonte do bloco.
#[test]
fn envelope_vazio_e_o_strut_sozinho() {
    let env = envelope(&[], FONT_SIZE, &ApproxMeasurer);
    assert!((env.above - ApproxMeasurer.font_ascent(FONT_SIZE)).abs() < 0.01);
    assert!((env.below - ApproxMeasurer.font_descent(FONT_SIZE)).abs() < 0.01);
}

/// Lote `medidor-ahem` (ronda 2): `text-top`/`text-bottom` com o STRUT em
/// Ahem usam a fração EXATA da fonte (0.8/0.2), não `ASCENT_RATIO`/
/// `DESCENT_RATIO` (0.90/0.3125, calibradas contra o Chrome real).
#[test]
fn ascent_com_baseline_propria_usa_fracao_exata_da_ahem() {
    let above = ascent_with_own_baseline(
        VerticalAlign::TextTop,
        100.0,
        0.0,
        FONT_SIZE,
        Some("Ahem"),
        &ApproxMeasurer,
    );
    assert!((above - FONT_SIZE * 0.8).abs() < 0.01, "acima={above}");
    let below_top = ascent_with_own_baseline(
        VerticalAlign::TextBottom,
        100.0,
        0.0,
        FONT_SIZE,
        Some("Ahem"),
        &ApproxMeasurer,
    );
    assert!(
        (below_top - (100.0 - FONT_SIZE * 0.2)).abs() < 0.01,
        "{below_top}"
    );
}

/// `text-top` aligns with the ascent of the STRUT's own family — the
/// number comes from the font, not from a constant of this module.
#[test]
fn text_top_uses_the_struts_family_ascent() {
    for family in [None, Some("Arial"), Some("monospace")] {
        let got = ascent_with_own_baseline(
            VerticalAlign::TextTop,
            100.0,
            0.0,
            FONT_SIZE,
            family,
            &ApproxMeasurer,
        );
        assert_eq!(
            got,
            ApproxMeasurer.font_ascent_family(FONT_SIZE, family),
            "{family:?}"
        );
    }
}

/// O STRUT do envelope (linha só de texto Ahem, sem inline-block) fecha
/// com `font_ascent`/`font_descent` exatos — 0.8/0.2 — em vez da
/// calibração do Chrome. `line_height` (o `content` que decide a
/// meia-entrelinha) também responde 1em exato.
#[test]
fn envelope_com_baseline_do_strut_ahem_e_exato() {
    let env = envelope_with_baseline(&[], FONT_SIZE, FONT_SIZE, Some("Ahem"), &ApproxMeasurer);
    assert!(
        (env.above - FONT_SIZE * 0.8).abs() < 0.01,
        "acima={}",
        env.above
    );
    assert!(
        (env.below - FONT_SIZE * 0.2).abs() < 0.01,
        "abaixo={}",
        env.below
    );
}
