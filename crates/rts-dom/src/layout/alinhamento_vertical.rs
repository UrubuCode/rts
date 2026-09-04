//! ALINHAMENTO VERTICAL: o modelo de baseline que `vertical-align` lê.
//!
//! Até aqui o motor só sabia posicionar dois dos oito valores CSS
//! (`middle`/`bottom`), e só dentro da "corrida" de inline-blocks
//! (`layout_inline_block_line`, em `linha_ib.rs`) — o resto caía num `_ => 0.0`
//! que alinhava pelo TOPO. A causa não era um valor por implementar de cada
//! vez: era a falta de uma ENTIDADE. Cada átomo de uma linha (um `inline-block`
//! vazio, hoje; um run de texto, amanhã) só tem ALTURA — não tem onde guardar
//! "a que distância da baseline fica o meu topo", e sem esse número
//! `baseline`/`sub`/`super`/`text-top`/`text-bottom` não têm o que ler.
//!
//! Este módulo dá nome a essa entidade: [`Envelope`] é a distância da baseline
//! da linha ao seu topo e ao seu fundo — o "acima"/"abaixo" do CSS 2.1
//! §10.8.1 — calculado a partir do STRUT (a fonte do bloco que contém a linha)
//! e de cada átomo baseline-family; [`topo_do_item`] usa esse envelope para
//! posicionar QUALQUER átomo, dado o seu `vertical-align`.
//!
//! ## O algoritmo (CSS 2.1 §10.8.1, duas passadas)
//!
//! 1. Ignorando `top`/`bottom` (que não têm opinião sobre onde a baseline
//!    fica), cada átomo baseline-family contribui uma distância ACIMA da
//!    baseline (o seu topo) e uma ABAIXO (o seu fundo) — ver
//!    [`ascent_acima_da_baseline`]. O envelope inicial é o MAIOR de cada lado,
//!    entre todos os átomos e o strut.
//! 2. `top`/`bottom` são acrescentados depois: um `bottom` cuja altura excede o
//!    envelope força o lado ACIMA a crescer (o seu topo tem de caber acima da
//!    baseline sem que o FUNDO da linha se mexa); um `top` força o lado ABAIXO
//!    a crescer, pela razão simétrica. A baseline nunca se desloca por causa de
//!    um `top`/`bottom` — só a extensão da linha.
//!
//! A ordem das duas extensões (bottom-antes-de-top ou o inverso) só importa
//! quando MAIS DO QUE UM item de cada lado competem entre si e um depende do
//! resultado do outro — não é o caso de nenhuma fixture medida, e por isso a
//! implementação faz uma passada em cada direção e pára; um ponto fixo
//! iterativo fica para quando houver medição que o exija.
//!
//! **Verificado nas sete equações de `claude-vertical-align.esperado.json`**
//! (a fonte das constantes, em `style::text_metrics`): as posições de
//! `#base`/`#topo`/`#meio`/`#fundo`/`#texto-topo`/`#super`/`#sub` saem todas
//! deste modelo com as mesmas quatro constantes, incluindo os dois valores
//! (`top`/`bottom`) que já estavam certos por acidente no modelo antigo — o
//! envelope aqui dá-lhes a mesma altura de linha (50px) que o `max(alturas)`
//! antigo dava, então não regridem.
//!
//! ## CORTE: `vertical-align` não declarado continua a alinhar pelo TOPO
//!
//! A spec diz que o valor inicial é `baseline`, e [`ascent_acima_da_baseline`]
//! sabe respondê-lo — mas nenhum chamador o faz por omissão: um átomo sem
//! `vertical-align` continua a entrar como se fosse `top` (deslocamento zero),
//! que é o que o motor sempre fez. Migrar o default exigiria remedir o corpus
//! inteiro (qualquer linha com átomos de alturas diferentes e SEM a
//! propriedade declarada mudaria de posição), e nenhuma das oito divergências
//! que motivam este módulo pede essa migração — `claude-vertical-align.html`
//! declara `vertical-align: baseline` EXPLICITAMENTE em `#base`. Fica para um
//! lote medido à parte, com fixtures que isolem o default.

use crate::layout::TextMeasurer;
use crate::style::{SUB_OFFSET_RATIO, SUPER_OFFSET_RATIO, VerticalAlign, X_HEIGHT_RATIO};

/// A distância da baseline da linha ao seu TOPO (`acima`) e ao seu FUNDO
/// (`abaixo`) — CSS 2.1 §10.8.1. A baseline fica em `y + acima`, o topo da
/// linha em `y` e o fundo em `y + acima + abaixo`, onde `y` é o cursor do
/// fluxo (o topo da linha nunca se move: só a baseline e o fundo, ver o doc
/// do módulo).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::layout) struct Envelope {
    pub(in crate::layout) acima: f32,
    pub(in crate::layout) abaixo: f32,
}

