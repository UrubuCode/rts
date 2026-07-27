//! Namespace ABI `input` — as fns que o TS chama (`input.mouseX`, `input.key`,
//! `input.modCtrl`, …) e que despacham para o capturador ATIVO (`with_input`).
//! O backend concreto (egui) implementa o trait `InputSource`; aqui só está a
//! casca ABI + o registro (`register_input`).
//!
//! Cada membro é declarado UMA vez, com `#[rtse::function]`: o símbolo do
//! linker, a assinatura ABI, a `ts_signature` e o fn-ptr saem DERIVADOS da
//! própria fn Rust, então não há tabela paralela para dessincronizar (F7 de
//! `docs/specs/rts-macro-single-source.md`).

use rts_engine::abi::ty::{Handle, F64, I64, U64};
use rts_engine::Engine;

/// Cursor X in points, or -1 if outside the window.
#[rtse::function(module = "input", value = "mouseX")]
pub fn mouse_x(target: U64) -> F64 {
    crate::with_input(|i| i.mouse_pos(target).0 as f64).unwrap_or(-1.0)
}

/// Cursor Y in points, or -1 if outside the window.
#[rtse::function(module = "input", value = "mouseY")]
pub fn mouse_y(target: U64) -> F64 {
    crate::with_input(|i| i.mouse_pos(target).1 as f64).unwrap_or(-1.0)
}

/// 1 if button (0=left 1=right 2=middle) is held now, else 0.
#[rtse::function(module = "input", value = "mouseDown")]
pub fn mouse_down(target: U64, button: I64) -> I64 {
    crate::with_input(|i| i.mouse_down(target, button) as i64).unwrap_or(0)
}

/// 1 if a full click of button happened this frame, else 0.
#[rtse::function(module = "input", value = "mouseClicked")]
pub fn mouse_clicked(target: U64, button: I64) -> I64 {
    crate::with_input(|i| i.mouse_clicked(target, button) as i64).unwrap_or(0)
}

/// 1 if button was pressed this frame (up->down transition).
#[rtse::function(module = "input", value = "mousePressed")]
pub fn mouse_pressed(target: U64, button: I64) -> I64 {
    crate::with_input(|i| i.mouse_pressed(target, button) as i64).unwrap_or(0)
}

/// 1 if button was released this frame (down->up transition).
#[rtse::function(module = "input", value = "mouseReleased")]
pub fn mouse_released(target: U64, button: I64) -> I64 {
    crate::with_input(|i| i.mouse_released(target, button) as i64).unwrap_or(0)
}

/// 1 if button was double-clicked this frame.
#[rtse::function(module = "input", value = "mouseDoubleClicked")]
pub fn mouse_double_clicked(target: U64, button: I64) -> I64 {
    crate::with_input(|i| i.mouse_double_clicked(target, button) as i64).unwrap_or(0)
}

/// Relative cursor movement X this frame (points).
#[rtse::function(module = "input", value = "mouseDeltaX")]
pub fn mouse_delta_x(target: U64) -> F64 {
    crate::with_input(|i| i.mouse_delta(target).0 as f64).unwrap_or(0.0)
}

/// Relative cursor movement Y this frame (points).
#[rtse::function(module = "input", value = "mouseDeltaY")]
pub fn mouse_delta_y(target: U64) -> F64 {
    crate::with_input(|i| i.mouse_delta(target).1 as f64).unwrap_or(0.0)
}

/// 1 while dragging (pressed + moving enough) — native drag.
#[rtse::function(module = "input", value = "dragging")]
pub fn dragging(target: U64) -> I64 {
    crate::with_input(|i| i.dragging(target) as i64).unwrap_or(0)
}

/// Vertical scroll delta this frame.
#[rtse::function(module = "input", value = "wheel")]
pub fn wheel(target: U64) -> F64 {
    crate::with_input(|i| i.wheel(target) as f64).unwrap_or(0.0)
}

/// Horizontal scroll delta this frame.
#[rtse::function(module = "input", value = "wheelX")]
pub fn wheel_x(target: U64) -> F64 {
    crate::with_input(|i| i.wheel_x(target) as f64).unwrap_or(0.0)
}

