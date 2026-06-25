//! `EguiRenderer` — implementação do trait `rts_render::Renderer` pelo egui. É o
//! que torna o egui um BACKEND PLUGÁVEL do namespace `render`: o DOM/layout (TS)
//! chama `render.rect/text/...` (no crate `rts-render`), que despacham para este
//! backend, registrado em `register_backend()` na inicialização.
//!
//! A pintura reusa a mecânica do canvas: enfileira `WidgetCmd::Draw*` no `UiCtx`
//! da janela-alvo (executados no `endFrame` via `egui::Painter`). `begin_frame`/
//! `end_frame` mapeiam para o ciclo de frame do egui. `measure_text` mede com a
//! fonte real (aprox. no PoC — ver canvas).

use rts_render::{InputSource, Renderer};

use crate::ctx::{self, WidgetCmd};

/// O backend egui (zero-sized — todo o estado vive no `UiCtx` por handle).
pub struct EguiRenderer;

impl Renderer for EguiRenderer {
    fn begin_frame(&self, target: u64) {
        crate::frame::__RTS_FN_NS_EGUI_BEGIN_FRAME(target);
    }

    fn rect(
        &self,
        target: u64,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        fill: u32,
        stroke_w: f32,
        stroke: u32,
        radius: f32,
    ) {
        ctx::with_ctx(target, |c| {
            if c.frame_active {
                c.cmds.push(WidgetCmd::DrawRect { x, y, w, h, fill, stroke_w, stroke, radius });
            }
        });
    }

    fn text(&self, target: u64, x: f32, y: f32, text: &str, color: u32, size: f32, flags: i64) {
        let text = text.to_string();
        ctx::with_ctx(target, |c| {
            if c.frame_active {
                c.cmds.push(WidgetCmd::DrawText { x, y, text, color, size, flags });
            }
        });
    }

    fn line(&self, target: u64, x1: f32, y1: f32, x2: f32, y2: f32, w: f32, color: u32) {
        ctx::with_ctx(target, |c| {
            if c.frame_active {
                c.cmds.push(WidgetCmd::DrawLine { x1, y1, x2, y2, w, color });
            }
        });
    }

    fn measure_text(&self, target: u64, text: &str, size: f32, _bold: bool) -> f32 {
        // PoC: medição aproximada (mesma do canvas; a exata via atlas de fontes é
        // TODO isolado). 0.52·size por caractere (proporcional).
        let _ = target;
        text.chars().count() as f32 * size * 0.52
    }

    fn image(
        &self,
        target: u64,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        pixels: *const u8,
        img_w: u32,
        img_h: u32,
    ) {
        // Copia os bytes RGBA do ponteiro para um ColorImage, sobe como textura
        // EFÊMERA (carregada por frame; egui descarta no fim) e pinta no retângulo.
        let n = (img_w as usize) * (img_h as usize) * 4;
        let bytes = unsafe { std::slice::from_raw_parts(pixels, n) };
        let color_image =
            egui::ColorImage::from_rgba_unmultiplied([img_w as usize, img_h as usize], bytes);
        ctx::with_ctx(target, |c| {
            if !c.frame_active {
                return;
            }
            // textura efêmera (nome único por frame evita reuso de cache stale).
            let tex = c.egui_ctx.load_texture(
                format!("__rts_img_{}", c.cmds.len()),
                color_image,
                egui::TextureOptions::LINEAR,
            );
            c.cmds.push(WidgetCmd::DrawImage { x, y, w, h, tex });
        });
    }

    fn end_frame(&self, target: u64) {
        crate::frame::__RTS_FN_NS_EGUI_END_FRAME(target);
    }
}

/// Mapeia o código de tecla NEUTRO do `rts-render` para a `egui::Key`.
fn neutral_to_egui_key(key: i64) -> Option<egui::Key> {
    use rts_render::*;
    Some(match key {
        KEY_ENTER => egui::Key::Enter,
        KEY_ESCAPE => egui::Key::Escape,
        KEY_SPACE => egui::Key::Space,
        KEY_BACKSPACE => egui::Key::Backspace,
        KEY_ARROW_UP => egui::Key::ArrowUp,
        KEY_ARROW_DOWN => egui::Key::ArrowDown,
        KEY_ARROW_LEFT => egui::Key::ArrowLeft,
        KEY_ARROW_RIGHT => egui::Key::ArrowRight,
        _ => return None,
    })
}

impl InputSource for EguiRenderer {
    fn mouse_pos(&self, target: u64) -> (f32, f32) {
        ctx::with_ctx(target, |c| {
            c.egui_ctx
                .input(|i| i.pointer.hover_pos().map(|p| (p.x, p.y)).unwrap_or((-1.0, -1.0)))
        })
        .unwrap_or((-1.0, -1.0))
    }

    fn mouse_down(&self, target: u64, button: i64) -> bool {
        let btn = egui_button(button);
        ctx::with_ctx(target, |c| c.egui_ctx.input(|i| i.pointer.button_down(btn))).unwrap_or(false)
    }

    fn mouse_clicked(&self, target: u64, button: i64) -> bool {
        let btn = egui_button(button);
        ctx::with_ctx(target, |c| c.egui_ctx.input(|i| i.pointer.button_clicked(btn)))
            .unwrap_or(false)
    }

    fn wheel(&self, target: u64) -> f32 {
        ctx::with_ctx(target, |c| c.egui_ctx.input(|i| i.smooth_scroll_delta.y)).unwrap_or(0.0)
    }

    fn key_pressed(&self, target: u64, key: i64) -> bool {
        let Some(k) = neutral_to_egui_key(key) else { return false };
        ctx::with_ctx(target, |c| c.egui_ctx.input(|i| i.key_pressed(k))).unwrap_or(false)
    }

    fn text_input(&self, target: u64) -> String {
        ctx::with_ctx(target, |c| {
            c.egui_ctx.input(|i| {
                let mut s = String::new();
                for ev in &i.events {
                    if let egui::Event::Text(t) = ev {
                        s.push_str(t);
                    }
                }
                s
            })
        })
        .unwrap_or_default()
    }
}

/// Mapeia 0/1/2 → `egui::PointerButton`.
fn egui_button(button: i64) -> egui::PointerButton {
    match button {
        1 => egui::PointerButton::Secondary,
        2 => egui::PointerButton::Middle,
        _ => egui::PointerButton::Primary,
    }
}

/// Registra o egui como o backend de render E input ativo do `rts-render`. Chamado
/// uma vez na inicialização (via o `register` do namespace egui).
pub fn register_backend() {
    rts_render::set_backend(Box::new(EguiRenderer));
    rts_render::set_input(Box::new(EguiRenderer));
}
