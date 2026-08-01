//! Namespace ABI `input` — as fns `extern "C"` que o TS chama (`input.mouseX`,
//! `input.key`, `input.modCtrl`, …) e que despacham para o capturador ATIVO
//! (`with_input`). O backend concreto (egui) implementa o trait `InputSource`;
//! aqui só está a casca ABI + a tabela de membros (`register_input`).

use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

use AbiType::{F64, I64, StrPtr, U64 as Handle};

#[rtse::abi(module = "input", value = "mouseX")]
pub fn __rtsm_input_mouseX(target: u64) -> f64 {
    crate::with_input(|i| i.mouse_pos(target).0 as f64).unwrap_or(-1.0)
}

#[rtse::abi(module = "input", value = "mouseY")]
pub fn __rtsm_input_mouseY(target: u64) -> f64 {
    crate::with_input(|i| i.mouse_pos(target).1 as f64).unwrap_or(-1.0)
}

#[rtse::abi(module = "input", value = "mouseDown")]
pub fn __rtsm_input_mouseDown(target: u64, button: i64) -> i64 {
    crate::with_input(|i| i.mouse_down(target, button) as i64).unwrap_or(0)
}

#[rtse::abi(module = "input", value = "mouseClicked")]
pub fn __rtsm_input_mouseClicked(target: u64, button: i64) -> i64 {
    crate::with_input(|i| i.mouse_clicked(target, button) as i64).unwrap_or(0)
}

#[rtse::abi(module = "input", value = "mousePressed")]
pub fn __rtsm_input_mousePressed(target: u64, button: i64) -> i64 {
    crate::with_input(|i| i.mouse_pressed(target, button) as i64).unwrap_or(0)
}

#[rtse::abi(module = "input", value = "mouseReleased")]
pub fn __rtsm_input_mouseReleased(target: u64, button: i64) -> i64 {
    crate::with_input(|i| i.mouse_released(target, button) as i64).unwrap_or(0)
}

#[rtse::abi(module = "input", value = "mouseDoubleClicked")]
pub fn __rtsm_input_mouseDoubleClicked(target: u64, button: i64) -> i64 {
    crate::with_input(|i| i.mouse_double_clicked(target, button) as i64).unwrap_or(0)
}

#[rtse::abi(module = "input", value = "mouseDeltaX")]
pub fn __rtsm_input_mouseDeltaX(target: u64) -> f64 {
    crate::with_input(|i| i.mouse_delta(target).0 as f64).unwrap_or(0.0)
}

#[rtse::abi(module = "input", value = "mouseDeltaY")]
pub fn __rtsm_input_mouseDeltaY(target: u64) -> f64 {
    crate::with_input(|i| i.mouse_delta(target).1 as f64).unwrap_or(0.0)
}

#[rtse::abi(module = "input", value = "dragging")]
pub fn __rtsm_input_dragging(target: u64) -> i64 {
    crate::with_input(|i| i.dragging(target) as i64).unwrap_or(0)
}

#[rtse::abi(module = "input", value = "wheel")]
pub fn __rtsm_input_wheel(target: u64) -> f64 {
    crate::with_input(|i| i.wheel(target) as f64).unwrap_or(0.0)
}

#[rtse::abi(module = "input", value = "wheelX")]
pub fn __rtsm_input_wheelX(target: u64) -> f64 {
    crate::with_input(|i| i.wheel_x(target) as f64).unwrap_or(0.0)
}

#[rtse::abi(module = "input", value = "setCursor")]
pub fn __rtsm_input_setCursor(target: u64, kind: i64) {
    crate::with_input(|i| i.set_cursor(target, kind));
}

/// Estado de uma tecla numa fase (`phase`: 0=down, 1=pressed, 2=released — ver
/// `KEY_PHASE_*`). Símbolo ÚNICO que substitui os antigos KEY_DOWN/PRESSED/
/// RELEASED; o `.ts` (canvas) expõe os atalhos `keyDown`/`keyPressed`/`keyReleased`.
#[rtse::abi(module = "input", value = "key")]
pub fn __rtsm_input_key(target: u64, key: i64, phase: i64) -> i64 {
    crate::with_input(|i| i.key_state(target, key, phase) as i64).unwrap_or(0)
}

#[rtse::abi(module = "input", value = "modCtrl")]
pub fn __rtsm_input_modCtrl(target: u64) -> i64 {
    crate::with_input(|i| i.modifiers(target).ctrl as i64).unwrap_or(0)
}

