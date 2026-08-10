//! A janela e o frame: o que abre, o que bombeia e o que apresenta.
//!
//! Todo membro daqui recebe o handle de janela como primeiro argumento, que é o
//! que o `openWindow` respondeu. O handle é opaco de propósito — é a chave de um
//! `thread_local` no `rts-egui`, e o `UiCtx` que ele nomeia é `!Send`, então a
//! thread que abriu a janela é a única que pode bombear o loop dela.

use rts_core::entry::Provided;

use crate::value::{self, fields, handle, integer, number, text};

/// Os membros que a janela e o frame contribuem para `rts:egui`.
pub const MEMBERS: &[(&str, Provided)] = &[
    ("openWindow", open_window),
    ("pump", pump),
    ("isOpen", is_open),
    ("close", close),
    ("moveWindow", move_window),
    ("setNextWindowPos", set_next_window_pos),
    ("setVsync", set_vsync),
    ("mouseLock", mouse_lock),
    ("snapshot", snapshot),
    ("beginFrame", begin_frame),
    ("endFrame", end_frame),
    ("winWidth", win_width),
    ("winHeight", win_height),
];

/// `openWindow(title, width, height, config)` — o handle, ou `0` em falha.
///
/// `config` é o mesmo bitfield da superfície antiga (bit0 alta performance, bit1
/// memória, bit2 limites altos, bit3 glow, bit4 transparente, bit5 sem
/// decoração), mantido porque é o que os programas existentes passam e porque
/// traduzi-lo aqui seria uma segunda grafia da mesma decisão.
extern "C" fn open_window(_e: u64, _t: u64, title: u64, width: u64, height: u64, config: u64) -> u64 {
    let opened = rts_egui::open_window(
        &text(title),
        integer(width, 960),
        integer(height, 600),
        integer(config, 0),
    );
    value::from_number(opened as f64)
}

/// `pump(win)` — processa os eventos pendentes do SO. `true` enquanto o programa
/// deve continuar.
///
/// A superfície antiga respondia `0 = continuar`, que é a convenção de código de
/// saída de um processo e o inverso do que um `while` lê. Aqui responde o que a
/// palavra diz.
extern "C" fn pump(_e: u64, _t: u64, win: u64, _a: u64, _b: u64, _c: u64) -> u64 {
    value::from_bool(rts_egui::pump(handle(win)) == 0)
}

/// `isOpen(win)`.
extern "C" fn is_open(_e: u64, _t: u64, win: u64, _a: u64, _b: u64, _c: u64) -> u64 {
    value::from_bool(rts_egui::is_open(handle(win)) != 0)
}

/// `close(win)` — destrói a janela. O event loop do processo sobrevive: o winit
/// não permite recriá-lo.
extern "C" fn close(_e: u64, _t: u64, win: u64, _a: u64, _b: u64, _c: u64) -> u64 {
    rts_egui::close(handle(win));
    value::nothing()
}

/// `moveWindow(win, x, y)` — posição absoluta na área de trabalho, em pixels
/// físicos, o que é como se escolhe um monitor.
extern "C" fn move_window(_e: u64, _t: u64, win: u64, x: u64, y: u64, _c: u64) -> u64 {
    rts_egui::move_window(handle(win), integer(x, 0), integer(y, 0));
    value::nothing()
}

/// `setNextWindowPos(x, y)` — onde a PRÓXIMA janela nasce. Mais confiável que
/// mover depois, porque a janela nunca chega a aparecer no lugar errado.
extern "C" fn set_next_window_pos(_e: u64, _t: u64, x: u64, y: u64, _b: u64, _c: u64) -> u64 {
    rts_egui::set_next_pos(integer(x, 0), integer(y, 0));
    value::nothing()
}

/// `setVsync(win, on)` — `false` é o opt-out explícito: quem desliga assume o
/// throttle e o consumo.
extern "C" fn set_vsync(_e: u64, _t: u64, win: u64, on: u64, _b: u64, _c: u64) -> u64 {
    rts_egui::set_vsync(handle(win), integer(on, 1));
    value::nothing()
}

/// `mouseLock(win, on)` — confina e esconde o cursor, e passa
/// `input.mouseDeltaX/Y` a reportar deltas CRUS do dispositivo, que é o que um
/// FPS precisa e o que a borda da tela estraga.
extern "C" fn mouse_lock(_e: u64, _t: u64, win: u64, on: u64, _b: u64, _c: u64) -> u64 {
    rts_egui::mouse_lock(handle(win), integer(on, 0));
    value::nothing()
}

/// `snapshot(win, path)` — agenda um PPM do próximo `endFrame`, para asserir que
/// um frame não está em branco sem olhar para ele. Só no backend glow.
extern "C" fn snapshot(_e: u64, _t: u64, win: u64, path: u64, _b: u64, _c: u64) -> u64 {
    rts_egui::snapshot(handle(win), &text(path));
    value::nothing()
}

/// `beginFrame(win)` — abre o pass: recolhe o input e zera a fila de comandos.
extern "C" fn begin_frame(_e: u64, _t: u64, win: u64, _a: u64, _b: u64, _c: u64) -> u64 {
    rts_egui::begin_frame(handle(win));
    value::nothing()
}

/// `endFrame(win)` — drena a fila, tesselá e apresenta.
extern "C" fn end_frame(_e: u64, _t: u64, win: u64, _a: u64, _b: u64, _c: u64) -> u64 {
    rts_egui::end_frame(handle(win));
    value::nothing()
}

/// `winWidth(win)` — largura LÓGICA em pontos, que acompanha o resize.
extern "C" fn win_width(_e: u64, _t: u64, win: u64, _a: u64, _b: u64, _c: u64) -> u64 {
    value::from_number(rts_egui::win_width(handle(win)))
}

/// `winHeight(win)`.
extern "C" fn win_height(_e: u64, _t: u64, win: u64, _a: u64, _b: u64, _c: u64) -> u64 {
    value::from_number(rts_egui::win_height(handle(win)))
}

/// Lê um objeto de opções pelos nomes dados, com defaults por posição.
///
/// Vive aqui e é usado pelos módulos de desenho porque é a mecânica da decisão
/// que `value` documenta: um objeto na segunda posição, campo ausente vale o
/// default.
pub fn options(object: u64, names: &[&str], defaults: &[f64]) -> Vec<f64> {
    let read = fields(object, names);
    read.iter()
        .zip(defaults)
        .map(|(value, default)| number(*value, *default))
        .collect()
}
