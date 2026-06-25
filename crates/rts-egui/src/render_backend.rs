//! `EguiRenderer` — implementação do trait `rts_render::Renderer` pelo egui. É o
//! que torna o egui um BACKEND PLUGÁVEL do namespace `render`: o DOM/layout (TS)
//! chama `render.rect/text/...` (no crate `rts-render`), que despacham para este
//! backend, registrado em `register_backend()` na inicialização.
//!
//! A pintura reusa a mecânica do canvas: enfileira `WidgetCmd::Draw*` no `UiCtx`
//! da janela-alvo (executados no `endFrame` via `egui::Painter`). `begin_frame`/
//! `end_frame` mapeiam para o ciclo de frame do egui. `measure_text` mede com a
//! fonte real (aprox. no PoC — ver canvas).

use rts_render::Renderer;

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

    fn end_frame(&self, target: u64) {
        crate::frame::__RTS_FN_NS_EGUI_END_FRAME(target);
    }
}

/// Registra o egui como o backend de render ativo do `rts-render`. Chamado uma vez
/// na inicialização (via o `register` do namespace egui).
pub fn register_backend() {
    rts_render::set_backend(Box::new(EguiRenderer));
}
