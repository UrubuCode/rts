//! O SIZING de trilhas de grid (CSS Grid 1 §11) e a expansão de
//! `repeat(auto-fill|auto-fit, …)` (§7.2.3.3) — as duas perguntas do lote R
//! que ficaram por fazer: "quantas trilhas cabem" é do `content_w`, e
//! "quão grande fica uma trilha intrínseca" precisa do min-content E do
//! max-content dos itens, não só do segundo.
//!
//! Extraído de `grid.rs` (que já estava no teto) — `resolve_tracks` cresceu
//! dois parâmetros e um variant a mais para dar conta de `Intrinsic`, e a
//! expansão de `AutoRepeat` é lógica nova por inteiro.

use super::*;

/// Um comprimento de TRILHA em px. A `%` resolve contra o CONTAINER do eixo —
/// é o que uma trilha percentual mede — e não pelo `Dimension::resolve`
/// genérico, que a resolveria contra a largura do pai da caixa. Nunca negativo.
///
/// Uma função e não duas: a contagem de repetições e o sizing das trilhas
/// perguntam a MESMA coisa, e tê-la escrita nos dois sítios é como um `%` de
/// contagem acabaria a divergir de um `%` de tamanho.
fn comprimento_de_trilha(d: &crate::style::Dimension, container: f32, ctx: &ResolveCtx) -> f32 {
    match d {
        crate::style::Dimension::Percent(p) => container * p / 100.0,
        other => other.resolve(ctx).unwrap_or(0.0),
    }
    .max(0.0)
}

/// `repeat(auto-fill|auto-fit, tracks)` → N cópias do padrão, N decidido
/// AGORA contra `container` (largura/altura disponível) e `gap`. Uma lista
/// sem nenhum `AutoRepeat` volta inalterada (o caminho comum, sem custo).
///
/// A fórmula é a da spec (§7.2.3.3): cada repetição "pesa"
/// `count_unit(padrão) + gaps internos`, e cabe mais uma sempre que sobrar
/// espaço para ela MAIS o gap que a precede — por isso `(container + gap) /
/// (peso + gap)`, e não `container / peso`, que subcontaria a última que
/// cabe exatamente. Nunca menos de 1: uma trilha vazia ainda é uma trilha.
///
/// Um lado INTRÍNSECO (`auto`, `min-content`, `max-content`, `fit-content()`)
/// pesa 0, e isso é o que a Grid 1 manda: §7.2.3.3 diz que o argumento de um
/// `repeat(auto-fill|auto-fit, …)` **não pode** conter tamanhos intrínsecos ou
/// flexíveis, portanto a declaração é inválida e o Blink responde `none`. Quem
/// a escreve é a CSS Grid **3** (`display: grid-lanes`), que a define pelo
/// conteúdo dos itens — e esse eixo não existe neste motor, por isso contar
/// pelo conteúdo aqui daria uma resposta que nenhum browser dá. Fica dito em
/// vez de adivinhado: quando `grid-lanes` chegar, é este `unidade` que ganha o
/// `(min-content, max-content)` dos itens por parâmetro.
///
/// `auto-fit` usa a MESMA contagem que `auto-fill` (a spec não distingue
/// aqui) — a diferença entre os dois é só se as repetições SEM item colapsam
/// depois da colocação, que é `collapsible` (devolvido ao lado) e não esta
/// função.
pub(in crate::layout) fn expand_auto_repeats(
    tracks: Vec<crate::style::GridTrack>,
    container: f32,
    gap: f32,
    ctx: &ResolveCtx,
) -> (Vec<crate::style::GridTrack>, Vec<bool>) {
    use crate::style::{GridTrack as T, TrackBound as B};
    if !tracks.iter().any(|t| matches!(t, T::AutoRepeat { .. })) {
        let n = tracks.len();
        return (tracks, vec![false; n]);
    }
    let unidade = |b: &B| -> f32 {
        match b {
            B::Fixed(d) => comprimento_de_trilha(d, container, ctx),
            B::MinContent | B::MaxContent | B::FitContent(_) => 0.0,
        }
    };
    let mut out = Vec::with_capacity(tracks.len());
    let mut collapsible = Vec::with_capacity(tracks.len());
    for t in tracks {
        match t {
            T::AutoRepeat {
                tracks: pattern,
                fit,
                count_unit,
            } => {
                if pattern.is_empty() {
                    continue;
                }
                let internal_gaps = (pattern.len().saturating_sub(1)) as f32 * gap;
                let soma: f32 = count_unit.iter().map(|b| unidade(b)).sum();
                let per_rep = (soma + internal_gaps).max(0.0);
                let n = if per_rep <= 0.0 || container <= 0.0 {
                    1
                } else {
                    (((container + gap) / (per_rep + gap)).floor() as i64).max(1) as usize
                };
                for _ in 0..n {
                    for p in &pattern {
                        out.push(p.clone());
                        collapsible.push(fit);
                    }
                }
            }
            other => {
                out.push(other);
                collapsible.push(false);
            }
        }
    }
    (out, collapsible)
}