/// Sets cursor icon: 0=default 1=pointer 2=text 3=grab 4=grabbing 5=resize-h
/// 6=resize-v 7=crosshair 8=not-allowed.
#[rtse::function(module = "input", value = "setCursor")]
pub fn set_cursor(target: U64, kind: I64) {
    crate::with_input(|i| i.set_cursor(target, kind));
}

/// 1 if key is in the given phase: 0=down (held, continuous), 1=pressed (fired
/// this frame, auto-repeat), 2=released (this frame). Neutral codes: 1-15
/// edit/nav, 100-125 A-Z, 130-139 0-9, 140-151 F1-F12. The .ts canvas wraps this
/// as keyDown/keyPressed/keyReleased. See input-system-design.md.
///
/// Símbolo ÚNICO que substitui os antigos KEY_DOWN/PRESSED/RELEASED.
#[rtse::function(module = "input", value = "key")]
pub fn key(target: U64, key: I64, phase: I64) -> I64 {
    crate::with_input(|i| i.key_state(target, key, phase) as i64).unwrap_or(0)
}

/// 1 if Ctrl is held now.
#[rtse::function(module = "input", value = "modCtrl")]
pub fn mod_ctrl(target: U64) -> I64 {
    crate::with_input(|i| i.modifiers(target).ctrl as i64).unwrap_or(0)
}

/// 1 if Shift is held now.
#[rtse::function(module = "input", value = "modShift")]
pub fn mod_shift(target: U64) -> I64 {
    crate::with_input(|i| i.modifiers(target).shift as i64).unwrap_or(0)
}

/// 1 if Alt is held now.
#[rtse::function(module = "input", value = "modAlt")]
pub fn mod_alt(target: U64) -> I64 {
    crate::with_input(|i| i.modifiers(target).alt as i64).unwrap_or(0)
}

/// 1 if Cmd/Super (Win/Cmd key) is held now (egui 'command', cross-platform).
#[rtse::function(module = "input", value = "modCmd")]
pub fn mod_cmd(target: U64) -> I64 {
    crate::with_input(|i| i.modifiers(target).cmd as i64).unwrap_or(0)
}

/// Text typed this frame (UTF-8), empty if none. Includes pasted text (Ctrl+V).
///
/// Retorno `Handle` + `#[ts("string")]`: é essa combinação que faz o motor
/// reboxar como TAG_STR (string usável no TS). Declarado `U64`/`number` o motor
/// reboxa como INTEIRO CRU — era o bug "dados de ponteiros no campo de texto".
#[rtse::function(module = "input", value = "textInput")]
#[ts("string")]
pub fn text_input(target: U64) -> Handle {
    let s = crate::with_input(|i| i.text_input(target)).unwrap_or_default();
    unsafe { __RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64) }
}

/// Put text on the OS clipboard (Ctrl+C).
#[rtse::function(module = "input", value = "copyText")]
pub fn copy_text(target: U64, text: &str) {
    if text.is_empty() {
        return;
    }
    crate::with_input(|i| i.copy_text(target, text));
}

unsafe extern "C" {
    fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64;
}

/// Monta o namespace `input` no Engine. As fns reportam o estado de input do
/// backend ativo (polling). O DOM/layout/app consome p/ hit-test + eventos.
pub fn register_input(e: &mut Engine) {
    e.module("input", |m| {
        m.doc(
            "Raw input from the active backend (polling). The DOM/layout hit-tests + dispatches \
             events; the backend doesn't know DOM nodes.",
        );
        m.registry(mouse_x_entry());
        m.registry(mouse_y_entry());
        m.registry(mouse_down_entry());
        m.registry(mouse_clicked_entry());
        m.registry(mouse_pressed_entry());
        m.registry(mouse_released_entry());
        m.registry(mouse_double_clicked_entry());
        m.registry(mouse_delta_x_entry());
        m.registry(mouse_delta_y_entry());
        m.registry(dragging_entry());
        m.registry(wheel_entry());
        m.registry(wheel_x_entry());
        m.registry(set_cursor_entry());
        m.registry(key_entry());
        m.registry(mod_ctrl_entry());
        m.registry(mod_shift_entry());
        m.registry(mod_alt_entry());
        m.registry(mod_cmd_entry());
        m.registry(text_input_entry());
        m.registry(copy_text_entry());
    });
}