impl Envelope {
    /// A altura TOTAL da linha que este envelope produz.
    pub(in crate::layout) fn altura(&self) -> f32 {
        self.acima + self.abaixo
    }
}

/// A distância entre o TOPO de um átomo baseline-family e a baseline da
/// linha — a "ascent" desse átomo. Um átomo sem baseline própria (todo o
/// conteúdo que este motor posiciona hoje: um `inline-block` vazio) tem a
/// margem de BAIXO na baseline quando `vertical-align: baseline` — logo o seu
/// topo fica exatamente `altura` acima dela, e os outros valores são esse
/// número deslocado. Ver a derivação de cada fração em `style::ASCENT_RATIO`.
///
/// `Top`/`Bottom` não têm resposta AQUI — são relativos à caixa de linha, não
/// à baseline — e os dois chamadores ([`envelope`] e [`topo_do_item`])
/// filtram-nos antes de chegar a esta função; o braço `_` que os apanha só
/// existe para não recusar um `match` não-exaustivo, nunca é exercitado.
fn ascent_acima_da_baseline(
    valign: VerticalAlign,
    altura: f32,
    font_size: f32,
    m: &dyn TextMeasurer,
) -> f32 {
    ascent_com_baseline_propria(valign, altura, altura, font_size, m)
}

/// O mesmo, para um átomo COM baseline própria: `ascent` é a distância do
/// topo dele à sua baseline (um inline-block com texto: borda + padding +
/// meia-entrelinha + ascent da fonte dele; um vazio: a altura toda, o fundo).
/// É o que faz o caret `::after` do Bootstrap (6px, vazio) e o texto ao lado
/// dele (20px, com linha) partilharem a baseline — o caret a y=9 e não a 0.
fn ascent_com_baseline_propria(
    valign: VerticalAlign,
    altura: f32,
    ascent: f32,
    font_size: f32,
    m: &dyn TextMeasurer,
) -> f32 {
    match valign {
        VerticalAlign::Sub => altura - font_size * SUB_OFFSET_RATIO,
        VerticalAlign::Super => altura + font_size * SUPER_OFFSET_RATIO,
        VerticalAlign::Middle => altura / 2.0 + font_size * X_HEIGHT_RATIO / 2.0,
        VerticalAlign::TextTop => m.font_ascent(font_size),
        VerticalAlign::TextBottom => altura - m.font_descent(font_size),
        VerticalAlign::Baseline => ascent,
        VerticalAlign::Top | VerticalAlign::Bottom => altura,
    }
}

/// [`envelope`] para átomos com baseline própria: `(altura, ascent, valign)`.
pub(in crate::layout) fn envelope_com_baseline(
    itens: &[(f32, f32, VerticalAlign)],
    font_size: f32,
    m: &dyn TextMeasurer,
) -> Envelope {
    let mut acima = m.font_ascent(font_size);
    let mut abaixo = m.font_descent(font_size);
    for &(altura, ascent, valign) in itens {
        if matches!(valign, VerticalAlign::Top | VerticalAlign::Bottom) {
            continue;
        }
        let a = ascent_com_baseline_propria(valign, altura, ascent, font_size, m).max(0.0);
        acima = acima.max(a);
        abaixo = abaixo.max((altura - a).max(0.0));
    }
    for &(altura, _, valign) in itens {
        if valign == VerticalAlign::Bottom {
            acima = acima.max(altura - abaixo);
        }
    }
    for &(altura, _, valign) in itens {
        if valign == VerticalAlign::Top {
            abaixo = abaixo.max(altura - acima);
        }
    }
    Envelope { acima, abaixo }
}

/// [`topo_do_item`] para um átomo com baseline própria.
pub(in crate::layout) fn topo_do_item_com_baseline(
    valign: VerticalAlign,
    altura: f32,
    ascent: f32,
    linha_y: f32,
    env: &Envelope,
    font_size: f32,
    m: &dyn TextMeasurer,
) -> f32 {
    match valign {
        VerticalAlign::Top => linha_y,
        VerticalAlign::Bottom => linha_y + env.altura() - altura,
        _ => linha_y + env.acima - ascent_com_baseline_propria(valign, altura, ascent, font_size, m),
    }
}

