//! ALINHAMENTO VERTICAL: o modelo de baseline que `vertical-align` lê.
//!
//! Até aqui o motor só sabia posicionar dois dos oito valores CSS
//! (`middle`/`bottom`), e só dentro da "corrida" de inline-blocks
//! (`layout_inline_block_line`, em `line_inline_block.rs`) — o resto caía num `_ => 0.0`
//! que alinhava pelo TOPO. A causa não era um valor por implementar de cada
//! vez: era a falta de uma ENTIDADE. Cada átomo de uma linha (um `inline-block`
//! vazio, hoje; um run de texto, amanhã) só tem ALTURA — não tem onde guardar
//! "a que distância da baseline fica o meu topo", e sem esse número
//! `baseline`/`sub`/`super`/`text-top`/`text-bottom` não têm o que ler.
//!
//! Este módulo dá nome a essa entidade: [`Envelope`] é a distância da baseline
//! da linha ao seu topo e ao seu fundo — o "acima"/"abaixo" do CSS 2.1
//! §10.8.1 — calculado a partir do STRUT (a fonte do bloco que contém a linha)
//! e de cada átomo baseline-family; [`item_top`] usa esse envelope para
//! posicionar QUALQUER átomo, dado o seu `vertical-align`.
//!
//! ## O algoritmo (CSS 2.1 §10.8.1, duas passadas)
//!
//! 1. Ignorando `top`/`bottom` (que não têm opinião sobre onde a baseline
//!    fica), cada átomo baseline-family contribui uma distância ACIMA da
//!    baseline (o seu topo) e uma ABAIXO (o seu fundo) — ver
//!    [`ascent_above_baseline`]. O envelope inicial é o MAIOR de cada lado,
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
//! A spec diz que o valor inicial é `baseline`, e [`ascent_above_baseline`]
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

/// A distância da baseline da linha ao seu TOPO (`above`) e ao seu FUNDO
/// (`below`) — CSS 2.1 §10.8.1. A baseline fica em `y + above`, o topo da
/// linha em `y` e o fundo em `y + above + below`, onde `y` é o cursor do
/// fluxo (o topo da linha nunca se move: só a baseline e o fundo, ver o doc
/// do módulo).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::layout) struct Envelope {
    pub(in crate::layout) above: f32,
    pub(in crate::layout) below: f32,
}

impl Envelope {
    /// A altura TOTAL da linha que este envelope produz.
    pub(in crate::layout) fn height(&self) -> f32 {
        self.above + self.below
    }
}

/// A distância entre o TOPO de um átomo baseline-family e a baseline da
/// linha — a "ascent" desse átomo. Um átomo sem baseline própria (todo o
/// conteúdo que este motor posiciona hoje: um `inline-block` vazio) tem a
/// margem de BAIXO na baseline quando `vertical-align: baseline` — logo o seu
/// topo fica exatamente `height` acima dela, e os outros valores são esse
/// número deslocado. Ver a derivação de cada fração em `style::ASCENT_RATIO`.
///
/// `Top`/`Bottom` não têm resposta AQUI — são relativos à caixa de linha, não
/// à baseline — e os dois chamadores ([`envelope`] e [`item_top`])
/// filtram-nos antes de chegar a esta função; o braço `_` que os apanha só
/// existe para não recusar um `match` não-exaustivo, nunca é exercitado.
pub(in crate::layout) fn ascent_above_baseline(
    valign: VerticalAlign,
    height: f32,
    font_size: f32,
    m: &dyn TextMeasurer,
) -> f32 {
    ascent_with_own_baseline(valign, height, height, font_size, None, m)
}

/// O mesmo, para um átomo COM baseline própria: `ascent` é a distância do
/// topo dele à sua baseline (um inline-block com texto: borda + padding +
/// meia-entrelinha + ascent da fonte dele; um vazio: a altura toda, o fundo).
/// É o que faz o caret `::after` do Bootstrap (6px, vazio) e o texto ao lado
/// dele (20px, com linha) partilharem a baseline — o caret a y=9 e não a 0.
pub(in crate::layout) fn ascent_with_own_baseline(
    valign: VerticalAlign,
    height: f32,
    ascent: f32,
    font_size: f32,
    // `font-family` do STRUT — Ahem responde `font_ascent`/`font_descent`
    // pela fração exata (0.8/0.2) em vez da calibrada contra o Chrome.
    family: Option<&str>,
    m: &dyn TextMeasurer,
) -> f32 {
    match valign {
        VerticalAlign::Sub => height - font_size * SUB_OFFSET_RATIO,
        VerticalAlign::Super => height + font_size * SUPER_OFFSET_RATIO,
        VerticalAlign::Middle => height / 2.0 + font_size * X_HEIGHT_RATIO / 2.0,
        VerticalAlign::TextTop => m.font_ascent_family(font_size, family),
        VerticalAlign::TextBottom => height - m.font_descent_family(font_size, family),
        VerticalAlign::Baseline => ascent,
        VerticalAlign::Top | VerticalAlign::Bottom => height,
    }
}