/// Zera as trilhas `auto-fit` que não receberam NENHUM item — CSS Grid 1
/// §7.2.3.3 — e devolve QUAIS colapsaram.
///
/// A máscara é o ponto todo, e o que a obrigou está escrito na spec a seguir
/// ao zero: *"a collapsed track is treated as having a fixed track sizing
/// function of 0px, **and the gutters on either side of it — including any
/// space allotted through distributed alignment — collapse**"*. São três
/// consequências e o tamanho é só a primeira:
///
/// 1. a trilha mede 0 (era só isto que esta função fazia);
/// 2. o gap de cada lado dela desaparece;
/// 3. ela não conta como trilha para `justify-content`/`align-content`.
///
/// Devolver a máscara em vez de contar zeros é deliberado: um `0px` escrito
/// pelo autor mede o mesmo e **não** colapsa gap nenhum nem sai da contagem.
/// Só quem colapsou sabe que colapsou.
///
/// O que isto valia: `grid-content-distribution-with-collapsed-tracks-011`
/// é `repeat(auto-fit, 20px)` nos dois eixos num quadrado de 200px com quatro
/// itens. São dez trilhas por eixo, três com item; `space-between` tem de
/// repartir os 140px livres por DOIS intervalos (entre as três vivas), e
/// contava nove. A fixture desenha quadrados vermelhos exactamente onde os
/// itens têm de aterrar, e é por isso que ela falha visivelmente em vez de
/// falhar por um pixel.
pub(in crate::layout) fn collapse_empty_auto_fit_tracks(
    sizes: &mut [f32],
    collapsible: &[bool],
    occupied: &[bool],
) -> Vec<bool> {
    let mut colapsadas = vec![false; sizes.len()];
    for i in 0..sizes.len() {
        if collapsible.get(i).copied().unwrap_or(false) && !occupied.get(i).copied().unwrap_or(false) {
            sizes[i] = 0.0;
            colapsadas[i] = true;
        }
    }
    colapsadas
}

/// Quantas trilhas SOBREVIVEM a um colapso de `auto-fit` — o `n` que a
/// distribuição de conteúdo pergunta, e o número de gaps é `n - 1`.
///
/// Nunca menos de 1: `justify_offsets` divide por `n` e por `n - 1`, e uma
/// grade cujas trilhas colapsaram todas não tem por onde repartir nada.
pub(in crate::layout) fn trilhas_vivas(colapsadas: &[bool]) -> usize {
    colapsadas.iter().filter(|c| !**c).count().max(1)
}