/// O ENVELOPE de uma linha com estes átomos (altura + `vertical-align` de
/// cada), à luz do STRUT (a fonte do bloco que contém a linha). Ver o
/// algoritmo de duas passadas no doc do módulo.
pub(in crate::layout) fn envelope(
    itens: &[(f32, VerticalAlign)],
    font_size: f32,
    m: &dyn TextMeasurer,
) -> Envelope {
    // Passada 1: o strut e os átomos baseline-family fecham o envelope
    // ignorando top/bottom — são eles que decidem ONDE a baseline fica.
    let mut acima = m.font_ascent(font_size);
    let mut abaixo = m.font_descent(font_size);
    for &(altura, valign) in itens {
        if matches!(valign, VerticalAlign::Top | VerticalAlign::Bottom) {
            continue;
        }
        let a = ascent_acima_da_baseline(valign, altura, font_size, m).max(0.0);
        acima = acima.max(a);
        abaixo = abaixo.max((altura - a).max(0.0));
    }
    // Passada 2: top/bottom alargam a linha sem mexer na baseline já fechada.
    // Um `bottom` mais alto do que o envelope cabe força o lado ACIMA a
    // crescer (o seu topo precisa desse espaço, e o seu fundo está preso ao
    // fundo da linha); um `top` faz o mesmo do lado ABAIXO, pela simetria.
    for &(altura, valign) in itens {
        if valign == VerticalAlign::Bottom {
            acima = acima.max(altura - abaixo);
        }
    }
    for &(altura, valign) in itens {
        if valign == VerticalAlign::Top {
            abaixo = abaixo.max(altura - acima);
        }
    }
    Envelope { acima, abaixo }
}

