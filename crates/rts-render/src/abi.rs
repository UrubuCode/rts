//! ABI do namespace `render` — os primitivos que o DOM/layout (TS) chama. Cada fn
//! DESPACHA para o backend de render ativo (`crate::with_backend`); se nenhum
//! backend estiver registrado, é no-op (e `measureText` devolve `-1`).
//!
//! O TS fala `render.*` e nunca nomeia o backend concreto (egui). Símbolos
//! `__rtsm_render_*`. Coords/tamanhos `number` (pontos); cores `0xRRGGBBAA`.

use rts_engine::abi::str_abi;
use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

use AbiType::{F64, I64, StrPtr, U64 as Handle};

#[rtse::abi(module = "render", value = "beginFrame")]
pub fn __rtsm_render_beginFrame(target: u64) {
    crate::with_backend(|r| r.begin_frame(target));
}

#[rtse::abi(module = "render", value = "rect")]
#[allow(clippy::too_many_arguments)]
pub fn __rtsm_render_rect(
    target: u64,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    fill: i64,
    stroke_w: f64,
    stroke: i64,
    radius: f64,
) {
    crate::with_backend(|r| {
        r.rect(
            target,
            x as f32,
            y as f32,
            w as f32,
            h as f32,
            fill as u32,
            stroke_w as f32,
            stroke as u32,
            radius as f32,
        )
    });
}

#[rtse::abi(module = "render", value = "text")]
pub fn __rtsm_render_text(
    target: u64,
    x: f64,
    y: f64,
    text_ptr: u64,
    text_len: i64,
    color: i64,
    size: f64,
    flags: i64,
) {
    let text_ptr = text_ptr as *const u8;

    let text = unsafe { str_abi::from_abi(text_ptr, text_len) }.unwrap_or("");
    crate::with_backend(|r| {
        r.text(target, x as f32, y as f32, text, color as u32, size as f32, flags)
    });
}

#[rtse::abi(module = "render", value = "line")]
#[allow(clippy::too_many_arguments)]
pub fn __rtsm_render_line(
    target: u64,
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    w: f64,
    color: i64,
) {
    crate::with_backend(|r| {
        r.line(target, x1 as f32, y1 as f32, x2 as f32, y2 as f32, w as f32, color as u32)
    });
}

#[rtse::abi(module = "render", value = "measureText")]
pub fn __rtsm_render_measureText(
    target: u64,
    text_ptr: u64,
    text_len: i64,
    size: f64,
    bold: i64,
) -> f64 {
    let text_ptr = text_ptr as *const u8;

    let text = unsafe { str_abi::from_abi(text_ptr, text_len) }.unwrap_or("");
    crate::with_backend(|r| r.measure_text(target, text, size as f32, bold != 0) as f64)
        .unwrap_or(-1.0)
}

/// `render.image(target, x, y, w, h, pixelsPtr, imgW, imgH)` — desenha um bitmap
/// RGBA8 (do ponteiro) escalado no retângulo. O ptr vem como `u64` (o TS o obtém
/// via `buffer.ptr(handle)`). Base de vídeo/imagem/viewport.
#[rtse::abi(module = "render", value = "image")]
#[allow(clippy::too_many_arguments)]
pub fn __rtsm_render_image(
    target: u64,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    pixels_ptr: u64,
    img_w: i64,
    img_h: i64,
) {
    if pixels_ptr == 0 || img_w <= 0 || img_h <= 0 {
        return;
    }
    let pixels = pixels_ptr as *const u8;
    crate::with_backend(|r| {
        r.image(target, x as f32, y as f32, w as f32, h as f32, pixels, img_w as u32, img_h as u32)
    });
}

#[rtse::abi(module = "render", value = "endFrame")]
pub fn __rtsm_render_endFrame(target: u64) {
    crate::with_backend(|r| r.end_frame(target));
}

fn func(name: &str, symbol: &str, sig: Sig, ts: &str, doc: &str, fp: *const u8) -> Member {
    Member {
        name: name.to_string(),
        kind: MemberKind::Function,
        sig,
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: doc.to_string(),
        ret_class: None,
        pure: false,
        emit: None,
    }
}

/// Monta o namespace `render` no Engine. As fns despacham para o backend ativo —
/// o engine não nomeia o backend concreto.
pub fn register(e: &mut Engine) {
    e.ns("render")
        .doc("Abstract render backend (the active Renderer paints). DOM/layout calls render.*, never a concrete backend.")
        .member(func(
            "beginFrame",
            "__rtsm_render_beginFrame",
            Sig::new(vec![Handle], AbiType::Void),
            "beginFrame(target: number): void",
            "Open a paint frame on the target (window).",
            __rtsm_render_beginFrame as *const u8,
        ))
        .member(func(
            "rect",
            "__rtsm_render_rect",
            Sig::new(vec![Handle, F64, F64, F64, F64, I64, F64, I64, F64], AbiType::Void),
            "rect(target: number, x: number, y: number, w: number, h: number, fill: number, strokeW: number, stroke: number, radius: number): void",
            "Filled rect + optional border at absolute coords. Colors 0xRRGGBBAA.",
            __rtsm_render_rect as *const u8,
        ))
        .member(func(
            "text",
            "__rtsm_render_text",
            Sig::new(vec![Handle, F64, F64, StrPtr, I64, F64, I64], AbiType::Void),
            "text(target: number, x: number, y: number, text: string, color: number, size: number, flags: number): void",
            "Text at absolute (x,y) top-left. flags 1=bold 2=italic 4=mono.",
            __rtsm_render_text as *const u8,
        ))
        .member(func(
            "line",
            "__rtsm_render_line",
            Sig::new(vec![Handle, F64, F64, F64, F64, F64, I64], AbiType::Void),
            "line(target: number, x1: number, y1: number, x2: number, y2: number, w: number, color: number): void",
            "Line at absolute coords. Color 0xRRGGBBAA.",
            __rtsm_render_line as *const u8,
        ))
        .member(func(
            "measureText",
            "__rtsm_render_measureText",
            Sig::new(vec![Handle, StrPtr, F64, I64], F64),
            "measureText(target: number, text: string, size: number, bold: number): number",
            "Width (points) of text in the real font — the only layout-aware op (TS can't measure without the font).",
            __rtsm_render_measureText as *const u8,
        ))
        .member(func(
            "image",
            "__rtsm_render_image",
            Sig::new(vec![Handle, F64, F64, F64, F64, Handle, I64, I64], AbiType::Void),
            "image(target: number, x: number, y: number, w: number, h: number, pixelsPtr: number, imgW: number, imgH: number): void",
            "Draws an RGBA8 bitmap (from pixelsPtr, imgW*imgH*4 bytes) scaled into the rect. Base for video/image/viewport. Get pixelsPtr via buffer.ptr(handle).",
            __rtsm_render_image as *const u8,
        ))
        .member(func(
            "endFrame",
            "__rtsm_render_endFrame",
            Sig::new(vec![Handle], AbiType::Void),
            "endFrame(target: number): void",
            "Close + present the frame.",
            __rtsm_render_endFrame as *const u8,
        ))
        .done();
}
