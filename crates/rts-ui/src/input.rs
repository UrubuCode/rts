//! `rts:input` — mouse, teclado, roda e modificadores.
//!
//! # Por que é um módulo separado de `rts:egui`
//!
//! Porque a fonte de entrada é trocável e a janela não. O `rts-input` define o
//! trait `InputSource`; o `rts-egui` o implementa captando do winit e se
//! registra como fonte ativa. Um programa que fala `input.*` continua valendo se
//! a fonte virar SDL, um gamepad ou um harness de teste — e é por isso que a
//! captação nunca é chamada por aqui: este módulo só pergunta à fonte ativa.
//!
//! # Polling, não eventos
//!
//! O programa pergunta o estado a cada frame e decide o que fazer. A fonte não
//! interpreta: ela reporta onde o mouse está e quais teclas estão em que fase,
//! e quem faz hit-test e despacho é o DOM ou o jogo. Um modelo de eventos
//! precisaria chamar de volta para dentro do programa a partir de um nativo, que
//! é exatamente a operação que `docs/engine/authoring-natives.md` diz não poder
//! acontecer com o empréstimo do contexto na mão.
//!
//! # A janela é argumento de tudo
//!
//! `input.mouseX(win)`, não `input.mouseX()`. Com várias janelas abertas, "o
//! mouse" não é uma pergunta respondível — e uma fonte que respondesse pela
//! janela em foco daria um valor que muda sem o programa pedir.

use rts_core::entry::Provided;

use crate::value::{self, handle, integer, text};

/// Os membros de `rts:input`.
pub const MEMBERS: &[(&str, Provided)] = &[
    ("mouseX", mouse_x),
    ("mouseY", mouse_y),
    ("mouseDown", mouse_down),
    ("mouseClicked", mouse_clicked),
    ("mousePressed", mouse_pressed),
    ("mouseReleased", mouse_released),
    ("mouseDoubleClicked", mouse_double_clicked),
    ("mouseDeltaX", mouse_delta_x),
    ("mouseDeltaY", mouse_delta_y),
    ("dragging", dragging),
    ("wheel", wheel),
    ("wheelX", wheel_x),
    ("setCursor", set_cursor),
    ("key", key),
    ("modCtrl", mod_ctrl),
    ("modShift", mod_shift),
    ("modAlt", mod_alt),
    ("modCmd", mod_cmd),
    ("textInput", text_input),
    ("copyText", copy_text),
];

/// `input.mouseX(win)` — em pontos lógicos. `-1` quando não há fonte ativa ou a
/// janela não existe, que é distinguível de qualquer posição real.
extern "C" fn mouse_x(_e: u64, _t: u64, win: u64, _a: u64, _b: u64, _c: u64) -> u64 {
    let win = handle(win);
    value::from_number(rts_input::with_input(|source| source.mouse_pos(win).0 as f64).unwrap_or(-1.0))
}

/// `input.mouseY(win)`.
extern "C" fn mouse_y(_e: u64, _t: u64, win: u64, _a: u64, _b: u64, _c: u64) -> u64 {
    let win = handle(win);
    value::from_number(rts_input::with_input(|source| source.mouse_pos(win).1 as f64).unwrap_or(-1.0))
}

/// `input.mouseDown(win, button)` — o botão está pressionado AGORA.
extern "C" fn mouse_down(_e: u64, _t: u64, win: u64, button: u64, _b: u64, _c: u64) -> u64 {
    let (win, button) = (handle(win), integer(button, 0));
    value::from_bool(rts_input::with_input(|source| source.mouse_down(win, button)).unwrap_or(false))
}

/// `input.mouseClicked(win, button)` — clique completo neste frame.
extern "C" fn mouse_clicked(_e: u64, _t: u64, win: u64, button: u64, _b: u64, _c: u64) -> u64 {
    let (win, button) = (handle(win), integer(button, 0));
    value::from_bool(rts_input::with_input(|source| source.mouse_clicked(win, button)).unwrap_or(false))
}

/// `input.mousePressed(win, button)` — a borda de descida.
extern "C" fn mouse_pressed(_e: u64, _t: u64, win: u64, button: u64, _b: u64, _c: u64) -> u64 {
    let (win, button) = (handle(win), integer(button, 0));
    value::from_bool(rts_input::with_input(|source| source.mouse_pressed(win, button)).unwrap_or(false))
}

/// `input.mouseReleased(win, button)` — a borda de subida.
extern "C" fn mouse_released(_e: u64, _t: u64, win: u64, button: u64, _b: u64, _c: u64) -> u64 {
    let (win, button) = (handle(win), integer(button, 0));
    value::from_bool(rts_input::with_input(|source| source.mouse_released(win, button)).unwrap_or(false))
}

/// `input.mouseDoubleClicked(win, button)`.
extern "C" fn mouse_double_clicked(_e: u64, _t: u64, win: u64, button: u64, _b: u64, _c: u64) -> u64 {
    let (win, button) = (handle(win), integer(button, 0));
    value::from_bool(
        rts_input::with_input(|source| source.mouse_double_clicked(win, button)).unwrap_or(false),
    )
}

