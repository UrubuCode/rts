//! ABI do namespace `render` — os primitivos que o DOM/layout (TS) chama. Cada fn
//! DESPACHA para o backend de render ativo (`crate::with_backend`); se nenhum
//! backend estiver registrado, é no-op (e `measureText` devolve `-1`).
//!
//! O TS fala `render.*` e nunca nomeia o backend concreto (egui). Símbolos
//! `__RTS_FN_NS_RENDER_*`. Coords/tamanhos `number` (pontos); cores `0xRRGGBBAA`.

use rts_engine::abi::str_abi;
use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

use AbiType::{F64, I64, StrPtr, U64 as Handle};

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_RENDER_BEGIN_FRAME(target: u64) {
    crate::with_backend(|r| r.begin_frame(target));
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn __RTS_FN_NS_RENDER_RECT(
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

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_RENDER_TEXT(
    target: u64,
    x: f64,
    y: f64,
    text_ptr: *const u8,
    text_len: i64,
    color: i64,
    size: f64,
    flags: i64,
) {
    let text = unsafe { str_abi::from_abi(text_ptr, text_len) }.unwrap_or("");
    crate::with_backend(|r| {
        r.text(target, x as f32, y as f32, text, color as u32, size as f32, flags)
    });
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn __RTS_FN_NS_RENDER_LINE(
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

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_RENDER_MEASURE_TEXT(
    target: u64,
    text_ptr: *const u8,
    text_len: i64,
    size: f64,
    bold: i64,
) -> f64 {
    let text = unsafe { str_abi::from_abi(text_ptr, text_len) }.unwrap_or("");
    crate::with_backend(|r| r.measure_text(target, text, size as f32, bold != 0) as f64)
        .unwrap_or(-1.0)
}

/// `render.image(target, x, y, w, h, pixelsPtr, imgW, imgH)` — desenha um bitmap
/// RGBA8 (do ponteiro) escalado no retângulo. O ptr vem como `u64` (o TS o obtém
/// via `buffer.ptr(handle)`). Base de vídeo/imagem/viewport.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn __RTS_FN_NS_RENDER_IMAGE(
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

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_RENDER_END_FRAME(target: u64) {
    crate::with_backend(|r| r.end_frame(target));
}

// ── INPUT (entrada) — o backend reporta o cru; o DOM/layout interpreta ──────────

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_INPUT_MOUSE_X(target: u64) -> f64 {
    crate::with_input(|i| i.mouse_pos(target).0 as f64).unwrap_or(-1.0)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_INPUT_MOUSE_Y(target: u64) -> f64 {
    crate::with_input(|i| i.mouse_pos(target).1 as f64).unwrap_or(-1.0)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_INPUT_MOUSE_DOWN(target: u64, button: i64) -> i64 {
    crate::with_input(|i| i.mouse_down(target, button) as i64).unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_INPUT_MOUSE_CLICKED(target: u64, button: i64) -> i64 {
    crate::with_input(|i| i.mouse_clicked(target, button) as i64).unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_INPUT_MOUSE_PRESSED(target: u64, button: i64) -> i64 {
    crate::with_input(|i| i.mouse_pressed(target, button) as i64).unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_INPUT_MOUSE_RELEASED(target: u64, button: i64) -> i64 {
    crate::with_input(|i| i.mouse_released(target, button) as i64).unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_INPUT_MOUSE_DOUBLE_CLICKED(target: u64, button: i64) -> i64 {
    crate::with_input(|i| i.mouse_double_clicked(target, button) as i64).unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_INPUT_MOUSE_DELTA_X(target: u64) -> f64 {
    crate::with_input(|i| i.mouse_delta(target).0 as f64).unwrap_or(0.0)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_INPUT_MOUSE_DELTA_Y(target: u64) -> f64 {
    crate::with_input(|i| i.mouse_delta(target).1 as f64).unwrap_or(0.0)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_INPUT_DRAGGING(target: u64) -> i64 {
    crate::with_input(|i| i.dragging(target) as i64).unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_INPUT_WHEEL(target: u64) -> f64 {
    crate::with_input(|i| i.wheel(target) as f64).unwrap_or(0.0)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_INPUT_WHEEL_X(target: u64) -> f64 {
    crate::with_input(|i| i.wheel_x(target) as f64).unwrap_or(0.0)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_INPUT_SET_CURSOR(target: u64, kind: i64) {
    crate::with_input(|i| i.set_cursor(target, kind));
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_INPUT_KEY_PRESSED(target: u64, key: i64) -> i64 {
    crate::with_input(|i| i.key_pressed(target, key) as i64).unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_INPUT_KEY_DOWN(target: u64, key: i64) -> i64 {
    crate::with_input(|i| i.key_down(target, key) as i64).unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_INPUT_KEY_RELEASED(target: u64, key: i64) -> i64 {
    crate::with_input(|i| i.key_released(target, key) as i64).unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_INPUT_MOD_CTRL(target: u64) -> i64 {
    crate::with_input(|i| i.modifiers(target).ctrl as i64).unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_INPUT_MOD_SHIFT(target: u64) -> i64 {
    crate::with_input(|i| i.modifiers(target).shift as i64).unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_INPUT_MOD_ALT(target: u64) -> i64 {
    crate::with_input(|i| i.modifiers(target).alt as i64).unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_INPUT_MOD_CMD(target: u64) -> i64 {
    crate::with_input(|i| i.modifiers(target).cmd as i64).unwrap_or(0)
}

/// `input.textInput(target)` → texto digitado neste frame, como handle de string
/// GC (o que o TS recebe como `string`). String vazia se nada.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_INPUT_TEXT(target: u64) -> u64 {
    let s = crate::with_input(|i| i.text_input(target)).unwrap_or_default();
    unsafe { __RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64) }
}

unsafe extern "C" {
    fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64;
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
        pure: false,
        intrinsic: None,
    }
}

/// Monta o namespace `render` no Engine. As fns despacham para o backend ativo —
/// o engine não nomeia o backend concreto.
pub fn register(e: &mut Engine) {
    e.ns("render")
        .doc("Abstract render backend (the active Renderer paints). DOM/layout calls render.*, never a concrete backend.")
        .member(func(
            "beginFrame",
            "__RTS_FN_NS_RENDER_BEGIN_FRAME",
            Sig::new(vec![Handle], AbiType::Void),
            "beginFrame(target: number): void",
            "Open a paint frame on the target (window).",
            __RTS_FN_NS_RENDER_BEGIN_FRAME as *const u8,
        ))
        .member(func(
            "rect",
            "__RTS_FN_NS_RENDER_RECT",
            Sig::new(vec![Handle, F64, F64, F64, F64, I64, F64, I64, F64], AbiType::Void),
            "rect(target: number, x: number, y: number, w: number, h: number, fill: number, strokeW: number, stroke: number, radius: number): void",
            "Filled rect + optional border at absolute coords. Colors 0xRRGGBBAA.",
            __RTS_FN_NS_RENDER_RECT as *const u8,
        ))
        .member(func(
            "text",
            "__RTS_FN_NS_RENDER_TEXT",
            Sig::new(vec![Handle, F64, F64, StrPtr, I64, F64, I64], AbiType::Void),
            "text(target: number, x: number, y: number, text: string, color: number, size: number, flags: number): void",
            "Text at absolute (x,y) top-left. flags 1=bold 2=italic 4=mono.",
            __RTS_FN_NS_RENDER_TEXT as *const u8,
        ))
        .member(func(
            "line",
            "__RTS_FN_NS_RENDER_LINE",
            Sig::new(vec![Handle, F64, F64, F64, F64, F64, I64], AbiType::Void),
            "line(target: number, x1: number, y1: number, x2: number, y2: number, w: number, color: number): void",
            "Line at absolute coords. Color 0xRRGGBBAA.",
            __RTS_FN_NS_RENDER_LINE as *const u8,
        ))
        .member(func(
            "measureText",
            "__RTS_FN_NS_RENDER_MEASURE_TEXT",
            Sig::new(vec![Handle, StrPtr, F64, I64], F64),
            "measureText(target: number, text: string, size: number, bold: number): number",
            "Width (points) of text in the real font — the only layout-aware op (TS can't measure without the font).",
            __RTS_FN_NS_RENDER_MEASURE_TEXT as *const u8,
        ))
        .member(func(
            "image",
            "__RTS_FN_NS_RENDER_IMAGE",
            Sig::new(vec![Handle, F64, F64, F64, F64, Handle, I64, I64], AbiType::Void),
            "image(target: number, x: number, y: number, w: number, h: number, pixelsPtr: number, imgW: number, imgH: number): void",
            "Draws an RGBA8 bitmap (from pixelsPtr, imgW*imgH*4 bytes) scaled into the rect. Base for video/image/viewport. Get pixelsPtr via buffer.ptr(handle).",
            __RTS_FN_NS_RENDER_IMAGE as *const u8,
        ))
        .member(func(
            "endFrame",
            "__RTS_FN_NS_RENDER_END_FRAME",
            Sig::new(vec![Handle], AbiType::Void),
            "endFrame(target: number): void",
            "Close + present the frame.",
            __RTS_FN_NS_RENDER_END_FRAME as *const u8,
        ))
        .done();
}

/// Monta o namespace `input` no Engine. As fns reportam o estado de input do
/// backend ativo (polling). O DOM/layout consome p/ hit-test + eventos.
pub fn register_input(e: &mut Engine) {
    e.ns("input")
        .doc("Raw input from the active backend (polling). The DOM/layout hit-tests + dispatches events; the backend doesn't know DOM nodes.")
        .member(func(
            "mouseX",
            "__RTS_FN_NS_INPUT_MOUSE_X",
            Sig::new(vec![Handle], F64),
            "mouseX(target: number): number",
            "Cursor X in points, or -1 if outside the window.",
            __RTS_FN_NS_INPUT_MOUSE_X as *const u8,
        ))
        .member(func(
            "mouseY",
            "__RTS_FN_NS_INPUT_MOUSE_Y",
            Sig::new(vec![Handle], F64),
            "mouseY(target: number): number",
            "Cursor Y in points, or -1 if outside the window.",
            __RTS_FN_NS_INPUT_MOUSE_Y as *const u8,
        ))
        .member(func(
            "mouseDown",
            "__RTS_FN_NS_INPUT_MOUSE_DOWN",
            Sig::new(vec![Handle, I64], I64),
            "mouseDown(target: number, button: number): number",
            "1 if button (0=left 1=right 2=middle) is held now, else 0.",
            __RTS_FN_NS_INPUT_MOUSE_DOWN as *const u8,
        ))
        .member(func(
            "mouseClicked",
            "__RTS_FN_NS_INPUT_MOUSE_CLICKED",
            Sig::new(vec![Handle, I64], I64),
            "mouseClicked(target: number, button: number): number",
            "1 if a full click of button happened this frame, else 0.",
            __RTS_FN_NS_INPUT_MOUSE_CLICKED as *const u8,
        ))
        .member(func(
            "mousePressed",
            "__RTS_FN_NS_INPUT_MOUSE_PRESSED",
            Sig::new(vec![Handle, I64], I64),
            "mousePressed(target: number, button: number): number",
            "1 if button was pressed this frame (up->down transition).",
            __RTS_FN_NS_INPUT_MOUSE_PRESSED as *const u8,
        ))
        .member(func(
            "mouseReleased",
            "__RTS_FN_NS_INPUT_MOUSE_RELEASED",
            Sig::new(vec![Handle, I64], I64),
            "mouseReleased(target: number, button: number): number",
            "1 if button was released this frame (down->up transition).",
            __RTS_FN_NS_INPUT_MOUSE_RELEASED as *const u8,
        ))
        .member(func(
            "mouseDoubleClicked",
            "__RTS_FN_NS_INPUT_MOUSE_DOUBLE_CLICKED",
            Sig::new(vec![Handle, I64], I64),
            "mouseDoubleClicked(target: number, button: number): number",
            "1 if button was double-clicked this frame.",
            __RTS_FN_NS_INPUT_MOUSE_DOUBLE_CLICKED as *const u8,
        ))
        .member(func(
            "mouseDeltaX",
            "__RTS_FN_NS_INPUT_MOUSE_DELTA_X",
            Sig::new(vec![Handle], F64),
            "mouseDeltaX(target: number): number",
            "Relative cursor movement X this frame (points).",
            __RTS_FN_NS_INPUT_MOUSE_DELTA_X as *const u8,
        ))
        .member(func(
            "mouseDeltaY",
            "__RTS_FN_NS_INPUT_MOUSE_DELTA_Y",
            Sig::new(vec![Handle], F64),
            "mouseDeltaY(target: number): number",
            "Relative cursor movement Y this frame (points).",
            __RTS_FN_NS_INPUT_MOUSE_DELTA_Y as *const u8,
        ))
        .member(func(
            "dragging",
            "__RTS_FN_NS_INPUT_DRAGGING",
            Sig::new(vec![Handle], I64),
            "dragging(target: number): number",
            "1 while dragging (pressed + moving enough) — native drag.",
            __RTS_FN_NS_INPUT_DRAGGING as *const u8,
        ))
        .member(func(
            "wheel",
            "__RTS_FN_NS_INPUT_WHEEL",
            Sig::new(vec![Handle], F64),
            "wheel(target: number): number",
            "Vertical scroll delta this frame.",
            __RTS_FN_NS_INPUT_WHEEL as *const u8,
        ))
        .member(func(
            "wheelX",
            "__RTS_FN_NS_INPUT_WHEEL_X",
            Sig::new(vec![Handle], F64),
            "wheelX(target: number): number",
            "Horizontal scroll delta this frame.",
            __RTS_FN_NS_INPUT_WHEEL_X as *const u8,
        ))
        .member(func(
            "setCursor",
            "__RTS_FN_NS_INPUT_SET_CURSOR",
            Sig::new(vec![Handle, I64], AbiType::Void),
            "setCursor(target: number, kind: number): void",
            "Sets cursor icon: 0=default 1=pointer 2=text 3=grab 4=grabbing 5=resize-h 6=resize-v 7=crosshair 8=not-allowed.",
            __RTS_FN_NS_INPUT_SET_CURSOR as *const u8,
        ))
        .member(func(
            "keyPressed",
            "__RTS_FN_NS_INPUT_KEY_PRESSED",
            Sig::new(vec![Handle, I64], I64),
            "keyPressed(target: number, key: number): number",
            "1 if key fired this frame (with auto-repeat). Neutral codes: 1-15 edit/nav, 100-125 A-Z, 130-139 0-9, 140-151 F1-F12. See input-system-design.md.",
            __RTS_FN_NS_INPUT_KEY_PRESSED as *const u8,
        ))
        .member(func(
            "keyDown",
            "__RTS_FN_NS_INPUT_KEY_DOWN",
            Sig::new(vec![Handle, I64], I64),
            "keyDown(target: number, key: number): number",
            "1 if key is held down now (continuous, no repeat).",
            __RTS_FN_NS_INPUT_KEY_DOWN as *const u8,
        ))
        .member(func(
            "keyReleased",
            "__RTS_FN_NS_INPUT_KEY_RELEASED",
            Sig::new(vec![Handle, I64], I64),
            "keyReleased(target: number, key: number): number",
            "1 if key was released this frame.",
            __RTS_FN_NS_INPUT_KEY_RELEASED as *const u8,
        ))
        .member(func(
            "modCtrl",
            "__RTS_FN_NS_INPUT_MOD_CTRL",
            Sig::new(vec![Handle], I64),
            "modCtrl(target: number): number",
            "1 if Ctrl is held now.",
            __RTS_FN_NS_INPUT_MOD_CTRL as *const u8,
        ))
        .member(func(
            "modShift",
            "__RTS_FN_NS_INPUT_MOD_SHIFT",
            Sig::new(vec![Handle], I64),
            "modShift(target: number): number",
            "1 if Shift is held now.",
            __RTS_FN_NS_INPUT_MOD_SHIFT as *const u8,
        ))
        .member(func(
            "modAlt",
            "__RTS_FN_NS_INPUT_MOD_ALT",
            Sig::new(vec![Handle], I64),
            "modAlt(target: number): number",
            "1 if Alt is held now.",
            __RTS_FN_NS_INPUT_MOD_ALT as *const u8,
        ))
        .member(func(
            "modCmd",
            "__RTS_FN_NS_INPUT_MOD_CMD",
            Sig::new(vec![Handle], I64),
            "modCmd(target: number): number",
            "1 if Cmd/Super (Win/Cmd key) is held now (egui 'command', cross-platform).",
            __RTS_FN_NS_INPUT_MOD_CMD as *const u8,
        ))
        .member(func(
            "textInput",
            "__RTS_FN_NS_INPUT_TEXT",
            Sig::new(vec![Handle], Handle),
            "textInput(target: number): string",
            "Text typed this frame (UTF-8), empty if none.",
            __RTS_FN_NS_INPUT_TEXT as *const u8,
        ))
        .done();
}