#[rtse::abi(module = "input", value = "modShift")]
pub fn __rtsm_input_modShift(target: u64) -> i64 {
    crate::with_input(|i| i.modifiers(target).shift as i64).unwrap_or(0)
}

#[rtse::abi(module = "input", value = "modAlt")]
pub fn __rtsm_input_modAlt(target: u64) -> i64 {
    crate::with_input(|i| i.modifiers(target).alt as i64).unwrap_or(0)
}

#[rtse::abi(module = "input", value = "modCmd")]
pub fn __rtsm_input_modCmd(target: u64) -> i64 {
    crate::with_input(|i| i.modifiers(target).cmd as i64).unwrap_or(0)
}

/// `input.textInput(target)` → texto digitado neste frame, como handle de string
/// GC (o que o TS recebe como `string`). String vazia se nada.
#[rtse::abi(module = "input", value = "textInput")]
pub fn __rtsm_input_textInput(target: u64) -> u64 {
    let s = crate::with_input(|i| i.text_input(target)).unwrap_or_default();
    unsafe { __RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64) }
}

/// `input.copyText(target, text)` — coloca `text` no clipboard do SO (Ctrl+C).
#[rtse::abi(module = "input", value = "copyText")]
pub fn __rtsm_input_copyText(target: u64, ptr: u64, len: i64) {
    let ptr = ptr as *const u8;

    if ptr.is_null() || len <= 0 {
        return;
    }
    let text = unsafe {
        std::str::from_utf8_unchecked(std::slice::from_raw_parts(ptr, len as usize))
    };
    crate::with_input(|i| i.copy_text(target, text));
}

use rts_engine::heap::string_pool::__RTS_FN_NS_GC_STRING_NEW;

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

