//! Namespace ABI `input` — as fns `extern "C"` que o TS chama (`input.mouseX`,
//! `input.key`, `input.modCtrl`, …) e que despacham para o capturador ATIVO
//! (`with_input`). O backend concreto (egui) implementa o trait `InputSource`;
//! aqui só está a casca ABI + a tabela de membros (`register_input`).

use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

use AbiType::{F64, I64, StrPtr, U64 as Handle};

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

/// Estado de uma tecla numa fase (`phase`: 0=down, 1=pressed, 2=released — ver
/// `KEY_PHASE_*`). Símbolo ÚNICO que substitui os antigos KEY_DOWN/PRESSED/
/// RELEASED; o `.ts` (canvas) expõe os atalhos `keyDown`/`keyPressed`/`keyReleased`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_INPUT_KEY(target: u64, key: i64, phase: i64) -> i64 {
    crate::with_input(|i| i.key_state(target, key, phase) as i64).unwrap_or(0)
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

/// `input.copyText(target, text)` — coloca `text` no clipboard do SO (Ctrl+C).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NS_INPUT_COPY_TEXT(target: u64, ptr: *const u8, len: i64) {
    if ptr.is_null() || len <= 0 {
        return;
    }
    let text = unsafe {
        std::str::from_utf8_unchecked(std::slice::from_raw_parts(ptr, len as usize))
    };
    crate::with_input(|i| i.copy_text(target, text));
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
            "key",
            "__RTS_FN_NS_INPUT_KEY",
            Sig::new(vec![Handle, I64, I64], I64),
            "key(target: number, key: number, phase: number): number",
            "1 if key is in the given phase: 0=down (held, continuous), 1=pressed (fired this frame, auto-repeat), 2=released (this frame). Neutral codes: 1-15 edit/nav, 100-125 A-Z, 130-139 0-9, 140-151 F1-F12. The .ts canvas wraps this as keyDown/keyPressed/keyReleased. See input-system-design.md.",
            __RTS_FN_NS_INPUT_KEY as *const u8,
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
            // Retorno `AbiType::Handle` EXPLÍCITO (não o alias `U64 as Handle`): só
            // `Handle` literal + ts `: string` faz o motor reboxar como TAG_STR
            // (string usável no TS). Com o alias `U64` reboxava como INTEIRO CRU —
            // era o bug "dados de ponteiros no campo de texto".
            Sig::new(vec![Handle], AbiType::Handle),
            "textInput(target: number): string",
            "Text typed this frame (UTF-8), empty if none. Includes pasted text (Ctrl+V).",
            __RTS_FN_NS_INPUT_TEXT as *const u8,
        ))
        .member(func(
            "copyText",
            "__RTS_FN_NS_INPUT_COPY_TEXT",
            Sig::new(vec![Handle, StrPtr], AbiType::Void),
            "copyText(target: number, text: string): void",
            "Put text on the OS clipboard (Ctrl+C).",
            __RTS_FN_NS_INPUT_COPY_TEXT as *const u8,
        ))
        .done();
}