/// `input.mouseDeltaX(win)` — com o ponteiro travado (`egui.mouseLock`), o delta
/// CRU do dispositivo, que a borda da tela não limita.
extern "C" fn mouse_delta_x(_e: u64, _t: u64, win: u64, _a: u64, _b: u64, _c: u64) -> u64 {
    let win = handle(win);
    value::from_number(rts_input::with_input(|source| source.mouse_delta(win).0 as f64).unwrap_or(0.0))
}

/// `input.mouseDeltaY(win)`.
extern "C" fn mouse_delta_y(_e: u64, _t: u64, win: u64, _a: u64, _b: u64, _c: u64) -> u64 {
    let win = handle(win);
    value::from_number(rts_input::with_input(|source| source.mouse_delta(win).1 as f64).unwrap_or(0.0))
}

/// `input.dragging(win)`.
extern "C" fn dragging(_e: u64, _t: u64, win: u64, _a: u64, _b: u64, _c: u64) -> u64 {
    let win = handle(win);
    value::from_bool(rts_input::with_input(|source| source.dragging(win)).unwrap_or(false))
}

/// `input.wheel(win)` — o deslocamento vertical acumulado neste frame.
extern "C" fn wheel(_e: u64, _t: u64, win: u64, _a: u64, _b: u64, _c: u64) -> u64 {
    let win = handle(win);
    value::from_number(rts_input::with_input(|source| source.wheel(win) as f64).unwrap_or(0.0))
}

/// `input.wheelX(win)` — o horizontal.
extern "C" fn wheel_x(_e: u64, _t: u64, win: u64, _a: u64, _b: u64, _c: u64) -> u64 {
    let win = handle(win);
    value::from_number(rts_input::with_input(|source| source.wheel_x(win) as f64).unwrap_or(0.0))
}

/// `input.setCursor(win, kind)` — o único membro que ESCREVE, e por isso está
/// aqui e não numa superfície de janela: o cursor é do dispositivo apontador.
extern "C" fn set_cursor(_e: u64, _t: u64, win: u64, kind: u64, _b: u64, _c: u64) -> u64 {
    let (win, kind) = (handle(win), integer(kind, 0));
    rts_input::with_input(|source| source.set_cursor(win, kind));
    value::nothing()
}

/// `input.key(win, code, phase)` — `phase` 0=pressionada agora, 1=borda de
/// descida, 2=borda de subida. Os códigos são as constantes `KEY_*` do
/// `rts-input`, neutras de propósito: um código de tecla do winit ou do egui
/// prenderia o programa à fonte.
extern "C" fn key(_e: u64, _t: u64, win: u64, code: u64, phase: u64, _c: u64) -> u64 {
    let (win, code, phase) = (handle(win), integer(code, -1), integer(phase, 0));
    value::from_bool(rts_input::with_input(|source| source.key_state(win, code, phase)).unwrap_or(false))
}

/// `input.modCtrl(win)`.
extern "C" fn mod_ctrl(_e: u64, _t: u64, win: u64, _a: u64, _b: u64, _c: u64) -> u64 {
    let win = handle(win);
    value::from_bool(rts_input::with_input(|source| source.modifiers(win).ctrl).unwrap_or(false))
}

/// `input.modShift(win)`.
extern "C" fn mod_shift(_e: u64, _t: u64, win: u64, _a: u64, _b: u64, _c: u64) -> u64 {
    let win = handle(win);
    value::from_bool(rts_input::with_input(|source| source.modifiers(win).shift).unwrap_or(false))
}

/// `input.modAlt(win)`.
extern "C" fn mod_alt(_e: u64, _t: u64, win: u64, _a: u64, _b: u64, _c: u64) -> u64 {
    let win = handle(win);
    value::from_bool(rts_input::with_input(|source| source.modifiers(win).alt).unwrap_or(false))
}

/// `input.modCmd(win)` — Command no macOS, a tecla de janela nos demais.
extern "C" fn mod_cmd(_e: u64, _t: u64, win: u64, _a: u64, _b: u64, _c: u64) -> u64 {
    let win = handle(win);
    value::from_bool(rts_input::with_input(|source| source.modifiers(win).cmd).unwrap_or(false))
}

/// `input.textInput(win)` — o texto digitado neste frame, já composto pelo SO
/// (acentuação, IME). Vazio quando não houve nenhum.
///
/// É o que um campo de texto deve consumir, e não a tecla: `key` reporta uma
/// tecla física e não sabe o que um layout de teclado produz com ela.
extern "C" fn text_input(_e: u64, _t: u64, win: u64, _a: u64, _b: u64, _c: u64) -> u64 {
    let win = handle(win);
    let typed = rts_input::with_input(|source| source.text_input(win)).unwrap_or_default();
    value::from_text(&typed)
}

/// `input.copyText(win, text)` — põe o texto no clipboard do SO.
extern "C" fn copy_text(_e: u64, _t: u64, win: u64, content: u64, _b: u64, _c: u64) -> u64 {
    let win = handle(win);
    let content = text(content);
    rts_input::with_input(|source| source.copy_text(win, &content));
    value::nothing()
}
