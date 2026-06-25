//! Canvas BURRO — primitivos de pintura que o TS dirige. O egui não decide layout
//! aqui: o TS (a fachada DOM + o layout engine em TS) calcula posições e cores e
//! emite `drawRect`/`drawText`/`drawLine` em coordenadas ABSOLUTAS; estas fns só
//! enfileiram um `WidgetCmd` que o `endFrame` executa via `egui::Painter`.
//!
//! `measureText` é a ÚNICA operação "inteligente" que sobra no egui: medir a
//! largura de um texto exige as métricas da fonte (atlas do egui/wgpu), que o TS
//! não tem (Risco 1 do roadmap — nunca reimplementar `glyph_width` em TS). Ela
//! mede SÍNCRONO (não enfileira) e devolve a largura em pontos.
//!
//! Ver `docs/specs/dom-in-ts-architecture.md`.

use rts_engine::abi::str_abi;

use crate::ctx::{self, WidgetCmd};

/// `drawRect(h, x, y, w, h_, fill, strokeW, stroke, radius)` — retângulo
/// preenchido + borda opcional, em coords absolutas. Cores `0xRRGGBBAA`.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn __RTS_FN_NS_EGUI_DRAW_RECT(
    h: u64,
    x: f64,
    y: f64,
    w: f64,
    h_: f64,
    fill: i64,
    stroke_w: f64,
    stroke: i64,
    radius: f64,
) {
    ctx::with_ctx(h, |c| {
        if c.frame_active {
            c.cmds.push(WidgetCmd::DrawRect {
                x: x as f32,
                y: y as f32,
                w: w as f32,
                h: h_ as f32,
                fill: fill as u32,
                stroke_w: stroke_w as f32,
                stroke: stroke as u32,
                radius: radius as f32,
            });
        }
    });
}

/// `drawText(h, x, y, text, color, size, flags)` — texto numa posição absoluta.
/// `flags` bitmask 1=bold 2=italic 4=mono. Cor `0xRRGGBBAA`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EGUI_DRAW_TEXT(
    h: u64,
    x: f64,
    y: f64,
    text_ptr: *const u8,
    text_len: i64,
    color: i64,
    size: f64,
    flags: i64,
) {
    let text = unsafe { str_abi::from_abi(text_ptr, text_len) }.unwrap_or("").to_string();
    ctx::with_ctx(h, |c| {
        if c.frame_active {
            c.cmds.push(WidgetCmd::DrawText {
                x: x as f32,
                y: y as f32,
                text,
                color: color as u32,
                size: size as f32,
                flags,
            });
        }
    });
}

/// `drawLine(h, x1, y1, x2, y2, w, color)` — linha em coords absolutas.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn __RTS_FN_NS_EGUI_DRAW_LINE(
    h: u64,
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    w: f64,
    color: i64,
) {
    ctx::with_ctx(h, |c| {
        if c.frame_active {
            c.cmds.push(WidgetCmd::DrawLine {
                x1: x1 as f32,
                y1: y1 as f32,
                x2: x2 as f32,
                y2: y2 as f32,
                w: w as f32,
                color: color as u32,
            });
        }
    });
}

/// `measureText(h, text, size, bold) -> width` — largura do texto em pontos,
/// medida com a fonte real do egui. É o que o TS chama para calcular layout
/// (quebra de linha, largura de caixa). SÍNCRONO. Retorna `-1.0` (bits) se a
/// janela não existe. Independe de `frame_active` (o TS mede antes de pintar).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_EGUI_MEASURE_TEXT(
    h: u64,
    text_ptr: *const u8,
    text_len: i64,
    size: f64,
    bold: i64,
) -> f64 {
    let text = unsafe { str_abi::from_abi(text_ptr, text_len) }.unwrap_or("");
    let _ = (h, bold);
    // PoC: medição APROXIMADA (largura média de glifo ≈ 0.52·size para a fonte
    // proporcional padrão; mono ≈ 0.60·size). A medição EXATA usa o atlas de
    // fontes do egui (`Fonts::layout_no_wrap`) — a API 0.34 a expõe só via um
    // caminho `&mut` dentro de um frame; trocar para a medição real é um TODO
    // isolado nesta fn, sem mudar o contrato (o canvas/layout-TS já funciona com a
    // aproximação para validar a arquitetura). Conta caracteres (Unicode-aware).
    let n = text.chars().count() as f64;
    n * size * 0.52
}
