//! GRID layout: track sizing, placement, alignment.
use super::abspos;
use super::align::cell_align_offset;
pub(in crate::layout) use super::align::span_size;
use super::aspect;
use super::collapse;
use super::lines::{collect_items, place_grid_items, GridItem};
use super::subgrid;
use super::tracks;
use super::*;

#[allow(clippy::too_many_arguments)]
pub(in crate::layout) fn layout_children_grid(
    dom: &Dom,
    id: NodeIdx,
    container: crate::boxes::BoxId,
    content_x: f32,
    content_y: f32,
    content_w: f32,
    container_content_h: Option<f32>,
    css: &ComputedStyle,
    font_size: f32,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) -> (f32, f32) {
    let resolve = ResolveCtx {
        parent_content_w: content_w,
        node_font_size: font_size,
        root_font_size: crate::style::root_font_size(),
        viewport_w: ctx.viewport_w,
        viewport_h: ctx.viewport_h,
    };
    let subgrid_ctx = subgrid::take_subgrid(container);
    let col_sub = subgrid_ctx.as_ref().and_then(|s| s.cols.as_ref());
    let row_sub = subgrid_ctx.as_ref().and_then(|s| s.rows.as_ref());

    let col_gap = col_sub.map(|s| s.gap).unwrap_or_else(|| css.gap.and_then(|d| d.resolve(&resolve)).unwrap_or(0.0).max(0.0));
    let row_gap = row_sub.map(|s| s.gap).unwrap_or_else(|| css.row_gap.and_then(|d| d.resolve(&resolve)).unwrap_or(0.0).max(0.0));

    let areas = css.grid_template_areas.clone();
    let container_content_h = css.height.is_some().then_some(container_content_h).flatten();
    let is_p_grid = subgrid::is_parent_grid(dom, id);
    let (col_tracks, col_collapsible, col_lines) =
        subgrid::init_col_tracks(col_sub, css, areas.as_deref(), content_w, col_gap, container_content_h, &resolve, is_p_grid);
    let explicit_cols_n = col_tracks.len();

    let tree = std::rc::Rc::clone(&list.tree);
    let children: Vec<GridItem> = collect_items(dom, &tree, container);
    let (explicit_rows, row_collapsible, row_lines) =
        subgrid::init_row_tracks(row_sub, css, container_content_h, row_gap, &resolve, is_p_grid);
    let has_oof = tree
        .children_without_generated(container)
        .iter()
        .any(|&b| tree.node_of(b).map_or(false, |n| is_out_of_flow(dom, n)));
    if children.is_empty() && !has_oof {
        return aspect::empty_grid_dimensions(
            &col_tracks, &explicit_rows, col_gap, row_gap, content_w, container_content_h, css, &resolve,
        );
    }
    let explicit_rows_n = explicit_rows.len();
    let auto_flow = css.grid_auto_flow.unwrap_or(crate::style::grid_lines::GridAutoFlow {
        column: false,
        dense: false,
    });
    let (mut cells, ncols_colocados) = place_grid_items(
        dom,
        &children,
        areas.as_deref(),
        &col_lines,
        &row_lines,
        explicit_cols_n,
        explicit_rows_n,
        auto_flow,
        col_sub.is_some(),
        row_sub.is_some(),
    );
    let mut col_tracks = col_tracks;
    let mut col_collapsible = col_collapsible;
    if col_sub.is_none() {
        while col_tracks.len() < ncols_colocados {
            let idx = col_tracks.len().saturating_sub(explicit_cols_n);
            let tr = css.grid_auto_columns.as_ref()
                .map_or(crate::style::GridTrack::Auto, |t| t.track_at(idx).clone());
            col_tracks.push(tr);
            col_collapsible.push(false);
        }
    }
    let ncols = col_tracks.len().max(1);
    if col_sub.is_some() {
        for c in &mut cells {
            c.c0 = c.c0.min(ncols.saturating_sub(1));
            c.c1 = c.c1.min(ncols).max(c.c0 + 1);
        }
    }
    // `auto-fit`: as repetições sem NENHUM item colapsam, e só agora — depois
    // da colocação — se sabe quais são. Ver `grid_colapso`.
    let col_collapsed =
        collapse::collapsed(&col_collapsible, ncols, cells.iter().map(|c| (c.c0, c.c1)));
    collapse::as_fixed_zero(&mut col_tracks, &col_collapsed);

    // A largura INTRÍNSECA por coluna — só medida quando alguma trilha PEDE
    // conteúdo (`Auto` ou `Intrinsic`, que pode precisar do min-content além
    // do max-content), porque medir custa uma travessia por item e a
    // esmagadora maioria das grades é só `fr` e px.
    let needs_min = col_tracks
        .iter()
        .any(|t| matches!(t, crate::style::GridTrack::Intrinsic { .. }));
    let needs_measure = needs_min
        || col_tracks
            .iter()
            .any(|t| matches!(t, crate::style::GridTrack::Auto));
    let (content_max, content_min): (Option<Vec<f32>>, Option<Vec<f32>>) = if needs_measure {
        let mut wmax = vec![0.0f32; ncols];
        let mut wmin = vec![0.0f32; ncols];
        let item_w = |c: &lines::GridCell| {
            let area_h = aspect::definite_area_height(
                &explicit_rows, css.grid_auto_rows.as_ref(), (c.r0, c.r1), row_gap, container_content_h, &resolve,
            );
            aspect::grid_item_outer_width(dom, c.child, c.box_id, area_h, content_w, css, font_size, ctx)
        };
        for c in &cells {
            if c.c1 - c.c0 == 1 && c.c0 < ncols {
                let w = item_w(c);
                wmax[c.c0] = wmax[c.c0].max(w);
                if needs_min {
                    let min = if aspect::is_item_orthogonal(dom, c.child, css) {
                        w
                    } else {
                        aspect::grid_item_min_content_width(dom, c.child, Some(c.box_id), None, Some(css), font_size, &resolve, ctx)
                    };
                    wmin[c.c0] = wmin[c.c0].max(min);
                }
            }
        }
        tracks::distribute_spanning_columns(
            &col_tracks,
            &cells,
            ncols,
            col_gap,
            &resolve,
            |c| {
                dom.computed_style_idx(c.child).is_some_and(|c| {
                    subgrid::is_subgrid_item(&c, css.writing_mode.unwrap_or_default()).0
                })
            },
            item_w,
            &mut wmax,
        );
        subgrid::collect_column_contributions(
            dom,
            &tree,
            &cells,
            ncols,
            needs_min,
            font_size,
            col_gap,
            css.writing_mode.unwrap_or_default(),
            ctx,
            &col_lines,
            areas.as_deref(),
            &col_tracks,
            &resolve,
            &mut wmax,
            &mut wmin,
        );
        (Some(wmax), needs_min.then_some(wmin))
    } else {
        (None, None)
    };
    let col_sizes = if let Some(cs) = col_sub {
        cs.track_sizes.clone()
    } else {
        tracks::resolve_tracks(
            &col_tracks,
            content_w,
            collapse::gutters(ncols, &col_collapsed, col_gap),
            content_max.as_deref(),
            content_min.as_deref(),
            css.justify.is_none(),
            &resolve,
        )
    };
    // O computed style do Blink pode consultar o LayoutObject para propriedades
    // dependentes de used values. Guardamos a mesma resolução no container para o
    // DOM a serializar sem executar um segundo algoritmo de track sizing.
    if css.grid_template_columns.is_some() {
        list.grid_column_tracks.insert(id, col_sizes.clone());
    }
    let (row_sizes, nrows, row_collapsed) = if let Some(rs) = row_sub {
        let nr = rs.track_sizes.len().max(1);
        (rs.track_sizes.clone(), nr, vec![false; nr])
    } else {
        let nrows = cells
            .iter()
            .map(|c| c.r1)
            .max()
            .unwrap_or(1)
            .max(areas.as_ref().map(|a| a.rows).unwrap_or(0))
            .max(explicit_rows.len())
            .max(1);
        let row_collapsed =
            collapse::collapsed(&row_collapsible, nrows, cells.iter().map(|c| (c.r0, c.r1)));
        let mut explicit_rows = explicit_rows.clone();
        collapse::as_fixed_zero(&mut explicit_rows, &row_collapsed);
        subgrid::register_col_subgrids_for_measure(
            dom,
            &cells,
            &col_sizes,
            col_gap,
            row_gap,
            &col_lines,
            areas.as_deref(),
            &resolve,
            css.writing_mode.unwrap_or_default(),
            col_sub.is_some(),
        );
        let (mut content_row_h, min_row_h) = tracks::measure_content_rows(
            dom,
            &cells,
            &explicit_rows,
            css.grid_auto_rows.as_ref(),
            &col_sizes,
            col_sub,
            col_gap,
            row_gap,
            nrows,
            container_content_h,
            &resolve,
            css.writing_mode.unwrap_or_default(),
            ctx,
        );
        subgrid::collect_row_contributions(
            dom,
            &tree,
            &cells,
            &col_sizes,
            col_gap,
            nrows,
            font_size,
            row_gap,
            css.writing_mode.unwrap_or_default(),
            ctx,
            &row_lines,
            areas.as_deref(),
            &explicit_rows,
            css.grid_auto_rows.as_ref(),
            &resolve,
            container_content_h,
            &mut content_row_h,
        );
        let row_sizes = tracks::resolve_row_tracks(
            nrows,
            &explicit_rows,
            css.grid_auto_rows.as_ref(),
            &content_row_h,
            &min_row_h,
            container_content_h,
            &resolve,
            css.align_content,
            &row_collapsed,
            row_gap,
        );
        (row_sizes, nrows, row_collapsed)
    };

    if row_sub.is_some() {
        for c in &mut cells {
            c.r0 = c.r0.min(nrows.saturating_sub(1));
            c.r1 = c.r1.min(nrows).max(c.r0 + 1);
        }
    }

    // ── POSICIONA cada item na sua célula ────────────────────────────────────────
    let justify = css
        .grid_justify_items
        .unwrap_or(crate::style::AlignItems::Normal);
    let align = css.align_items.unwrap_or(crate::style::AlignItems::Normal);
    // O início de cada coluna e de cada linha, com `justify-content` /
    // `align-content` já embutidos — o mesmo `justify_offsets` do flex (a spec
    // §8.4 partilha o vocabulário). `justify-content` ausente é `start`.
    let col_collapsed = if col_sub.is_some() {
        vec![false; ncols]
    } else {
        col_collapsed
    };
    let is_rtl = css.direction == Some(crate::style::Direction::Rtl);
    let mut col_x = if col_sub.is_none() {
        collapse::starts(
            &col_sizes,
            &col_collapsed,
            content_x,
            col_gap,
            css.justify.map(|j| (j, content_w)),
        )
    } else {
        Vec::new()
    };
    if col_sub.is_none() && is_rtl {
        let used =
            col_sizes.iter().sum::<f32>() + collapse::gutters(ncols, &col_collapsed, col_gap);
        let right_edge = content_x + if content_w > 0.0 { content_w } else { used };
        for (i, x) in col_x.iter_mut().enumerate().take(col_sizes.len()) {
            let orig_offset = *x - content_x;
            *x = right_edge - orig_offset - col_sizes[i];
        }
    }
    let row_y = if row_sub.is_none() {
        collapse::starts(
            &row_sizes,
            &row_collapsed,
            content_y,
            row_gap,
            css.align_content.zip(container_content_h),
        )
    } else {
        Vec::new()
    };
    for cell in &cells {
        let child = cell.child;
        let (cell_x, cell_w) = if let Some(cs) = col_sub {
            let (rel_x, w) = cs.cell_pos_and_span(cell.c0, cell.c1);
            (content_x + rel_x, w)
        } else {
            let c0 = if is_rtl {
                cell.c1.saturating_sub(1).min(col_x.len().saturating_sub(1))
            } else {
                cell.c0.min(col_x.len().saturating_sub(1))
            };
            (col_x[c0], span_size(&col_sizes, cell.c0, cell.c1, col_gap))
        };
        let (cell_y, cell_h) = if let Some(rs) = row_sub {
            let (rel_y, h) = rs.cell_pos_and_span(cell.r0, cell.r1.min(nrows));
            (content_y + rel_y, h)
        } else {
            let r0 = cell.r0.min(row_y.len().saturating_sub(1));
            (
                row_y[r0],
                span_size(&row_sizes, cell.r0, cell.r1.min(nrows), row_gap),
            )
        };
        // `justify-self`/`align-self` do ITEM vencem `justify-items`/`align-items`
        // do container — mesma prioridade de `align-self` no flex.
        let item_css = dom.computed_style_idx(child).unwrap_or_default();
        let (is_col_sub, is_row_sub) =
            subgrid::is_subgrid_item(&item_css, css.writing_mode.unwrap_or_default());
        let area_h = aspect::definite_area_height(&explicit_rows, css.grid_auto_rows.as_ref(), (cell.r0, cell.r1), row_gap, container_content_h, &resolve);
        let ratio_w = aspect::ratio_outer_width(dom, child, area_h, cell_w, font_size, ctx);
        let is_rep = matches!(&dom.node(child).kind, crate::dom::NodeKind::Element { tag } if matches!(tag.as_str(), "img" | "canvas" | "svg"));
        let has_ratio = ratio_w.is_some() || dom.image_dims(child).is_some() || is_rep;
        let resolve_align = |val: crate::style::AlignItems| match val {
            crate::style::AlignItems::Normal => if has_ratio { crate::style::AlignItems::FlexStart } else { crate::style::AlignItems::Stretch },
            other => other,
        };
        let justify = if is_col_sub { crate::style::AlignItems::Stretch } else { resolve_align(item_css.justify_self.unwrap_or(justify)) };
        let align = if is_row_sub { crate::style::AlignItems::Stretch } else { resolve_align(item_css.align_self.unwrap_or(align)) };
        let is_stretch = |a: crate::style::AlignItems| matches!(a, crate::style::AlignItems::Stretch);
        let stretch_x = is_col_sub || (is_stretch(justify) && item_css.width.is_none());
        let ratio_w = ratio_w.filter(|_| !stretch_x);
        let stretch_y = is_row_sub || (is_stretch(align) && item_css.height.is_none());
        let use_area_h = area_h.or(Some(cell_h));
        let (nat_w, nat_h) = measure_block(
            dom, child, cell.box_id, cell_w, use_area_h, ratio_w, None, true, ctx,
        );
        use crate::scrollbar::Overflow::{Clip, Visible};
        let axis_vis = |o: Option<crate::scrollbar::Overflow>| matches!(o.unwrap_or(Visible), Visible | Clip);
        let overflow_visible = axis_vis(item_css.overflow_x) && axis_vis(item_css.overflow_y);
        let def_px = |d| match d { Some(crate::style::Dimension::Px(p)) => Some(p), _ => None };
        let clamp_auto = |mut base: f32, sz, mx| {
            if let Some(p) = def_px(sz) { base = base.min(p); }
            if let Some(m) = def_px(mx) { base = base.min(m); }
            base
        };
        let iw = match ratio_w {
            Some(w) => w,
            None if stretch_x => cell_w,
            None if item_css.width == Some(crate::style::Dimension::MinContent) => {
                crate::table::min_content(dom, child, font_size, ctx).min(cell_w)
            }
            None if item_css.min_width.is_none() && overflow_visible => {
                let cnt = crate::table::min_content_with_floor(dom, child, font_size, ctx);
                let auto = aspect::resolve_specified_suggestion(item_css.width, cnt).unwrap_or(cnt);
                nat_w.max(clamp_auto(auto, item_css.width, item_css.max_width))
            }
            None => nat_w,
        };
        let ih = if stretch_y {
            cell_h
        } else if item_css.min_height.is_none() && overflow_visible {
            let unc_h = measure_block(dom, child, cell.box_id, cell_w, None, ratio_w, None, true, ctx).1;
            let auto = aspect::resolve_specified_suggestion(item_css.height, unc_h).unwrap_or(unc_h);
            nat_h.max(clamp_auto(auto, item_css.height, item_css.max_height))
        } else {
            nat_h
        };
        let x = cell_x + cell_align_offset(justify, cell_w, iw);
        let y = cell_y + cell_align_offset(align, cell_h, ih);
        let forced_w = if is_col_sub {
            Some(cell_w)
        } else if stretch_x {
            let rep = matches!(&dom.node(child).kind, crate::dom::NodeKind::Element { tag } if matches!(tag.as_str(), "img" | "canvas" | "svg"));
            (rep || crate::layout::block::rotated::opens_frame(dom, child, &item_css) || crate::layout::block::rotated::is_ortho_in_frame(child)).then_some(cell_w)
        } else {
            Some(iw)
        };
        let forced_h = if is_row_sub || stretch_y {
            Some(cell_h)
        } else {
            Some(ih)
        };
        if is_col_sub || is_row_sub {
            subgrid::register_if_subgrid(
                dom,
                child,
                cell,
                &col_sizes,
                &row_sizes,
                col_gap,
                row_gap,
                &col_lines,
                &row_lines,
                areas.as_deref(),
                &resolve,
                col_sub.is_some(),
                row_sub.is_some(),
            );
        }
        layout_block_reusing(
            dom,
            child,
            cell.box_id,
            x,
            y,
            cell_w,
            use_area_h,
            || (0.0, 0.0),
            forced_w,
            forced_h,
            is_row_sub,
            !stretch_x,
            &BlockFormattingContext::new(),
            ctx,
            list,
        );
    }
    subgrid::cleanup_subgrids(&cells);
    let total_w = col_sub
        .map(|cs| cs.content_span())
        .unwrap_or_else(|| {
            col_sizes.iter().sum::<f32>() + collapse::gutters(ncols, &col_collapsed, col_gap)
        })
        .max(0.0);
    let total_h = row_sub
        .map(|rs| rs.content_span())
        .unwrap_or_else(|| {
            row_sizes.iter().sum::<f32>() + collapse::gutters(nrows, &row_collapsed, row_gap)
        })
        .max(0.0);
    if has_oof {
        abspos::record_out_of_flow(
            dom,
            &tree,
            container,
            css,
            &resolve,
            content_x,
            content_y,
            total_w,
            total_h,
            is_rtl,
            col_sub,
            row_sub,
            &col_x,
            &row_y,
            &col_sizes,
            &row_sizes,
            &col_lines,
            &row_lines,
            areas.as_deref(),
            ncols,
            nrows,
            list,
        );
    }
    (total_w, total_h)
}
