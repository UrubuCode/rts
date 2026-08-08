//! O canvas e os widgets-folha: o que o programa emite entre `beginFrame` e
//! `endFrame`.
//!
//! Nada aqui pinta. Cada membro enfileira um comando no `UiCtx` da janela, e o
//! `endFrame` os executa — que é por que `button` responde o clique do frame
//! ANTERIOR, casado por posição. A latência de um frame é da mecânica de fila do
//! `rts-egui` e não desta camada; está dita aqui porque é o que surpreende quem
//! lê `button()` esperando o clique de agora.

use rts_core_rwk::entry::Provided;

use crate::value::{self, handle, integer, number, text};
use crate::window::options;

/// Os membros de desenho de `rts:egui`.
pub const MEMBERS: &[(&str, Provided)] = &[
    ("drawRect", draw_rect),
    ("drawText", draw_text),
    ("drawLine", draw_line),
    ("measureText", measure_text),
    ("label", label),
    ("button", button),
    ("slider", slider),
    ("horizontalBegin", horizontal_begin),
    ("horizontalEnd", horizontal_end),
    ("html", html),
];

/// `drawRect(win, { x, y, w, h, fill, strokeW, stroke, radius })`.
///
/// Nove parâmetros na superfície antiga; um objeto aqui — ver `crate::value`.
/// Cores são `0xRRGGBBAA`.
extern "C" fn draw_rect(_e: u64, _t: u64, win: u64, spec: u64, _b: u64, _c: u64) -> u64 {
    let read = options(
        spec,
        &["x", "y", "w", "h", "fill", "strokeW", "stroke", "radius"],
        &[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
    );
    rts_egui::draw_rect(
        handle(win),
        read[0],
        read[1],
        read[2],
        read[3],
        read[4] as i64,
        read[5],
        read[6] as i64,
        read[7],
    );
    value::nothing()
}

/// `drawText(win, { x, y, text, color, size, flags })`.
///
/// `flags` é o bitmask 1=negrito 2=itálico 4=mono. O texto é lido à parte dos
/// números porque é o único campo que não é um.
extern "C" fn draw_text(_e: u64, _t: u64, win: u64, spec: u64, _b: u64, _c: u64) -> u64 {
    let read = options(
        spec,
        &["x", "y", "color", "size", "flags"],
        &[0.0, 0.0, 0.0, 14.0, 0.0],
    );
    let content = value::fields(spec, &["text"]);
    rts_egui::draw_text(
        handle(win),
        read[0],
        read[1],
        &text(content[0]),
        read[2] as i64,
        read[3],
        read[4] as i64,
    );
    value::nothing()
}

/// `drawLine(win, { x1, y1, x2, y2, w, color })`.
extern "C" fn draw_line(_e: u64, _t: u64, win: u64, spec: u64, _b: u64, _c: u64) -> u64 {
    let read = options(
        spec,
        &["x1", "y1", "x2", "y2", "w", "color"],
        &[0.0, 0.0, 0.0, 0.0, 1.0, 0.0],
    );
    rts_egui::draw_line(
        handle(win),
        read[0],
        read[1],
        read[2],
        read[3],
        read[4],
        read[5] as i64,
    );
    value::nothing()
}

/// `measureText(win, text, size, bold)` — largura em pontos, com a fonte real.
///
/// É a única operação do egui que sabe algo de layout, e existe porque medir
/// texto precisa das métricas da fonte, que o TS não tem. Síncrona.
extern "C" fn measure_text(_e: u64, _t: u64, win: u64, content: u64, size: u64, bold: u64) -> u64 {
    let width = rts_egui::measure_text(
        handle(win),
        &text(content),
        number(size, 14.0),
        integer(bold, 0),
    );
    value::from_number(width)
}

/// `label(win, text)`.
extern "C" fn label(_e: u64, _t: u64, win: u64, content: u64, _b: u64, _c: u64) -> u64 {
    rts_egui::label(handle(win), &text(content));
    value::nothing()
}

/// `button(win, label)` — se foi clicado. Ver a nota de latência no topo.
extern "C" fn button(_e: u64, _t: u64, win: u64, content: u64, _b: u64, _c: u64) -> u64 {
    value::from_bool(rts_egui::button(handle(win), &text(content)) != 0)
}

/// `slider(win, value, min, max)` — o valor, possivelmente arrastado.
extern "C" fn slider(_e: u64, _t: u64, win: u64, held: u64, low: u64, high: u64) -> u64 {
    let moved = rts_egui::slider(
        handle(win),
        number(held, 0.0),
        number(low, 0.0),
        number(high, 1.0),
    );
    value::from_number(moved)
}

/// `horizontalBegin(win)` — os widgets até o `horizontalEnd` ficam lado a lado.
extern "C" fn horizontal_begin(_e: u64, _t: u64, win: u64, _a: u64, _b: u64, _c: u64) -> u64 {
    rts_egui::horizontal_begin(handle(win));
    value::nothing()
}

/// `horizontalEnd(win)`.
extern "C" fn horizontal_end(_e: u64, _t: u64, win: u64, _a: u64, _b: u64, _c: u64) -> u64 {
    rts_egui::horizontal_end(handle(win));
    value::nothing()
}

/// `html(win, source)` — parseia e desenha, com a árvore retida na janela.
///
/// O caminho por handle de DOM (`render(win, dom)`) NÃO está aqui: ele lê o
/// store do `rts-dom`, que é alcançado pelo namespace `rts:dom` — e esse ainda
/// não existe no motor novo. Registrá-lo agora daria uma função que sempre
/// desenha nada, que é o modo de falhar que `rts-std-rwk` já recusou uma vez ao
/// não registrar um `rts:egui` de fachada.
extern "C" fn html(_e: u64, _t: u64, win: u64, source: u64, _b: u64, _c: u64) -> u64 {
    rts_egui::html(handle(win), &text(source));
    value::nothing()
}
