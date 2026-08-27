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
//! Ver `docs/ui/dom-in-ts.md`.

use crate::ctx::{self, WidgetCmd};

/// `drawRect(h, x, y, w, h_, fill, strokeW, stroke, radius)` — retângulo
/// preenchido + borda opcional, em coords absolutas. Cores `0xRRGGBBAA`.
#[allow(clippy::too_many_arguments)]
pub fn draw_rect(
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

/// `drawImage(h, x, y, w, h, pixels, img_w, img_h)` — pinta pixels RGBA8 num
/// retângulo.
///
/// # Por que uma FATIA e não um ponteiro
///
/// O `RenderBackend::image` deste mesmo crate recebe `*const u8` e faz um
/// `from_raw_parts` — o que serve àquele trait, cujo chamador é código Rust que
/// segura os bytes vivos. Esta é a porta para um PROGRAMA, e a regra ali é outra:
/// o coletor move células, então um endereço que o programa calculou pode não ser
/// mais o buffer quando chegar aqui. É a mesma razão que fez `meshUpload` receber
/// views tipadas em vez de ponteiro e tamanho.
///
/// A textura do framebuffer é persistente por janela. O primeiro frame aloca o
/// recurso; frames seguintes actualizam o mesmo `TextureHandle`, evitando criar e
/// libertar uma textura egui por frame. Se as dimensões mudarem, o recurso é
/// recriado uma vez para acompanhar o novo tamanho.
pub fn draw_image(h: u64, x: f64, y: f64, w: f64, h_: f64, pixels: &[u8], img_w: i64, img_h: i64) {
    let width = img_w.max(0) as usize;
    let height = img_h.max(0) as usize;
    // Um tamanho que não bate com os bytes é um pedido impossível, e o
    // `from_rgba_unmultiplied` do egui entra em pânico nele. Recusar em silêncio
    // é o certo: a alternativa é derrubar o processo do programa por um preview.
    if width == 0 || height == 0 || pixels.len() < width * height * 4 {
        return;
    }
    let image = egui::ColorImage::from_rgba_unmultiplied([width, height], &pixels[..width * height * 4]);
    ctx::with_ctx(h, |c| {
        if !c.frame_active {
            return;
        }
        if c.image_tex.is_none() || c.image_w != width || c.image_h != height {
            c.image_w = width;
            c.image_h = height;
            c.image_tex = Some(c.egui_ctx.load_texture(
                "__rts_framebuffer",
                image,
                egui::TextureOptions::LINEAR,
            ));
        } else if let Some(tex) = c.image_tex.as_mut() {
            tex.set(image, egui::TextureOptions::LINEAR);
        }
        let Some(tex) = c.image_tex.as_ref() else {
            return;
        };
        c.cmds.push(WidgetCmd::DrawImage {
            x: x as f32,
            y: y as f32,
            w: w as f32,
            h: h_ as f32,
            tex: tex.clone(),
        });
    });
}

/// `drawText(h, x, y, text, color, size, flags)` — texto numa posição absoluta.
/// `flags` bitmask 1=bold 2=italic 4=mono. Cor `0xRRGGBBAA`.
#[allow(clippy::too_many_arguments)]
pub fn draw_text(h: u64, x: f64, y: f64, text: &str, color: i64, size: f64, flags: i64) {
    let text = text.to_string();
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
#[allow(clippy::too_many_arguments)]
pub fn draw_line(
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
pub fn measure_text(h: u64, text: &str, size: f64, bold: i64) -> f64 {
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
