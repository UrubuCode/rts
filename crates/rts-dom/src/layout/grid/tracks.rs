//! O SIZING de trilhas de grid (CSS Grid 1 §11) e a expansão de
//! `repeat(auto-fill|auto-fit, …)` (§7.2.3.2/3).
use super::*;
/// função.
pub(crate) fn expand_auto_repeats(
    track_list: &crate::style::TrackList,
    container: f32,
    gap: f32,
    min_size: Option<f32>,
) -> (
    Vec<crate::style::GridTrack>,
    Vec<bool>,
    Vec<crate::style::LineNames>,
) {
    use crate::style::GridTrack as T;
    let track_base = |t: &T| match t {
        T::Fixed(crate::style::Dimension::Px(p)) | T::Bounded { min: crate::style::Dimension::Px(p), .. } => *p,
        T::Fixed(crate::style::Dimension::Percent(pct)) | T::Bounded { min: crate::style::Dimension::Percent(pct), .. } if container > 0.0 => pct * container / 100.0,
        _ => 0.0,
    };
    let (other_base, other_count) = track_list.entries.iter().filter(|(_, t)| !matches!(t, T::AutoRepeat { .. }))
        .fold((0.0f32, 0usize), |(b, c), (_, t)| (b + track_base(t), c + 1));

    let mut out_tracks = Vec::with_capacity(track_list.entries.len());
    let mut collapsible = Vec::with_capacity(track_list.entries.len());
    let mut out_lines = Vec::with_capacity(track_list.entries.len() + 1);

    let mut current_line: crate::style::LineNames = Vec::new();

    for (entry_lines, track) in &track_list.entries {
        current_line.extend(entry_lines.iter().cloned());
        match track {
            T::AutoRepeat {
                tracks: pattern,
                fit,
                count_unit,
            } => {
                if pattern.entries.is_empty() {
                    continue;
                }
                let m = pattern.entries.len();
                let total_size = |n: usize| -> f32 {
                    let total_tracks = other_count + n * m;
                    let total_gaps = total_tracks.saturating_sub(1) as f32 * gap;
                    other_base + (n as f32) * count_unit + total_gaps
                };
                let n = if *count_unit <= 0.0 || m == 0 {
                    1
                } else if container > 0.0 {
                    let mut best = 1;
                    while total_size(best + 1) <= container {
                        best += 1;
                    }
                    best
                } else if let Some(ms) = min_size.filter(|&s| s > 0.0) {
                    let mut best = 1;
                    while total_size(best) < ms {
                        best += 1;
                    }
                    best
                } else {
                    1
                };
                for _ in 0..n {
                    for (p_lines, p_track) in &pattern.entries {
                        current_line.extend(p_lines.iter().cloned());
                        out_lines.push(std::mem::take(&mut current_line));
                        out_tracks.push(p_track.clone());
                        collapsible.push(*fit);
                    }
                    current_line.extend(pattern.trailing.iter().cloned());
                }
            }
            other => {
                out_lines.push(std::mem::take(&mut current_line));
                out_tracks.push(other.clone());
                collapsible.push(false);
            }
        }
    }
    current_line.extend(track_list.trailing.iter().cloned());
    out_lines.push(current_line);

    (out_tracks, collapsible, out_lines)
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
    // A soma das calhas, e não o `gap`: com `auto-fit` a calha ao lado de uma
    // trilha colapsada colapsa com ela (`collapse::calhas`).
    total_gap: f32,
    content_max: Option<&[f32]>,
    content_min: Option<&[f32]>,
    stretch: bool,
    ctx: &ResolveCtx,
) -> Vec<f32> {
    use crate::style::{GridTrack as T, TrackBound as B};
    let dim = |d: &crate::style::Dimension| -> f32 {
        match d {
            // % de trilha resolve contra o container (largura p/ colunas).
            crate::style::Dimension::Percent(p) => container * p / 100.0,
            other => other.resolve(ctx).unwrap_or(0.0),
        }
        .max(0.0)
    };
    let max_of = |i: usize| content_max.and_then(|c| c.get(i)).copied().unwrap_or(0.0);
    let min_of = |i: usize| content_min.and_then(|c| c.get(i)).copied().unwrap_or(0.0);
    // Um lado de `minmax()`/`fit-content()` avaliado contra o conteúdo da
    // trilha `i`. `FitContent` é `min(<len>, max-content)` na letra da spec.
    let eval_bound = |b: &B, i: usize| -> f32 {
        match b {
            B::Fixed(d) => dim(d),
            B::MinContent => min_of(i),
            B::MaxContent | B::Auto => max_of(i),
            B::FitContent(d) => dim(d).min(max_of(i)),
        }
    };

    // 1ª passada: a BASE de cada trilha — o que ela pede antes de haver sobra.
    let mut sizes = vec![0.0f32; tracks.len()];
    let mut sum_fr = 0.0f32;
    for (i, t) in tracks.iter().enumerate() {
        sizes[i] = match t {
            T::Fixed(d) => dim(d),
            T::Bounded { min, .. } => dim(min),
            T::Auto => max_of(i),
            T::Intrinsic { min, max } => {
                let base = match min {
                    B::Auto => min_of(i),
                    other => eval_bound(other, i),
                };
                if matches!(min, B::Auto) {
                    match max {
                        B::Fixed(d) => base.min(dim(d)),
                        B::FitContent(d) => base.min(dim(d)),
                        _ => base,
                    }
                } else {
                    base
                }
            }
            T::Fr(f) => {
                sum_fr += f.max(0.0);
                0.0
            }
            // Expandido antes de chegar aqui (`expand_auto_repeats`); uma
            // lista com um `AutoRepeat` por resolver é um erro do chamador.
            T::AutoRepeat { .. } | T::List(_) => 0.0,
        };
    }
    let total_used = sizes.iter().sum::<f32>() + total_gap;
    if container > 0.0 && total_used > container {
        let overflow = total_used - container;
        let auto_tracks: Vec<usize> = tracks
            .iter()
            .enumerate()
            .filter(|(_, t)| matches!(t, T::Auto))
            .map(|(i, _)| i)
            .collect();
        if !auto_tracks.is_empty() {
            let reducible: f32 = auto_tracks
                .iter()
                .map(|&i| (sizes[i] - min_of(i)).max(0.0))
                .sum();
            if reducible > 0.0 {
                let factor = (overflow / reducible).min(1.0);
                for &i in &auto_tracks {
                    let can_reduce = (sizes[i] - min_of(i)).max(0.0);
                    sizes[i] -= can_reduce * factor;
                }
            }
        }
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
    let mut remaining = free;
    let bounded: Vec<usize> = tracks
        .iter()
        .enumerate()
        .filter(|(_, t)| matches!(t, T::Bounded { .. } | T::Intrinsic { .. }))
        .map(|(i, _)| i)
        .collect();
    if !bounded.is_empty() && remaining > 0.0 {
        let ceilings: Vec<f32> = bounded
            .iter()
            .map(|&i| match &tracks[i] {
                T::Bounded { max, .. } => dim(max),
                T::Intrinsic { max, min } => {
                    let c = eval_bound(max, i);
                    if matches!(min, B::Auto) { c } else { c.max(sizes[i]) }
                }
                _ => unreachable!("filtrado acima"),
            })
            .collect();
        let mut active: Vec<usize> = (0..bounded.len())
            .filter(|&idx| sizes[bounded[idx]] < ceilings[idx])
            .collect();
        while !active.is_empty() && remaining > 1e-4 {
            let quota = remaining / active.len() as f32;
            let mut next_active = Vec::new();
            for &idx in &active {
                let i = bounded[idx];
                let ceiling = ceilings[idx];
                let grow = (ceiling - sizes[i]).min(quota);
                sizes[i] += grow;
                remaining -= grow;
                if sizes[i] < ceiling - 1e-4 {
                    next_active.push(idx);
                }
            }
            if next_active.len() == active.len() {
                break;
            }
            active = next_active;
        }
    }
    let auto_tracks: Vec<usize> = tracks
        .iter()
        .enumerate()
        .filter(|(_, t)| matches!(t, T::Auto | T::Intrinsic { max: B::Auto, .. }))
        .map(|(i, _)| i)
        .collect();
    if stretch && !auto_tracks.is_empty() && remaining > 0.0 {
        let each = remaining / auto_tracks.len() as f32;
        for i in auto_tracks {
            sizes[i] += each;
        }
    }
    sizes
}

pub(in crate::layout) fn measure_content_rows(
    dom: &Dom,
    cells: &[super::lines::GridCell],
    explicit_rows: &[crate::style::GridTrack],
    auto_row: Option<&crate::style::GridTrack>,
    col_sizes: &[f32],
    col_sub: Option<&super::subgrid::SubgriddedAxis>,
    col_gap: f32,
    row_gap: f32,
    nrows: usize,
    container_content_h: Option<f32>,
    resolve: &ResolveCtx,
    wm: crate::style::WritingMode,
    ctx: &LayoutCtx,
) -> (Vec<f32>, Vec<f32>) {
    let mut content_row_h = vec![0.0f32; nrows];
    let mut min_row_h = vec![0.0f32; nrows];
    let mut multi_row_items = Vec::new();
    for cell in cells {
        let is_row_sub = dom
            .computed_style_idx(cell.child)
            .is_some_and(|c| super::subgrid::is_subgrid_item(&c, wm).1);
        if is_row_sub {
            continue;
        }
        let cw = col_sub
            .map(|s| s.cell_span(cell.c0, cell.c1))
            .unwrap_or_else(|| super::grid::span_size(col_sizes, cell.c0, cell.c1, col_gap));
        let is_rep = matches!(&dom.node(cell.child).kind, crate::dom::NodeKind::Element { tag } if matches!(tag.as_str(), "img" | "canvas" | "svg"));
        let stretch_x = dom.computed_style_idx(cell.child).is_some_and(|c| {
            let j = c.justify_self.unwrap_or(crate::style::AlignItems::Normal);
            let s = match j {
                crate::style::AlignItems::Stretch => true,
                crate::style::AlignItems::Normal => !is_rep,
                _ => false,
            };
            s && c.width.is_none()
        });
        let forced_w = if stretch_x { Some(cw) } else { None };
        let area_h = super::aspect::definite_area_height(explicit_rows, auto_row, (cell.r0, cell.r1), row_gap, container_content_h, resolve);
        let (_, h) = measure_block(
            dom, cell.child, cell.box_id, cw, area_h, forced_w, None, !stretch_x, ctx,
        );
        if cell.rows() == 1 && cell.r0 < nrows {
            content_row_h[cell.r0] = content_row_h[cell.r0].max(h);
            let mh = super::aspect::grid_item_min_content_height(dom, cell.child, h, cw, resolve.node_font_size, resolve, ctx);
            min_row_h[cell.r0] = min_row_h[cell.r0].max(mh);
        } else if cell.rows() > 1 {
            multi_row_items.push((cell, h));
        }
    }
    for (cell, h) in multi_row_items {
        let end = cell.r1.min(nrows);
        let mut current_span_h = (end.saturating_sub(cell.r0 + 1)) as f32 * row_gap;
        let mut unfixed_rows = Vec::new();
        for r in cell.r0..end {
            let track = explicit_rows
                .get(r)
                .or_else(|| auto_row.map(|t| t.track_at(r.saturating_sub(explicit_rows.len()))));
            match track {
                Some(crate::style::GridTrack::Fixed(d)) => {
                    current_span_h +=
                        resolve_height(Some(*d), container_content_h, resolve).unwrap_or(0.0);
                }
                _ => {
                    current_span_h += content_row_h[r];
                    unfixed_rows.push(r);
                }
            }
        }
        if h > current_span_h {
            let targets = if !unfixed_rows.is_empty() {
                unfixed_rows
            } else {
                (cell.r0..end).collect()
            };
            let extra = (h - current_span_h) / targets.len() as f32;
            for r in targets {
                content_row_h[r] += extra;
            }
        }
    }
    (content_row_h, min_row_h)
}

#[allow(clippy::too_many_arguments)]
pub(in crate::layout) fn resolve_row_tracks(
    nrows: usize,
    explicit_rows: &[crate::style::GridTrack],
    auto_row: Option<&crate::style::GridTrack>,
    content_row_h: &[f32],
    min_row_h: &[f32],
    container_h: Option<f32>,
    resolve: &ResolveCtx,
    align_content: Option<crate::style::JustifyContent>,
    row_collapsed: &[bool],
    row_gap: f32,
) -> Vec<f32> {
    use crate::style::{GridTrack as T, TrackBound as B};
    let row_track = |r: usize| {
        explicit_rows
            .get(r)
            .or_else(|| auto_row.map(|t| t.track_at(r.saturating_sub(explicit_rows.len()))))
    };
    let mut row_sizes: Vec<f32> = (0..nrows)
        .map(|r| {
            let ch = content_row_h.get(r).copied().unwrap_or(0.0);
            match row_track(r) {
                Some(T::Fixed(d)) => resolve_height(Some(*d), container_h, resolve).unwrap_or(ch),
                Some(T::Bounded { min, max }) => {
                    let mn = resolve_height(Some(*min), container_h, resolve).unwrap_or(0.0);
                    let mx =
                        resolve_height(Some(*max), container_h, resolve).unwrap_or(f32::INFINITY);
                    ch.clamp(mn, mx)
                }
                Some(T::Intrinsic { min, max }) => {
                    let min_h = min_row_h.get(r).copied().unwrap_or(0.0);
                    let mn = match min {
                        B::Fixed(d) => resolve_height(Some(*d), container_h, resolve).unwrap_or(0.0),
                        B::MinContent | B::Auto => min_h,
                        B::MaxContent => ch,
                        B::FitContent(d) => resolve_height(Some(*d), container_h, resolve).unwrap_or(ch),
                    };
                    let mx = match max {
                        B::Fixed(d) => resolve_height(Some(*d), container_h, resolve).unwrap_or(f32::INFINITY),
                        B::MaxContent | B::Auto => f32::INFINITY,
                        B::MinContent => min_h,
                        B::FitContent(d) => resolve_height(Some(*d), container_h, resolve).unwrap_or(ch),
                    };
                    let base = if matches!(min, B::Auto) {
                        mn.min(mx)
                    } else {
                        mn
                    };
                    ch.max(base).min(mx.max(base))
                }
                _ => ch,
            }
        })
        .collect();

    if align_content.is_none() {
        if let Some(ch) = container_h {
            let total_gap = super::collapse::gutters(nrows, row_collapsed, row_gap);
            let sum_fr: f32 = (0..nrows).filter_map(|r| match row_track(r) { Some(T::Fr(f)) => Some(f.max(0.0)), _ => None }).sum();
            if sum_fr > 0.0 {
                let non_fr: f32 = (0..nrows).filter(|&r| !matches!(row_track(r), Some(T::Fr(_)))).map(|r| row_sizes[r]).sum::<f32>() + total_gap;
                let free = (ch - non_fr).max(0.0);
                for r in 0..nrows {
                    if let Some(T::Fr(f)) = row_track(r) { row_sizes[r] = free * f.max(0.0) / sum_fr; }
                }
            } else {
                let min_of_row = |r: usize| match row_track(r) {
                    Some(T::Fixed(d)) => resolve_height(Some(*d), container_h, resolve).unwrap_or(0.0),
                    Some(T::Bounded { min, .. }) => resolve_height(Some(*min), container_h, resolve).unwrap_or(0.0),
                    Some(T::Intrinsic { min, .. }) => match min {
                        B::Fixed(d) => resolve_height(Some(*d), container_h, resolve).unwrap_or(0.0),
                        B::MinContent | B::Auto => min_row_h.get(r).copied().unwrap_or(0.0),
                        B::MaxContent => content_row_h.get(r).copied().unwrap_or(0.0),
                        B::FitContent(d) => resolve_height(Some(*d), container_h, resolve).unwrap_or(0.0),
                    },
                    _ => min_row_h.get(r).copied().unwrap_or(0.0),
                };
                let current_used: f32 = row_sizes.iter().sum::<f32>() + total_gap;
                if ch > current_used {
                    let auto_rows: Vec<usize> = (0..nrows).filter(|&r| matches!(row_track(r), None | Some(T::Auto))).collect();
                    if !auto_rows.is_empty() {
                        let each = (ch - current_used) / auto_rows.len() as f32;
                        for r in auto_rows { row_sizes[r] += each; }
                    }
                } else if current_used > ch {
                    let overflow = current_used - ch;
                    let non_fixed: Vec<usize> = (0..nrows).filter(|&r| !matches!(row_track(r), Some(T::Fixed(_)))).collect();
                    let reducible: f32 = non_fixed
                        .iter()
                        .map(|&r| (row_sizes[r] - min_of_row(r)).max(0.0))
                        .sum();
                    if reducible > 0.0 {
                        let factor = (overflow / reducible).min(1.0);
                        for &r in &non_fixed {
                            let can_reduce = (row_sizes[r] - min_of_row(r)).max(0.0);
                            row_sizes[r] -= can_reduce * factor;
                        }
                    }
                }
            }
        } else {
            let mut max_fr_share = 0.0f32;
            for r in 0..nrows {
                if let Some(T::Fr(f)) = row_track(r) {
                    if *f > 0.0 {
                        let ch = content_row_h.get(r).copied().unwrap_or(0.0);
                        max_fr_share = max_fr_share.max(ch / f);
                    }
                }
            }
            if max_fr_share > 0.0 {
                for r in 0..nrows {
                    if let Some(T::Fr(f)) = row_track(r) {
                        row_sizes[r] = row_sizes[r].max(max_fr_share * f);
                    }
                }
            }
        }
    }
    row_sizes
}
