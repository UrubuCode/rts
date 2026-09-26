//! Two pieces of `quebra.rs`'s `wrap_runs`, moved out because that file sits
//! at the 500-line ceiling — a pure move, nothing changed. Unlike the cluster
//! macros (`fechar_cluster!`/`juntar!`/`glue_space!`, which stay in
//! `quebra.rs` because each captures a dozen locals of the cluster state),
//! both functions here only touch the ordinary line cursor (`cur`/`lines`/
//! `cur_w`/`at_line_start`) and take everything else as a parameter.
//!
//! - [`dividir_peca_que_nao_cabe`]: what `overflow-wrap`/`word-break` do once
//!   a piece has to split — walk grapheme-safe prefixes that fit, one line
//!   each.
//! - [`run_inteiro_cabe`]: the whole-run fast path — when a run both opens
//!   and closes a cluster and fits the current line as ONE string, skip the
//!   per-word scanner (`wrap-runs` was 38% of a large page's relayout at
//!   11 000 `text_width` calls a frame; this is why).

use super::*;

/// The one line `wrap_runs` emits when a flow produced no run at all — moved
/// from the tail of that function, verbatim.
pub(in crate::layout) fn linha_vazia() -> Segment {
    Segment {
        text: String::new(),
        text_width: 0.0,
        color: 0,
        bold: false,
        italic: false,
        deco: 0,
        owners: Vec::new(),
        atomic: None,
        ww: 0.0,
        wh: 0.0,
        lead_w: 0.0,
    }
}

/// A piece that does not fit is split at the widest prefix that does, one
/// line at a time — `overflow-wrap: break-word`/`word-break: break-all`.
/// Moved verbatim from inside `fechar_cluster!`'s emission loop.
#[allow(clippy::too_many_arguments)]
pub(in crate::layout) fn dividir_peca_que_nao_cabe(
    cur: &mut Vec<Segment>,
    lines: &mut Vec<Vec<Segment>>,
    cur_w: &mut f32,
    at_line_start: &mut bool,
    max_w: &mut dyn FnMut(usize) -> f32,
    run: &InlineRun,
    peca_run: usize,
    texto: &str,
    vao: f32,
    font_size: f32,
    mono: bool,
    ahem: bool,
    fontes: &super::run_font::Fontes,
    m: &dyn TextMeasurer,
) {
    let mut resto = texto;
    let mut lead = vao;
    while !resto.is_empty() {
        let disp = max_w(lines.len()) - *cur_w;
        let (mut n, mut w) = crate::inline_box::prefixo_que_cabe(
            resto, disp, font_size, mono, run.bold, run.italic, ahem, m,
        );
        if n == 0 && *at_line_start {
            // Numa caixa mais estreita que um glifo, nada cabe e descer de
            // linha não muda isso: sem um carácter forçado o laço não
            // termina. Transbordar um carácter é o que o browser também faz.
            n = resto.chars().next().map_or(0, char::len_utf8);
            w = fontes.largura(m, peca_run, &resto[..n], run.bold, run.italic);
        }
        if n == 0 {
            lines.push(std::mem::take(cur));
            *cur_w = 0.0;
            *at_line_start = true;
            continue;
        }
        push_segment(cur, run, &resto[..n], w, lead);
        lead = 0.0;
        *cur_w += w;
        *at_line_start = false;
        resto = &resto[n..];
        if !resto.is_empty() {
            lines.push(std::mem::take(cur));
            *cur_w = 0.0;
            *at_line_start = true;
        }
    }
}

/// The whole-run fast path (FAST PATH 2 in `quebra.rs`): when `run` both
/// OPENS a cluster (nothing pending) and CLOSES one (ends in whitespace),
/// measuring it as one string answers for every word inside — the caller
/// only reaches the per-word scanner when this returns `false`.
///
/// `abre_cluster`/`fecha_cluster` and the `word_spacing`/SHY guards are
/// decided by the caller, which already has `cluster`/`run.text` at hand;
/// this only does the measuring, the fit check, and the emission.
#[allow(clippy::too_many_arguments)]
pub(in crate::layout) fn run_inteiro_cabe(
    cur: &mut Vec<Segment>,
    cur_w: &mut f32,
    at_line_start: &mut bool,
    pending_space: &mut bool,
    espaco_de_fora: &mut bool,
    max_w: &mut dyn FnMut(usize) -> f32,
    lines_len: usize,
    run: &InlineRun,
    i: usize,
    normalizado: &str,
    fontes: &super::run_font::Fontes,
    m: &dyn TextMeasurer,
) -> bool {
    if normalizado.is_empty() {
        return false;
    }
    let w = fontes.largura(m, i, normalizado, run.bold, run.italic);
    if *at_line_start || *cur_w + w > max_w(lines_len) {
        return false;
    }
    let vao = if *pending_space && *espaco_de_fora {
        fontes.largura(m, i.wrapping_sub(1), " ", false, false)
    } else {
        0.0
    };
    push_segment(cur, run, normalizado, w, vao);
    *cur_w += w;
    *at_line_start = false;
    *pending_space = true;
    *espaco_de_fora = true;
    true
}
