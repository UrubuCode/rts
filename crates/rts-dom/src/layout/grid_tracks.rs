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
/// `auto-fit` usa a MESMA contagem que `auto-fill` (a spec não distingue
/// aqui) — a diferença entre os dois é só se as repetições SEM item colapsam
/// depois da colocação, que é `collapsible` (devolvido ao lado) e não esta
/// função.
pub(in crate::layout) fn expand_auto_repeats(
    tracks: Vec<crate::style::GridTrack>,
    container: f32,
    gap: f32,
) -> (Vec<crate::style::GridTrack>, Vec<bool>) {
    use crate::style::GridTrack as T;
    if !tracks.iter().any(|t| matches!(t, T::AutoRepeat { .. })) {
        let n = tracks.len();
        return (tracks, vec![false; n]);
    }
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
                let per_rep = (count_unit + internal_gaps).max(0.0);
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
/// §7.2.3.3 "the empty repeated tracks are collapsed". Só o TAMANHO colapsa
/// aqui (o gap ao lado de uma trilha colapsada continua a ser contado): a
/// spec também suprime esse gap, que ficou por fazer — nenhuma fixture do
/// corpus mede `auto-fit` com trilhas vazias (só `auto-fill`, onde isto é
/// sempre `false` e a função não toca em nada), por isso a aproximação fica
/// documentada em vez de adivinhada.
pub(in crate::layout) fn collapse_empty_auto_fit_tracks(
    sizes: &mut [f32],
    collapsible: &[bool],
    occupied: &[bool],
) {
    for i in 0..sizes.len() {
        if collapsible.get(i).copied().unwrap_or(false) && !occupied.get(i).copied().unwrap_or(false) {
            sizes[i] = 0.0;
        }
    }
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
    let dim = |d: &crate::style::Dimension| -> f32 {
        match d {
            // % de trilha resolve contra o container (largura p/ colunas).
            crate::style::Dimension::Percent(p) => container * p / 100.0,
            other => other.resolve(ctx).unwrap_or(0.0),
        }
        .max(0.0)
    };
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