/// Monta o namespace `input` no Engine. As fns reportam o estado de input do
/// backend ativo (polling). O DOM/layout/app consome p/ hit-test + eventos.
pub fn register_input(e: &mut Engine) {
    e.ns("input")
        .doc("Raw input from the active backend (polling). The DOM/layout hit-tests + dispatches events; the backend doesn't know DOM nodes.")
        .member(func(
            "mouseX",
            "__rtsm_input_mouseX",
            Sig::new(vec![Handle], F64),
            "mouseX(target: number): number",
            "Cursor X in points, or -1 if outside the window.",
            __rtsm_input_mouseX as *const u8,
        ))
        .member(func(
            "mouseY",
            "__rtsm_input_mouseY",
            Sig::new(vec![Handle], F64),
            "mouseY(target: number): number",
            "Cursor Y in points, or -1 if outside the window.",
            __rtsm_input_mouseY as *const u8,
        ))
        .member(func(
            "mouseDown",
            "__rtsm_input_mouseDown",
            Sig::new(vec![Handle, I64], I64),
            "mouseDown(target: number, button: number): number",
            "1 if button (0=left 1=right 2=middle) is held now, else 0.",
            __rtsm_input_mouseDown as *const u8,
        ))
        .member(func(
            "mouseClicked",
            "__rtsm_input_mouseClicked",
            Sig::new(vec![Handle, I64], I64),
            "mouseClicked(target: number, button: number): number",
            "1 if a full click of button happened this frame, else 0.",
            __rtsm_input_mouseClicked as *const u8,
        ))
        .member(func(
            "mousePressed",
            "__rtsm_input_mousePressed",
            Sig::new(vec![Handle, I64], I64),
            "mousePressed(target: number, button: number): number",
            "1 if button was pressed this frame (up->down transition).",
            __rtsm_input_mousePressed as *const u8,
        ))
        .member(func(
            "mouseReleased",
            "__rtsm_input_mouseReleased",
            Sig::new(vec![Handle, I64], I64),
            "mouseReleased(target: number, button: number): number",
            "1 if button was released this frame (down->up transition).",
            __rtsm_input_mouseReleased as *const u8,
        ))
        .member(func(
            "mouseDoubleClicked",
            "__rtsm_input_mouseDoubleClicked",
            Sig::new(vec![Handle, I64], I64),
            "mouseDoubleClicked(target: number, button: number): number",
            "1 if button was double-clicked this frame.",
            __rtsm_input_mouseDoubleClicked as *const u8,
        ))
        .member(func(
            "mouseDeltaX",
            "__rtsm_input_mouseDeltaX",
            Sig::new(vec![Handle], F64),
            "mouseDeltaX(target: number): number",
            "Relative cursor movement X this frame (points).",
            __rtsm_input_mouseDeltaX as *const u8,
        ))
        .member(func(
            "mouseDeltaY",
            "__rtsm_input_mouseDeltaY",
            Sig::new(vec![Handle], F64),
            "mouseDeltaY(target: number): number",
            "Relative cursor movement Y this frame (points).",
            __rtsm_input_mouseDeltaY as *const u8,
        ))
        .member(func(
            "dragging",
            "__rtsm_input_dragging",
            Sig::new(vec![Handle], I64),
            "dragging(target: number): number",
            "1 while dragging (pressed + moving enough) — native drag.",
            __rtsm_input_dragging as *const u8,
        ))
        .member(func(
            "wheel",
            "__rtsm_input_wheel",
            Sig::new(vec![Handle], F64),
            "wheel(target: number): number",
            "Vertical scroll delta this frame.",
            __rtsm_input_wheel as *const u8,
        ))
        .member(func(
            "wheelX",
            "__rtsm_input_wheelX",
            Sig::new(vec![Handle], F64),
            "wheelX(target: number): number",
            "Horizontal scroll delta this frame.",
            __rtsm_input_wheelX as *const u8,
        ))
        .member(func(
            "setCursor",
            "__rtsm_input_setCursor",
            Sig::new(vec![Handle, I64], AbiType::Void),
            "setCursor(target: number, kind: number): void",
            "Sets cursor icon: 0=default 1=pointer 2=text 3=grab 4=grabbing 5=resize-h 6=resize-v 7=crosshair 8=not-allowed.",
            __rtsm_input_setCursor as *const u8,
        ))
        .member(func(
            "key",
            "__rtsm_input_key",
            Sig::new(vec![Handle, I64, I64], I64),
            "key(target: number, key: number, phase: number): number",
            "1 if key is in the given phase: 0=down (held, continuous), 1=pressed (fired this frame, auto-repeat), 2=released (this frame). Neutral codes: 1-15 edit/nav, 100-125 A-Z, 130-139 0-9, 140-151 F1-F12. The .ts canvas wraps this as keyDown/keyPressed/keyReleased. See input-system-design.md.",
            __rtsm_input_key as *const u8,
        ))
        .member(func(
            "modCtrl",
            "__rtsm_input_modCtrl",
            Sig::new(vec![Handle], I64),
            "modCtrl(target: number): number",
            "1 if Ctrl is held now.",
            __rtsm_input_modCtrl as *const u8,
        ))
        .member(func(
            "modShift",
            "__rtsm_input_modShift",
            Sig::new(vec![Handle], I64),
            "modShift(target: number): number",
            "1 if Shift is held now.",
            __rtsm_input_modShift as *const u8,
        ))
        .member(func(
            "modAlt",
            "__rtsm_input_modAlt",
            Sig::new(vec![Handle], I64),
            "modAlt(target: number): number",
            "1 if Alt is held now.",
            __rtsm_input_modAlt as *const u8,
        ))
        .member(func(
            "modCmd",
            "__rtsm_input_modCmd",
            Sig::new(vec![Handle], I64),
            "modCmd(target: number): number",
            "1 if Cmd/Super (Win/Cmd key) is held now (egui 'command', cross-platform).",
            __rtsm_input_modCmd as *const u8,
        ))
        .member(func(
            "textInput",
            "__rtsm_input_textInput",
            // Retorno `AbiType::Handle` EXPLÍCITO (não o alias `U64 as Handle`): só
            // `Handle` literal + ts `: string` faz o motor reboxar como TAG_STR
            // (string usável no TS). Com o alias `U64` reboxava como INTEIRO CRU —
            // era o bug "dados de ponteiros no campo de texto".
            Sig::new(vec![Handle], AbiType::Handle),
            "textInput(target: number): string",
            "Text typed this frame (UTF-8), empty if none. Includes pasted text (Ctrl+V).",
            __rtsm_input_textInput as *const u8,
        ))
        .member(func(
            "copyText",
            "__rtsm_input_copyText",
            Sig::new(vec![Handle, StrPtr], AbiType::Void),
            "copyText(target: number, text: string): void",
            "Put text on the OS clipboard (Ctrl+C).",
            __rtsm_input_copyText as *const u8,
        ))
        .done();
}
