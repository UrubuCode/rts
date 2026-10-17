//! O SIZING de trilhas de grid (CSS Grid 1 §11) e a expansão de
//! `repeat(auto-fill|auto-fit, …)` (§7.2.3.2/3).
use super::*;
pub(crate) use super::repeats::expand_auto_repeats;


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
            B::FitContent(d) => dim(d).max(min_of(i)).min(max_of(i).max(min_of(i))),
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
            T::Intrinsic { min, max } => match min {
                B::Auto => match max {
                    B::FitContent(_) => eval_bound(max, i),
                    _ => min_of(i),
                },
                other => eval_bound(other, i),
            },

            T::Fr(f) => {
                sum_fr += f.max(0.0);
                min_of(i)
            }
            T::Flex { min, flex } => {
                sum_fr += flex.max(0.0);
                match min {
                    B::Auto => min_of(i),
                    other => eval_bound(other, i),
                }
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
            .filter(|(_, t)| matches!(t, T::Auto | T::Intrinsic { max: B::FitContent(_), .. }))
            .map(|(i, _)| i)
            .collect();
        let min_sizes: Vec<f32> = (0..tracks.len()).map(min_of).collect();
        super::spanning::reduce_tracks_overflow(&mut sizes, &min_sizes, &auto_tracks, overflow);
    }
    let free = (container - sizes.iter().sum::<f32>() - total_gap).max(0.0);

    // 2ª passada: o espaço livre. `fr` come-o todo quando existe — é o que a
    // unidade significa —, e nesse caso uma trilha limitada ou intrínseca fica
    // pela sua base.
    if sum_fr > 0.0 {
        let mut frozen = vec![false; tracks.len()];
        loop {
            let active_fr: f32 = tracks.iter().enumerate().filter(|(i, t)| !frozen[*i] && t.flex_factor().is_some())
                .map(|(_, t)| t.flex_factor().unwrap_or(0.0).max(0.0)).sum();
            let non_flex_sum: f32 = tracks.iter().enumerate()
                .filter(|(i, t)| frozen[*i] || t.flex_factor().is_none())
                .map(|(i, _)| sizes[i])
                .sum::<f32>() + total_gap;
            let leftover = (container - non_flex_sum).max(0.0);
            let flex_sum = active_fr.max(1.0);
            let hypothetical_fr = leftover / flex_sum;
            let mut newly_frozen = false;
            for (i, t) in tracks.iter().enumerate() {
                if let Some(f) = t.flex_factor() {
                    let base = sizes[i];
                    if !frozen[i] && hypothetical_fr * f.max(0.0) < base {
                        sizes[i] = base;
                        frozen[i] = true;
                        newly_frozen = true;
                    }
                }
            }
            if !newly_frozen {
                for (i, t) in tracks.iter().enumerate() {
                    if let Some(f) = t.flex_factor() {
                        if !frozen[i] {
                            sizes[i] = hypothetical_fr * f.max(0.0);
                        }
                    }
                }
                break;
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
        remaining = super::spanning::expand_bounded_tracks(&mut sizes, &ceilings, &bounded, remaining);
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
    row_baselines: &[Option<super::baseline::TrackBaseline>],
    ctx: &LayoutCtx,
) -> (Vec<f32>, Vec<f32>) {
    let mut content_row_h = vec![0.0f32; nrows];
    let mut min_row_h = vec![0.0f32; nrows];
    let mut multi_row_items = Vec::new();
    for cell in cells {
        let is_row_sub = super::subgrid::is_subgrid_node(dom, cell.child, wm).1;
        if is_row_sub {
            continue;
        }
        let cw = col_sub
            .map(|s| s.cell_span(cell.c0, cell.c1))
            .unwrap_or_else(|| super::grid::span_size(col_sizes, cell.c0, cell.c1, col_gap));
        let is_rep = matches!(&dom.node(cell.child).kind, crate::dom::NodeKind::Element { tag } if matches!(tag.as_str(), "img" | "canvas" | "svg"));
        let is_ortho_frame = crate::layout::block::rotated::is_ortho_in_frame(cell.child);
        let stretch_x = !is_ortho_frame && dom.computed_style_idx(cell.child).is_some_and(|c| {
            let j = c.justify_self.unwrap_or(crate::style::AlignItems::Normal);
            let s = match j {
                crate::style::AlignItems::Stretch => true,
                crate::style::AlignItems::Normal => !is_rep,
                _ => false,
            };
            let is_auto_w = c.width.is_none() || matches!(c.width, Some(crate::style::Dimension::Auto));
            s && is_auto_w && !c.margin.left.is_auto() && !c.margin.right.is_auto()
        });
        let forced_w = if stretch_x { Some(cw) } else { None };
        let area_h = if is_ortho_frame { Some(cw) } else {
            super::aspect::definite_area_height(explicit_rows, auto_row, (cell.r0, cell.r1), row_gap, container_content_h, resolve)
        };
        let avail_w = if is_ortho_frame { f32::INFINITY } else { cw };
        let (_bw, bh) = {
            let _guard = crate::layout::replaced::ForcedReplacedScope::enter(forced_w, None);
            measure_block(dom, cell.child, cell.box_id, avail_w, area_h, forced_w, None, !stretch_x, ctx)
        };
        let h = bh;
        let item_css = dom.computed_style_idx(cell.child);
        let is_ortho_item = is_ortho_frame || item_css.as_ref().map_or(false, |c| crate::layout::block::rotated::opens_frame(dom, cell.child, c));
        let mh = if is_ortho_item && item_css.as_ref().map_or(true, |c| c.min_height.is_none() && c.height.is_none() && c.max_height.is_none()) {
            measure_block(dom, cell.child, cell.box_id, cw, Some(0.0), None, None, true, ctx).1
        } else if is_ortho_frame {
            measure_block(dom, cell.child, cell.box_id, 0.0, area_h, None, None, true, ctx).1
        } else {
            super::aspect::grid_item_min_content_height(dom, cell.child, h, cw, resolve.node_font_size, resolve, ctx)
        };
        let min_sz = super::clamp::clamp_item_min_h(mh, dom, cell.child, explicit_rows, cell.r0, cell.r1, row_gap, container_content_h, resolve);
        if cell.rows() == 1 && cell.r0 < nrows {
            content_row_h[cell.r0] = content_row_h[cell.r0].max(h);
            min_row_h[cell.r0] = min_row_h[cell.r0].max(min_sz);
        } else if cell.rows() > 1 {
            multi_row_items.push(super::spanning::SpanningItem {
                start: cell.r0,
                end: cell.r1,
                min_size: min_sz,
                min_content: mh,
                max_content: h,
            });
        }
    }
    let mut had_single_row = vec![false; nrows];
    for cell in cells {
        if cell.rows() == 1 && cell.r0 < nrows && !super::subgrid::is_subgrid_node(dom, cell.child, wm).1 {
            had_single_row[cell.r0] = true;
        }
    }
    let row_tracks: Vec<crate::style::GridTrack> = (0..nrows)
        .map(|r| {
            explicit_rows
                .get(r)
                .cloned()
                .or_else(|| auto_row.map(|t| t.track_at(r.saturating_sub(explicit_rows.len()))).cloned())
                .unwrap_or(crate::style::GridTrack::Auto)
        })
        .collect();
    multi_row_items.sort_by_key(|i| i.end - i.start);
    super::baseline::apply_baseline_sizes(&mut content_row_h, row_baselines);
    super::spanning::distribute_spanning_tracks(
        &row_tracks,
        &multi_row_items,
        nrows,
        row_gap,
        container_content_h,
        resolve,
        &had_single_row,
        &mut content_row_h,
        &mut min_row_h,
    );
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
    mnh: Option<f32>,
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
                    let base = mn;
                    ch.max(base).min(mx.max(base))
                }
                Some(T::Fr(_)) => 0.0,
                Some(T::Flex { min, .. }) => match min {
                    B::Fixed(d) => resolve_height(Some(*d), container_h, resolve).unwrap_or(0.0),
                    B::MinContent | B::Auto => min_row_h.get(r).copied().unwrap_or(0.0),
                    B::MaxContent => content_row_h.get(r).copied().unwrap_or(0.0),
                    B::FitContent(d) => resolve_height(Some(*d), container_h, resolve).unwrap_or(ch),
                },
                _ => ch,
            }
        })
        .collect();

    let total_gap = super::collapse::gutters(nrows, row_collapsed, row_gap);
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
    if let Some(ch) = container_h {
        let current_used: f32 = row_sizes.iter().sum::<f32>() + total_gap;
        if current_used > ch {
            let overflow = current_used - ch;
            let auto_tracks: Vec<usize> = (0..nrows)
                .filter(|&r| matches!(row_track(r), Some(T::Auto | T::Intrinsic { max: B::FitContent(_), .. }) | None))
                .collect();
            let min_sizes: Vec<f32> = (0..nrows).map(min_of_row).collect();
            super::spanning::reduce_tracks_overflow(&mut row_sizes, &min_sizes, &auto_tracks, overflow);
        }
    }

    let sum_fr: f32 = (0..nrows).filter_map(|r| row_track(r).and_then(|t| t.flex_factor()).map(|f| f.max(0.0))).sum();
    if let Some(ch) = container_h.or(mnh) {
        if sum_fr > 0.0 {
            let non_fr: f32 = (0..nrows).filter(|&r| row_track(r).and_then(|t| t.flex_factor()).is_none()).map(|r| row_sizes[r]).sum::<f32>() + total_gap;
            let free = (ch - non_fr).max(0.0);
            let flex_sum = sum_fr.max(1.0);
            for r in 0..nrows {
                if let Some(f) = row_track(r).and_then(|t| t.flex_factor()) { row_sizes[r] = free * f.max(0.0) / flex_sum; }
            }
        } else {
            let current_used: f32 = row_sizes.iter().sum::<f32>() + total_gap;
            if ch > current_used {
                let free = ch - current_used;
                let bounded_indices: Vec<usize> = (0..nrows)
                    .filter(|&r| matches!(row_track(r), Some(T::Bounded { .. } | T::Intrinsic { .. })))
                    .collect();
                let ceilings: Vec<f32> = bounded_indices
                    .iter()
                    .map(|&r| match row_track(r) {
                        Some(T::Bounded { max, .. }) => resolve_height(Some(*max), container_h, resolve).unwrap_or(f32::INFINITY),
                        Some(T::Intrinsic { max, min }) => {
                            let c = match max {
                                B::Fixed(d) => resolve_height(Some(*d), container_h, resolve).unwrap_or(row_sizes[r]),
                                B::MinContent => min_row_h.get(r).copied().unwrap_or(0.0),
                                B::MaxContent | B::Auto => content_row_h.get(r).copied().unwrap_or(row_sizes[r]),
                                B::FitContent(d) => {
                                    let lim = resolve_height(Some(*d), container_h, resolve).unwrap_or(row_sizes[r]);
                                    let mn = min_row_h.get(r).copied().unwrap_or(0.0);
                                    let mx = content_row_h.get(r).copied().unwrap_or(row_sizes[r]).max(mn);
                                    lim.max(mn).min(mx)
                                }
                            };
                            if matches!(min, B::Auto) { c } else { c.max(row_sizes[r]) }
                        }
                        _ => unreachable!(),
                    })
                    .collect();
                let remaining = super::spanning::expand_bounded_tracks(&mut row_sizes, &ceilings, &bounded_indices, free);
                let stretch = align_content.map_or(true, |a| matches!(a, crate::style::JustifyContent::Stretch));
                if stretch && remaining > 0.0 {
                    let auto_rows: Vec<usize> = (0..nrows)
                        .filter(|&r| !row_collapsed.get(r).copied().unwrap_or(false) && matches!(row_track(r), None | Some(T::Auto | T::Intrinsic { max: B::Auto, .. })))
                        .collect();
                    if !auto_rows.is_empty() {
                        let each = remaining / auto_rows.len() as f32;
                        for r in auto_rows { row_sizes[r] += each; }
                    }
                }
            }
        }
    } else if align_content.map_or(true, |a| matches!(a, crate::style::JustifyContent::Stretch)) {
            let mut max_fr_share = 0.0f32;
            for r in 0..nrows {
                if let Some(t) = row_track(r) {
                    if let Some(f) = t.flex_factor() {
                        if f > 0.0 {
                            let ch = content_row_h.get(r).copied().unwrap_or(0.0);
                            max_fr_share = max_fr_share.max(ch / f.max(1.0));
                        }
                    }
                }
            }
            if max_fr_share > 0.0 {
                for r in 0..nrows {
                    if let Some(t) = row_track(r) {
                        if let Some(f) = t.flex_factor() {
                            row_sizes[r] = row_sizes[r].max(max_fr_share * f);
                        }
                    }
                }
                let total_gap = super::collapse::gutters(nrows, row_collapsed, row_gap);
                let container_fixed_h = row_sizes.iter().sum::<f32>() + total_gap;
                let sum_fr: f32 = (0..nrows).filter_map(|r| row_track(r).and_then(|t| t.flex_factor()).map(|f| f.max(0.0))).sum();
                let non_fr: f32 = (0..nrows).filter(|&r| row_track(r).and_then(|t| t.flex_factor()).is_none()).map(|r| row_sizes[r]).sum::<f32>() + total_gap;
                let free = (container_fixed_h - non_fr).max(0.0);
                let flex_sum = sum_fr.max(1.0);
                for r in 0..nrows {
                    if let Some(t) = row_track(r) {
                        if let Some(f) = t.flex_factor() {
                            row_sizes[r] = free * f.max(0.0) / flex_sum;
                        }
                    }
                }
            }
            let has_pct_row = (0..nrows).any(|r| match row_track(r) {
                Some(T::Fixed(crate::style::Dimension::Percent(_))) => true,
                _ => false,
            });
            if has_pct_row {
                let total_gap = super::collapse::gutters(nrows, row_collapsed, row_gap);
                let container_fixed_h = row_sizes.iter().sum::<f32>() + total_gap;
                let mut used = total_gap;
                let mut auto_rows = Vec::new();
                for r in 0..nrows {
                    match row_track(r) {
                        Some(T::Fixed(crate::style::Dimension::Percent(pct))) => {
                            row_sizes[r] = (container_fixed_h * pct / 100.0).max(0.0);
                            used += row_sizes[r];
                        }
                        Some(T::Auto | T::Intrinsic { max: B::Auto, .. }) | None => {
                            auto_rows.push(r);
                        }
                        _ => {
                            used += row_sizes[r];
                        }
                    }
                }
                if !auto_rows.is_empty() && container_fixed_h > used {
                    let each = (container_fixed_h - used) / auto_rows.len() as f32;
                    for r in auto_rows {
                        row_sizes[r] = each;
                    }
            }
        }
    }
    row_sizes
}
