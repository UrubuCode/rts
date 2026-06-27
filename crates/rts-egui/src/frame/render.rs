//! Render do DOM — agora SÓ PAINT. O LAYOUT (geometria x/y/w/h de cada nó) é
//! calculado pelo `rts-dom` (`rts_dom::layout`), que devolve uma `DisplayList`
//! plana; aqui apenas PERCORREMOS essa lista e pintamos via `ui.painter()`.
//!
//! Esta é a virada de 2026-06-27 ("processar tudo no DOM, o egui só lê e exibe").
//! O egui deixou de decidir layout (o antigo `ui.label`/`horizontal`/`Frame` foi
//! removido) — ele é um BACKEND DE PAINT trocável. A única coisa que o `rts-dom`
//! não faz sozinho é MEDIR texto (largura/altura de glifo); isso o egui fornece
//! via [`EguiMeasurer`], que implementa o trait `rts_dom::layout::TextMeasurer`
//! usando o sistema de fontes real do egui (galley) — então a medida é exata, não
//! aproximada, e mesmo assim o DOM continua dono do layout.

use rts_dom::layout::{self, DisplayItem, DisplayList, TextMeasurer};

/// Converte a cor própria do motor de estilo (`u32` RGBA `0xRRGGBBAA`, egui-free)
/// para o `Color32` do egui. A conversão vive AQUI (no backend), nunca no rts-dom.
fn rgba_to_color32(c: u32) -> egui::Color32 {
    let r = ((c >> 24) & 0xFF) as u8;
    let g = ((c >> 16) & 0xFF) as u8;
    let b = ((c >> 8) & 0xFF) as u8;
    let a = (c & 0xFF) as u8;
    egui::Color32::from_rgba_unmultiplied(r, g, b, a)
}

/// Implementa a medição de texto do `rts-dom` usando o sistema de fontes REAL do
/// egui (não a aproximação do `ApproxMeasurer`). Mede largura via galley e usa a
/// altura de linha da fonte — assim o layout calculado no rts-dom bate com o que
/// o egui vai de fato pintar. Guarda o `Context` para consultar `fonts`.
struct EguiMeasurer<'a> {
    ctx: &'a egui::Context,
}

impl<'a> EguiMeasurer<'a> {
    fn font_id(size: f32, mono: bool) -> egui::FontId {
        let family = if mono { egui::FontFamily::Monospace } else { egui::FontFamily::Proportional };
        egui::FontId::new(size, family)
    }
}

impl<'a> TextMeasurer for EguiMeasurer<'a> {
    fn text_width(&self, text: &str, size: f32, mono: bool) -> f32 {
        let font = Self::font_id(size, mono);
        // `fonts_mut` dá um `&mut FontsView` (glyph_width exige `&mut`).
        self.ctx.fonts_mut(|f| text.chars().map(|c| f.glyph_width(&font, c)).sum())
    }
    fn line_height(&self, size: f32) -> f32 {
        let font = Self::font_id(size, false);
        self.ctx.fonts_mut(|f| f.row_height(&font))
    }
}

/// Renderiza um `Dom` inteiro: calcula o layout (rts-dom) e PINTA a display list.
///
/// A origem do conteúdo é o canto superior-esquerdo da área do `ui`
/// (`ui.max_rect().min`); cada item da lista vem em coordenadas de conteúdo e é
/// transladado por essa origem ao pintar. O `ui` é avançado por `allocate_space`
/// na altura total do conteúdo (para o layout do egui ao redor — scroll, etc —
/// saber o tamanho ocupado).
pub(crate) fn render_dom(ui: &mut egui::Ui, dom: &crate::dom::Dom) {
    let avail = ui.available_size();
    let measurer = EguiMeasurer { ctx: ui.ctx() };
    let ctx = layout::LayoutCtx {
        viewport_w: avail.x.max(1.0),
        viewport_h: ui.ctx().screen_rect().height().max(1.0),
        measurer: &measurer,
    };
    let list = layout::layout_document(dom, &ctx);
    paint_list(ui, &list);
}

/// Percorre a [`DisplayList`] e pinta cada item via `ui.painter()`, em coordenadas
/// absolutas (conteúdo + origem do `ui`). A ordem da lista É o z-order (o que vem
/// depois pinta por cima). Reserva o espaço da altura total para o `ui` pai.
fn paint_list(ui: &mut egui::Ui, list: &DisplayList) {
    let origin = ui.max_rect().min; // canto sup-esq da área de conteúdo
    let painter = ui.painter().clone();
    for item in &list.items {
        match item {
            DisplayItem::SolidRect { rect, color, radius } => {
                let r = egui::Rect::from_min_size(
                    origin + egui::vec2(rect.x, rect.y),
                    egui::vec2(rect.w, rect.h),
                );
                painter.rect_filled(r, egui::CornerRadius::same(*radius as u8), rgba_to_color32(*color));
            }
            DisplayItem::Border { rect, width, color, radius } => {
                let r = egui::Rect::from_min_size(
                    origin + egui::vec2(rect.x, rect.y),
                    egui::vec2(rect.w, rect.h),
                );
                painter.rect_stroke(
                    r,
                    egui::CornerRadius::same(*radius as u8),
                    egui::Stroke::new(*width, rgba_to_color32(*color)),
                    egui::StrokeKind::Inside,
                );
            }
            DisplayItem::Text { x, y, text, color, size, mono } => {
                let family = if *mono { egui::FontFamily::Monospace } else { egui::FontFamily::Proportional };
                painter.text(
                    origin + egui::vec2(*x, *y),
                    egui::Align2::LEFT_TOP,
                    text,
                    egui::FontId::new(*size, family),
                    rgba_to_color32(*color),
                );
            }
        }
    }
    // Reserva o espaço ocupado (para scroll/medida do egui ao redor).
    ui.allocate_space(egui::vec2(ui.available_width(), list.content_height));
}