/// A LARGURA (ou altura) de cada trilha de uma grade — CSS Grid 1 §11,
/// reduzido ao que este motor sustenta: sem itens a atravessar trilhas (essa
/// repartição é a mesma pergunta do `colspan` de tabela, tratada à parte em
/// `grid.rs`) e sem uma segunda passada de "resolve intrinsic track sizes"
/// distribuindo POR ITEM — cada trilha já lê o seu próprio conteúdo direto.
///
/// A ordem das passadas é a regra, e não um detalhe de implementação: uma
/// trilha intrínseca é dimensionada pelo CONTEÚDO antes de qualquer espaço
/// livre ser repartido, porque o espaço livre só existe depois de se saber o
/// que o conteúdo pede. Inverter as duas é o que fazia a grade do `<main>` da
/// Wikipédia dar 948px à coluna de conteúdo e empurrar a barra lateral para
/// fora da janela.
///
/// `conteudo_max[i]`/`conteudo_min[i]` são o max-content/min-content dos
/// itens da trilha `i` — `None` quando quem chama não os mediu (nenhuma
/// trilha `Auto`/`Intrinsic` na lista, e aí não são precisos).
pub(in crate::layout) fn resolve_tracks(
    tracks: &[crate::style::GridTrack],
    container: f32,
    gap: f32,
    conteudo_max: Option<&[f32]>,
    conteudo_min: Option<&[f32]>,
    ctx: &ResolveCtx,
) -> Vec<f32> {
    use crate::style::{GridTrack as T, TrackBound as B};
    let n = tracks.len().max(1);
    let total_gap = (n.saturating_sub(1)) as f32 * gap;
    // % de trilha resolve contra o container (largura p/ colunas) — ver
    // `comprimento_de_trilha`, partilhada com a contagem de repetições.
    let dim = |d: &crate::style::Dimension| -> f32 { comprimento_de_trilha(d, container, ctx) };
    let max_de = |i: usize| conteudo_max.and_then(|c| c.get(i)).copied().unwrap_or(0.0);
    let min_de = |i: usize| conteudo_min.and_then(|c| c.get(i)).copied().unwrap_or(0.0);
    // Um lado de `minmax()`/`fit-content()` avaliado contra o conteúdo da
    // trilha `i`. `FitContent` é `min(<len>, max-content)` na letra da spec.
    let eval_bound = |b: &B, i: usize| -> f32 {
        match b {
            B::Fixed(d) => dim(d),
            B::MinContent => min_de(i),
            B::MaxContent => max_de(i),
            B::FitContent(d) => dim(d).min(max_de(i)),
        }
    };

    // 1ª passada: a BASE de cada trilha — o que ela pede antes de haver sobra.
    let mut sizes = vec![0.0f32; tracks.len()];
    let mut sum_fr = 0.0f32;
    for (i, t) in tracks.iter().enumerate() {
        sizes[i] = match t {
            T::Fixed(d) => dim(d),
            T::Bounded { min, .. } => dim(min),
            T::Auto => max_de(i),
            T::Intrinsic { min, .. } => eval_bound(min, i),
            T::Fr(f) => {
                sum_fr += f.max(0.0);
                0.0
            }
            // Expandido antes de chegar aqui (`expand_auto_repeats`); uma
            // lista com um `AutoRepeat` por resolver é um erro do chamador.
            T::AutoRepeat { .. } => 0.0,
        };
    }
    let free = (container - sizes.iter().sum::<f32>() - total_gap).max(0.0);

    // 2ª passada: o espaço livre. `fr` come-o todo quando existe — é o que a
    // unidade significa —, e nesse caso uma trilha limitada ou intrínseca fica
    // pela sua base.
    if sum_fr > 0.0 {
        for (i, t) in tracks.iter().enumerate() {
            if let T::Fr(f) = t {
                sizes[i] = free * f.max(0.0) / sum_fr;
            }
        }
        return sizes;
    }

    // 3ª passada, sem `fr`: primeiro as trilhas LIMITADAS (`Bounded` e
    // `Intrinsic` — as duas têm um TECTO real, ao contrário de `Auto`, cujo
    // "máximo" é o próprio conteúdo e por isso cresce na 4ª) crescem até ao
    // seu máximo, e só o que sobrar depois disso é que estica as intrínsecas
    // sem tecto — `align-content: stretch`, o default.
    let mut sobra = free;
    let limitadas: Vec<usize> = tracks
        .iter()
        .enumerate()
        .filter(|(_, t)| matches!(t, T::Bounded { .. } | T::Intrinsic { .. }))
        .map(|(i, _)| i)
        .collect();
    if !limitadas.is_empty() && sobra > 0.0 {
        // Reparte por igual e não em proporção: a proporção seria contra as
        // bases, que num `minmax(0, x)` são todas zero.
        let quota = sobra / limitadas.len() as f32;
        for i in limitadas {
            let teto = match &tracks[i] {
                T::Bounded { max, .. } => dim(max),
                // O mínimo pode exceder o máximo declarado (uma palavra mais
                // larga do que o `minmax(min-content, 200px)` permite) — a
                // spec (§11.1) manda o TECTO subir para acompanhar a base
                // nesse caso, nunca encolher a base para caber nele.
                T::Intrinsic { max, .. } => eval_bound(max, i).max(sizes[i]),
                _ => unreachable!("filtrado acima"),
            };
            let novo = (sizes[i] + quota).min(teto);
            sobra -= novo - sizes[i];
            sizes[i] = novo;
        }
    }
    let autos: Vec<usize> = tracks
        .iter()
        .enumerate()
        .filter(|(_, t)| matches!(t, T::Auto))
        .map(|(i, _)| i)
        .collect();
    if !autos.is_empty() && sobra > 0.0 {
        let cada = sobra / autos.len() as f32;
        for i in autos {
            sizes[i] += cada;
        }
    }
    sizes
}
