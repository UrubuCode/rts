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
    /// A família egui p/ (mono, bold): bold vence (família nomeada "bold"); senão
    /// mono → Monospace; senão Proportional. Casa medição com pintura.
    fn family(mono: bool, bold: bool) -> egui::FontFamily {
        if bold {
            egui::FontFamily::Name("bold".into())
        } else if mono {
            egui::FontFamily::Monospace
        } else {
            egui::FontFamily::Proportional
        }
    }
    fn font_id(size: f32, mono: bool, bold: bool) -> egui::FontId {
        egui::FontId::new(size, Self::family(mono, bold))
    }
}

impl<'a> TextMeasurer for EguiMeasurer<'a> {
    fn text_width(&self, text: &str, size: f32, mono: bool, bold: bool) -> f32 {
        let font = Self::font_id(size, mono, bold);
        // `fonts_mut` dá um `&mut FontsView` (glyph_width exige `&mut`).
        self.ctx.fonts_mut(|f| text.chars().map(|c| f.glyph_width(&font, c)).sum())
    }
    fn line_height(&self, size: f32) -> f32 {
        let font = Self::font_id(size, false, false);
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
    paint_list(ui, &list, 0.0);
    // reserva a altura total ocupada (p/ o egui ao redor dimensionar).
    ui.allocate_space(egui::vec2(ui.available_width(), list.content_height));
}

/// Renderiza o DOM COM SCROLL — o egui burro: mantém só o offset (input do mouse),
/// translada o conteúdo por -offset e pinta. A BARRA (track+thumb) é emitida pelo
/// DOM (`layout::emit_scrollbar`) como `SolidRect` — NÃO usa o ScrollArea do egui,
/// p/ a barra não ficar presa ao backend (visão: egui removível). `h` é o handle do
/// DOM; `sb` o estilo do CSS; `scroll_y` se o eixo Y rola; `force` se a barra é
/// sempre visível (overflow:scroll).
pub(crate) fn render_dom_scrolled(
    ui: &mut egui::Ui,
    h: u64,
    sb: &rts_dom::scrollbar::ScrollbarStyle,
    scroll_y: bool,
    force: bool,
) {
    let avail = ui.available_size();
    let viewport_w = avail.x.max(1.0);
    let viewport_h = avail.y.max(1.0);
    // layout (com a barra ainda não — precisa do content_h primeiro).
    let measurer = EguiMeasurer { ctx: ui.ctx() };
    let lctx = layout::LayoutCtx { viewport_w, viewport_h, measurer: &measurer };
    let mut list = rts_dom::store::with_dom(h, |d| layout::layout_document(d, &lctx))
        .unwrap_or_default();
    let content_h = list.content_height;

    // OFFSET de scroll: estado por-handle no egui (input é do backend). Acumula a roda
    // do mouse; limita a [0, content_h - viewport_h].
    let max_off = (content_h - viewport_h).max(0.0);
    let id = egui::Id::new(("rts_dom_scroll", h));
    let mut offset = ui.ctx().memory(|m| m.data.get_temp::<f32>(id).unwrap_or(0.0));
    if scroll_y && (max_off > 0.0 || force) {
        // a roda do mouse só conta quando o ponteiro está sobre a área do DOM.
        let hovered = ui.rect_contains_pointer(ui.max_rect());
        if hovered {
            let dy = ui.input(|i| i.smooth_scroll_delta.y);
            offset -= dy; // roda p/ cima (dy>0) sobe o conteúdo (offset menor)
        }
    }
    offset = offset.clamp(0.0, max_off);
    ui.ctx().memory_mut(|m| m.data.insert_temp(id, offset));

    // BARRA emitida pelo DOM (SolidRect) — fixa na viewport (a função soma o offset).
    if scroll_y {
        layout::emit_scrollbar(&mut list, viewport_w, viewport_h, content_h, offset, sb, force);
    }
    // pinta tudo transladado por -offset (o conteúdo sobe; a barra, somando offset na
    // emissão, fica parada na tela). Recorta na área visível.
    let clip = ui.max_rect();
    let old_clip = ui.clip_rect();
    ui.set_clip_rect(clip);
    paint_list(ui, &list, -offset);
    ui.set_clip_rect(old_clip);
    // reserva a área visível (não a altura total — o scroll é nosso, não do egui).
    ui.allocate_space(egui::vec2(viewport_w, viewport_h));
}

/// Percorre a [`DisplayList`] e pinta cada item via `ui.painter()`, em coordenadas
/// absolutas (conteúdo + origem do `ui`). A ordem da lista É o z-order (o que vem
/// depois pinta por cima). Reserva o espaço da altura total para o `ui` pai.
fn paint_list(ui: &mut egui::Ui, list: &DisplayList, offset_y: f32) {
    // origem do conteúdo + a translação de scroll (offset_y negativo sobe o conteúdo).
    let origin = ui.max_rect().min + egui::vec2(0.0, offset_y);
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
            DisplayItem::Text { x, y, text, color, size, mono, bold } => {
                // bold vence (família "bold"); senão mono → Monospace; senão Proportional.
                let family = if *bold {
                    egui::FontFamily::Name("bold".into())
                } else if *mono {
                    egui::FontFamily::Monospace
                } else {
                    egui::FontFamily::Proportional
                };
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
}