/// O `y` do TOPO de um átomo (altura + `vertical-align`) dentro de uma linha
/// cujo topo está em `linha_y` e cujo [`Envelope`] já é conhecido (calculado
/// por [`envelope`] sobre TODOS os átomos da linha, este incluído).
pub(in crate::layout) fn topo_do_item(
    valign: VerticalAlign,
    altura: f32,
    linha_y: f32,
    env: &Envelope,
    font_size: f32,
    m: &dyn TextMeasurer,
) -> f32 {
    match valign {
        VerticalAlign::Top => linha_y,
        VerticalAlign::Bottom => linha_y + env.altura() - altura,
        _ => {
            let baseline = linha_y + env.acima;
            baseline - ascent_acima_da_baseline(valign, altura, font_size, m)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::ApproxMeasurer;

    /// O `font_size` e o strut da fixture que calibrou as constantes:
    /// `#linha { font-size:20px }`, `line-height:20px` herdada — mas o
    /// STRUT deste módulo só usa `font_ascent`/`font_descent` (18/6.25 a
    /// 20px), não o `line-height` declarado, porque é o strut da FONTE que
    /// entra no envelope (CSS 2.1 §10.8.1), não a caixa de linha inteira.
    const FONTE: f32 = 20.0;

    /// Os sete átomos da fixture, na mesma ordem — usados por vários testes
    /// para fechar o MESMO envelope que o Chrome mediu (linha de 50px,
    /// baseline a 34.91 do topo).
    fn atomos_da_fixture() -> [(f32, VerticalAlign); 7] {
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
        let env = envelope(&atomos_da_fixture(), FONTE, &ApproxMeasurer);
        assert!((env.acima - 34.91).abs() < 0.01, "acima={}", env.acima);
        assert!((env.abaixo - 15.09).abs() < 0.01, "abaixo={}", env.abaixo);
        assert!((env.altura() - 50.0).abs() < 0.01, "altura={}", env.altura());
    }

    /// `baseline`: o fundo da caixa fica NA baseline (sem baseline própria,
    /// CSS 2.1 §10.8.1) — `#base`, h=20, `y` esperado 14.91.
    #[test]
    fn baseline_poe_o_fundo_da_caixa_na_baseline() {
        let env = envelope(&atomos_da_fixture(), FONTE, &ApproxMeasurer);
        let y = topo_do_item(VerticalAlign::Baseline, 20.0, 0.0, &env, FONTE, &ApproxMeasurer);
        assert!((y - 14.91).abs() < 0.01, "y={y}");
    }

    /// `top`: alinha com o TOPO da linha — sempre `linha_y`, independente do
    /// envelope. `#topo`, h=30, `y` esperado 0.
    #[test]
    fn top_alinha_com_o_topo_da_linha() {
        let env = envelope(&atomos_da_fixture(), FONTE, &ApproxMeasurer);
        let y = topo_do_item(VerticalAlign::Top, 30.0, 0.0, &env, FONTE, &ApproxMeasurer);
        assert!((y - 0.0).abs() < 0.01, "y={y}");
    }

    /// `middle`: o CENTRO da caixa fica meio x-height acima da baseline.
    /// `#meio`, h=40, `y` esperado 10.
    #[test]
    fn middle_centra_meio_x_height_acima_da_baseline() {
        let env = envelope(&atomos_da_fixture(), FONTE, &ApproxMeasurer);
        let y = topo_do_item(VerticalAlign::Middle, 40.0, 0.0, &env, FONTE, &ApproxMeasurer);
        assert!((y - 10.0).abs() < 0.01, "y={y}");
    }

    /// `bottom`: alinha com o FUNDO da linha — `linha_y + altura(env) -
    /// altura_do_item`. `#fundo`, h=50, `y` esperado 0 (a linha inteira mede
    /// 50, então o fundo do item COINCIDE com o topo da linha).
    #[test]
    fn bottom_alinha_com_o_fundo_da_linha() {
        let env = envelope(&atomos_da_fixture(), FONTE, &ApproxMeasurer);
        let y = topo_do_item(VerticalAlign::Bottom, 50.0, 0.0, &env, FONTE, &ApproxMeasurer);
        assert!((y - 0.0).abs() < 0.01, "y={y}");
    }

    /// `text-top`: o topo da caixa alinha com o topo da FONTE (o ascent do
    /// strut) — não com o topo da linha. `#texto-topo`, h=25, `y` esperado
    /// 16.91.
    #[test]
    fn text_top_alinha_com_o_ascent_do_strut() {
        let env = envelope(&atomos_da_fixture(), FONTE, &ApproxMeasurer);
        let y = topo_do_item(VerticalAlign::TextTop, 25.0, 0.0, &env, FONTE, &ApproxMeasurer);
        assert!((y - 16.91).abs() < 0.01, "y={y}");
    }

    /// `super`: sobe a caixa `SUPER_OFFSET_RATIO × font-size` acima de onde
    /// `baseline` a poria. `#super`, h=20, `y` esperado 7.25.
    #[test]
    fn super_sobe_a_caixa_acima_da_posicao_de_baseline() {
        let env = envelope(&atomos_da_fixture(), FONTE, &ApproxMeasurer);
        let y = topo_do_item(VerticalAlign::Super, 20.0, 0.0, &env, FONTE, &ApproxMeasurer);
        assert!((y - 7.25).abs() < 0.01, "y={y}");
    }

    /// `sub`: desce a caixa `SUB_OFFSET_RATIO × font-size` abaixo de onde
    /// `baseline` a poria. `#sub`, h=20, `y` esperado 19.91.
    #[test]
    fn sub_desce_a_caixa_abaixo_da_posicao_de_baseline() {
        let env = envelope(&atomos_da_fixture(), FONTE, &ApproxMeasurer);
        let y = topo_do_item(VerticalAlign::Sub, 20.0, 0.0, &env, FONTE, &ApproxMeasurer);
        assert!((y - 19.91).abs() < 0.01, "y={y}");
    }

    /// `text-bottom`: o fundo da caixa alinha com o fundo da FONTE (o descent
    /// do strut) — o seu topo fica `altura − descent` acima da baseline. Sem
    /// fixture medida no Chrome para este valor, o teste isola UM átomo: o
    /// envelope fecha exatamente na contribuição dele (`acima = altura −
    /// descent`, maior que o ascent do strut sozinho a 20px), então a
    /// baseline cai bem no ponto que a fórmula prevê e `y = 0`.
    #[test]
    fn text_bottom_alinha_com_o_descent_do_strut() {
        let env = envelope(&[(30.0, VerticalAlign::TextBottom)], FONTE, &ApproxMeasurer);
        let y = topo_do_item(
            VerticalAlign::TextBottom,
            30.0,
            0.0,
            &env,
            FONTE,
            &ApproxMeasurer,
        );
        assert!((y - 0.0).abs() < 0.01, "y={y}");
        // a formula por trás: 30 − descent(20) = 30 − 6.25 = 23.75 acima da
        // baseline, e é maior do que o ascent do strut sozinho (18) — por
        // isso É este átomo que fecha o lado de cima do envelope.
        assert!((env.acima - 23.75).abs() < 0.01, "acima={}", env.acima);
    }

    /// Um envelope de UM SÓ átomo nunca fica menor do que o strut sozinho —
    /// uma linha vazia (só texto, sem inline-block nenhum) ainda tem a altura
    /// da fonte do bloco.
    #[test]
    fn envelope_vazio_e_o_strut_sozinho() {
        let env = envelope(&[], FONTE, &ApproxMeasurer);
        assert!((env.acima - ApproxMeasurer.font_ascent(FONTE)).abs() < 0.01);
        assert!((env.abaixo - ApproxMeasurer.font_descent(FONTE)).abs() < 0.01);
    }
}
