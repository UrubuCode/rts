//! Trilhas COLAPSADAS de `repeat(auto-fit, …)` — CSS Grid 1 §7.2.3.3:
//! "A collapsed track is treated as having a fixed track sizing function of
//! 0px, and the gutters on either side of it — including any space allotted
//! through distributed alignment — collapse."
//!
//! São três consequências e cada uma tem um consumidor diferente, por isso
//! vivem juntas aqui e não espalhadas por `grid.rs`:
//!
//! - o TAMANHO: a trilha entra no dimensionamento como `0px` fixo, e não é
//!   zerada DEPOIS — zerar depois deixava-a comer a sua parte de `fr` e de
//!   `stretch`, e as trilhas com itens ficavam mais estreitas do que o Chrome
//!   as põe;
//! - as CALHAS: só contam as que ficam entre duas trilhas visíveis;
//! - a DISTRIBUIÇÃO (`justify-content`/`align-content`): reparte o espaço
//!   livre pelas trilhas visíveis e não pelas N da repetição. Contar as N era
//!   o que punha os itens de `grid-content-distribution-with-collapsed-tracks-*`
//!   no sítio de uma grelha que tem seis trilhas vazias a ocupar espaço.
//!
//! Com `auto-fill`, ou sem repetição automática, nada colapsa e cada função
//! daqui devolve exactamente o que o cálculo sem colapso devolvia.

use crate::style::GridTrack;

/// As trilhas que colapsam: repetições de `auto-fit` onde nenhum item calhou.
/// `ocupa` são os intervalos `[início, fim)` de trilhas que cada item cobre.
pub(in crate::layout) fn colapsadas(
    colapsavel: &[bool],
    n: usize,
    ocupa: impl Iterator<Item = (usize, usize)>,
) -> Vec<bool> {
    let mut ocupada = vec![false; n];
    for (a, b) in ocupa {
        for o in ocupada.iter_mut().take(b.min(n)).skip(a) {
            *o = true;
        }
    }
    (0..n)
        .map(|i| colapsavel.get(i).copied().unwrap_or(false) && !ocupada[i])
        .collect()
}

/// A lista de trilhas com cada colapsada trocada por `0px` fixo — a letra da
/// spec, e o que a mantém fora da repartição de espaço livre.
pub(in crate::layout) fn como_fixas_a_zero(trilhas: &mut [GridTrack], colapsada: &[bool]) {
    for (t, &c) in trilhas.iter_mut().zip(colapsada) {
        if c {
            *t = GridTrack::Fixed(crate::style::Dimension::Px(0.0));
        }
    }
}

/// Quantas trilhas NÃO colapsaram.
pub(in crate::layout) fn visiveis(n: usize, colapsada: &[bool]) -> usize {
    (0..n).filter(|&i| !colapsada.get(i).copied().unwrap_or(false)).count()
}

/// A soma das calhas de um eixo: só as que separam duas trilhas visíveis.
pub(in crate::layout) fn calhas(n: usize, colapsada: &[bool], gap: f32) -> f32 {
    visiveis(n, colapsada).saturating_sub(1) as f32 * gap
}

/// O início de cada trilha (e o fim da última, no índice `n`), com a
/// distribuição de conteúdo já aplicada. `distribuicao` é `None` quando o
/// eixo não tem `justify-content`/`align-content` que reparta, ou quando o
/// contentor não tem tamanho definido nesse eixo.
///
/// Uma trilha colapsada começa onde a anterior acabou e não abre calha nem
/// recebe espaço distribuído: é por isso que a sua vizinha visível seguinte
/// cai exactamente onde cairia se ela não existisse.
pub(in crate::layout) fn inicios(
    tamanhos: &[f32],
    colapsada: &[bool],
    origem: f32,
    gap: f32,
    distribuicao: Option<(crate::style::JustifyContent, f32)>,
) -> Vec<f32> {
    let n = tamanhos.len();
    let usado = tamanhos.iter().sum::<f32>() + calhas(n, colapsada, gap);
    let (inicial, entre) = match distribuicao {
        Some((v, contentor)) => {
            let livre = (contentor - usado).max(0.0);
            crate::layout::flex::column::justify_offsets(v, livre, visiveis(n, colapsada))
        }
        None => (0.0, 0.0),
    };
    let mut out = Vec::with_capacity(n + 1);
    let mut cursor = origem + inicial;
    let mut primeira_visivel = true;
    for (i, &t) in tamanhos.iter().enumerate() {
        if !colapsada.get(i).copied().unwrap_or(false) {
            if !primeira_visivel {
                cursor += gap + entre;
            }
            primeira_visivel = false;
        }
        out.push(cursor);
        cursor += t;
    }
    out.push(cursor);
    out
}