/// [`envelope`] para átomos com baseline própria: `(height, ascent, valign)`.
/// O STRUT é o da caixa de linha do pai — `line_height` repartido pela
/// meia-entrelinha à volta da content area (CSS 2.1 §10.8.1), e não a fonte
/// crua: com `line-height: 20px` a 16px o strut é 15,4 acima / 4,6 abaixo, e
/// uma linha só de inline-blocks com texto mede exactamente 20 (o Blink), não
/// 20,4 (`claude-letter-spacing`, `#negativo.y`).
pub(in crate::layout) fn envelope_with_baseline(
    items: &[(f32, f32, VerticalAlign)],
    font_size: f32,
    line_height: f32,
    family: Option<&str>,
    m: &dyn TextMeasurer,
) -> Envelope {
    let content = crate::inline_box::altura_do_conteudo(font_size, family, m);
    let half_leading = crate::inline_box::meia_entrelinha(line_height, content);
    let (mut above, mut below) = if crate::layout::block::rotated::in_rotated_frame() {
        (0.0, 0.0)
    } else {
        let a = half_leading + m.font_ascent_family(font_size, family);
        let b = (line_height - a).max(0.0);
        (a, b)
    };
    for &(height, ascent, valign) in items {
        if matches!(valign, VerticalAlign::Top | VerticalAlign::Bottom) {
            continue;
        }
        let a = ascent_with_own_baseline(valign, height, ascent, font_size, family, m).max(0.0);
        above = above.max(a);
        below = below.max((height - a).max(0.0));
    }
    for &(height, _, valign) in items {
        if valign == VerticalAlign::Bottom {
            above = above.max(height - below);
        }
    }
    for &(height, _, valign) in items {
        if valign == VerticalAlign::Top {
            below = below.max(height - above);
        }
    }
    Envelope { above, below }
}

/// [`item_top`] para um átomo com baseline própria.
pub(in crate::layout) fn item_top_with_baseline(
    valign: VerticalAlign,
    height: f32,
    ascent: f32,
    line_y: f32,
    env: &Envelope,
    font_size: f32,
    family: Option<&str>,
    m: &dyn TextMeasurer,
) -> f32 {
    match valign {
        VerticalAlign::Top => line_y,
        VerticalAlign::Bottom => line_y + env.height() - height,
        _ => {
            line_y + env.above
                - ascent_with_own_baseline(valign, height, ascent, font_size, family, m)
        }
    }
}

/// O ENVELOPE de uma linha com estes átomos (altura + `vertical-align` de
/// cada), à luz do STRUT (a fonte do bloco que contém a linha). Ver o
/// algoritmo de duas passadas no doc do módulo.
pub(in crate::layout) fn envelope(
    items: &[(f32, VerticalAlign)],
    font_size: f32,
    m: &dyn TextMeasurer,
) -> Envelope {
    // Passada 1: o strut e os átomos baseline-family fecham o envelope
    // ignorando top/bottom — são eles que decidem ONDE a baseline fica.
    let mut above = m.font_ascent(font_size);
    let mut below = m.font_descent(font_size);
    for &(height, valign) in items {
        if matches!(valign, VerticalAlign::Top | VerticalAlign::Bottom) {
            continue;
        }
        let a = ascent_above_baseline(valign, height, font_size, m).max(0.0);
        above = above.max(a);
        below = below.max((height - a).max(0.0));
    }
    // Passada 2: top/bottom alargam a linha sem mexer na baseline já fechada.
    // Um `bottom` mais alto do que o envelope cabe força o lado ACIMA a
    // crescer (o seu topo precisa desse espaço, e o seu fundo está preso ao
    // fundo da linha); um `top` faz o mesmo do lado ABAIXO, pela simetria.
    for &(height, valign) in items {
        if valign == VerticalAlign::Bottom {
            above = above.max(height - below);
        }
    }
    for &(height, valign) in items {
        if valign == VerticalAlign::Top {
            below = below.max(height - above);
        }
    }
    Envelope { above, below }
}

/// O `y` do TOPO de um átomo (altura + `vertical-align`) dentro de uma linha
/// cujo topo está em `line_y` e cujo [`Envelope`] já é conhecido (calculado
/// por [`envelope`] sobre TODOS os átomos da linha, este incluído).
pub(in crate::layout) fn item_top(
    valign: VerticalAlign,
    height: f32,
    line_y: f32,
    env: &Envelope,
    font_size: f32,
    m: &dyn TextMeasurer,
) -> f32 {
    match valign {
        VerticalAlign::Top => line_y,
        VerticalAlign::Bottom => line_y + env.height() - height,
        _ => {
            let baseline = line_y + env.above;
            baseline - ascent_above_baseline(valign, height, font_size, m)
        }
    }
}


