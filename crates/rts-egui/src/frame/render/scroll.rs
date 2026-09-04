use super::*;


/// Processa os SCROLL CONTAINERS internos (#1744): para cada `ScrollRegion`, lê/
/// atualiza o offset (roda do mouse quando o ponteiro está sobre a div) e emite as
/// barras (x/y) DENTRO da região via `emit_scrollbar_in`. `page_dy` é a translação do
/// scroll da página (p/ posicionar a região na tela). Egui burro: só input + dados.
///
/// O offset vive no `Dom` (`dom/scroll.rs`, finding 3 da auditoria estrutural) —
/// não mais em `ui.ctx().memory()`, e este backend NÃO injeta mais o offset num
/// `BeginClip` da `DisplayList`: `paint_list` (e o hit-test) voltam a perguntar
/// ao `Dom` o valor VIVO no momento em que precisam, então mutar a lista aqui
/// escreveria um valor que ninguém mais lê. A única razão de ainda calcular
/// `off` localmente é dar à BARRA (`emit_scrollbar_in`) o valor já CLAMPADO
/// deste frame, sem um segundo `with_dom` só para reler o que acabou de gravar.
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
        // `oid` continua a existir só como identidade de INTERAÇÃO das barras
        // (`ui.interact` abaixo) — o offset em si já não mora no egui.
        let oid = egui::Id::new(("rts_dom_region", h, region.node_idx));
        let (ox, oy) = rts_dom::store::with_dom(h, |d| d.scroll_of_idx(region.node_idx))
            .unwrap_or((0.0, 0.0));
        let mut off = egui::vec2(ox, oy);
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
        // Escreve de volta no `Dom` — só em resposta a este input (roda/drag),
        // nunca guardado "para si". `_extent`: o teto (`max_x`/`max_y`) já
        // veio da MESMA `list` que este frame vai pintar (o medidor REAL), e
        // pedir um layout próprio aqui só para clampar pagaria o documento
        // inteiro a cada tick da roda do rato (ver a nota de topo de
        // `dom/scroll.rs`). Dispara o evento "scroll" quando o valor muda.
        let _ = rts_dom::store::with_dom_mut(h, |d| {
            d.set_scroll_extent_idx(region.node_idx, off.x, off.y, max_x, max_y)
        });
        // barras DENTRO da região (coords de conteúdo; o paint soma o page scroll).
        layout::emit_scrollbar_in(list, region, off.x, off.y, sb);
    }
}
