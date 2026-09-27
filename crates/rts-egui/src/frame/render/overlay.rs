//! The inspector's highlight, painted over the page — DevTools' hover box.
//!
//! The state is `rts_dom::overlay` (set by the `Overlay` domain in
//! `rts-dom-bridge`); this only paints it. When nothing is highlighted — every
//! frame of a program that never opened the inspector — the cost is one
//! thread-local read (plan `docs/superpowers/plans/2026-09-26-dom-inspector.md`,
//! F2/F7).

use rts_dom::paint::DisplayList;
use rts_dom::query::Geometry;

/// DevTools' content-box highlight colour, `rgba(111, 168, 220, 0.66)`.
const FILL: egui::Color32 = egui::Color32::from_rgba_premultiplied(73, 111, 145, 168);
const EDGE: egui::Color32 = egui::Color32::from_rgb(111, 168, 220);

/// Paints the highlighted node of document `handle`, if there is one, at the
/// same place the page painted it: content coordinates moved by the window
/// origin and the page scroll, as the hit-test converts them the other way.
pub(super) fn paint(
    ui: &egui::Ui,
    handle: u64,
    list: &DisplayList,
    geometry: &Geometry,
    offset: f32,
) {
    let Some(node) = rts_dom::overlay::highlight(handle) else {
        return;
    };
    let Some(idx) = rts_dom::store::with_dom(handle, |dom| dom.resolve(node)).flatten() else {
        return;
    };
    let Some(rect) = list.rect_of_in(geometry, idx) else {
        return;
    };
    let origin = ui.max_rect().min;
    let screen = egui::Rect::from_min_size(
        egui::pos2(origin.x + rect.x, origin.y + rect.y - offset),
        egui::vec2(rect.w, rect.h),
    );
    let painter = ui.painter();
    painter.rect_filled(screen, 0.0, FILL);
    painter.rect_stroke(screen, 0.0, egui::Stroke::new(1.0_f32, EDGE), egui::StrokeKind::Inside);
}
