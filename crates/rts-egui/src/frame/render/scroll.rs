use super::*;


/// Processa os SCROLL CONTAINERS internos (#1744): para cada `ScrollRegion`, lê/
/// atualiza o offset (roda do mouse quando o ponteiro está sobre a div), injeta esse
/// offset no `BeginClip` correspondente (p/ o paint transladar os filhos) e emite as
/// barras (x/y) DENTRO da região via `emit_scrollbar_in`. `page_dy` é a translação do
/// scroll da página (p/ posicionar a região na tela). Egui burro: só input + dados.
pub(in crate::frame::render) fn process_scroll_regions(
    ui: &mut egui::Ui,
    h: u64,
    list: &mut layout::DisplayList,
    sb: &rts_dom::scrollbar::ScrollbarStyle,
    page_dy: f32,
) {
    // As regiões roláveis podem ter vindo de uma subárvore REUSADA: a lista
    // guarda as próprias, e a `geometry()` junta as das subárvores.
    let geometria = list.geometry();
    if geometria.scroll_regions.is_empty() {
        return;
    }
    let base = ui.max_rect().min;
    let regions = geometria.scroll_regions.clone();
    for region in &regions {
        let max_x = (region.content_w - region.visible.w).max(0.0);
        let max_y = (region.content_h - region.visible.h).max(0.0);
        let can_x = region.overflow_x.scrollable() && max_x > 0.0;
        let can_y = region.overflow_y.scrollable() && max_y > 0.0;
        if !can_x && !can_y {
            continue;
        }
        // offset por-nó em memory.
        let oid = egui::Id::new(("rts_dom_region", h, region.node_idx));
        let mut off = ui.ctx().memory(|m| m.data.get_temp::<egui::Vec2>(oid).unwrap_or_default());
        // rect da região na TELA (visible + page scroll).
        let screen = egui::Rect::from_min_size(
            base + egui::vec2(region.visible.x, region.visible.y + page_dy),
            egui::vec2(region.visible.w, region.visible.h),
        );
        if ui.rect_contains_pointer(screen) {
            let d = ui.input(|i| i.smooth_scroll_delta);
            // se rola Y, a roda move Y; se SÓ rola X, a roda (Y) move X (UX comum).
            if can_y {
                off.y -= d.y;
            }
            if can_x {
                off.x -= if can_y { d.x } else { d.y };
            }
        }

        // ARRASTAR as barras da DIV (clicar e puxar). Geometria igual à emit_scrollbar_in:
        // barra-Y na borda direita, barra-X na borda inferior. A posição do mouse na
        // faixa da barra → fração → offset.
        let bar_w = match sb.width {
            Some(rts_dom::scrollbar::BarWidth::Thin) => 8.0,
            Some(rts_dom::scrollbar::BarWidth::Px(px)) => px,
            _ => 12.0,
        };
        let v = region.visible;
        let sx = base.x + v.x;
        let sy = base.y + v.y + page_dy;
        if can_y {
            let track_h = if can_x { v.h - bar_w } else { v.h };
            let bar_rect = egui::Rect::from_min_size(
                egui::pos2(sx + v.w - bar_w, sy),
                egui::vec2(bar_w, track_h),
            );
            let resp = ui.interact(bar_rect, oid.with("bar_y"), egui::Sense::click_and_drag());
            if let Some(p) = resp.interact_pointer_pos() {
                if resp.is_pointer_button_down_on() || resp.dragged() {
                    let frac = (track_h / region.content_h).clamp(0.0, 1.0);
                    let thumb_h = (track_h * frac).max(24.0);
                    let local = (p.y - sy - thumb_h / 2.0).max(0.0);
                    off.y = (local / (track_h - thumb_h).max(1.0)).clamp(0.0, 1.0) * max_y;
                }
            }
        }
        if can_x {
            let track_w = if can_y { v.w - bar_w } else { v.w };
            let bar_rect = egui::Rect::from_min_size(
                egui::pos2(sx, sy + v.h - bar_w),
                egui::vec2(track_w, bar_w),
            );
            let resp = ui.interact(bar_rect, oid.with("bar_x"), egui::Sense::click_and_drag());
            if let Some(p) = resp.interact_pointer_pos() {
                if resp.is_pointer_button_down_on() || resp.dragged() {
                    let frac = (track_w / region.content_w).clamp(0.0, 1.0);
                    let thumb_w = (track_w * frac).max(24.0);
                    let local = (p.x - sx - thumb_w / 2.0).max(0.0);
                    off.x = (local / (track_w - thumb_w).max(1.0)).clamp(0.0, 1.0) * max_x;
                }
            }
        }
        off.x = off.x.clamp(0.0, max_x);
        off.y = off.y.clamp(0.0, max_y);
        ui.ctx().memory_mut(|m| m.data.insert_temp(oid, off));

        // injeta o offset no BeginClip desta região (acha pelo node).
        // Mutar um item exige a lista PLANA: um `BeginClip` pode estar dentro de
        // uma subárvore compartilhada, e escrever nele afetaria todos os nós que
        // a reusam.
        list.materialize();
        for it in list.items.iter_mut() {
            if let layout::DisplayItem::BeginClip { node, offset_x, offset_y, .. } = it {
                if *node == region.node_idx {
                    *offset_x = off.x;
                    *offset_y = off.y;
                    break;
                }
            }
        }
        // barras DENTRO da região (coords de conteúdo; o paint soma o page scroll).
        layout::emit_scrollbar_in(list, region, off.x, off.y, sb);
    }
}
