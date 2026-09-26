//! Two pieces of `line_break.rs`'s `wrap_runs`, moved out because that file sits
//! at the 500-line ceiling — a pure move, nothing changed. Unlike the cluster
//! macros (`fechar_cluster!`/`juntar!`/`glue_space!`, which stay in
//! `line_break.rs` because each captures a dozen locals of the cluster state),
//! both functions here only touch the ordinary line cursor (`cur`/`lines`/
//! `cur_w`/`at_line_start`) and take everything else as a parameter.
//!
//! - [`split_piece_that_does_not_fit`]: what `overflow-wrap`/`word-break` do once
//!   a piece has to split — walk grapheme-safe prefixes that fit, one line
//!   each.
//! - [`whole_run_fits`]: the whole-run fast path — when a run both opens
//!   and closes a cluster and fits the current line as ONE string, skip the
//!   per-word scanner (`wrap-runs` was 38% of a large page's relayout at
//!   11 000 `text_width` calls a frame; this is why).

use super::*;

/// The one line `wrap_runs` emits when a flow produced no run at all — moved
/// from the tail of that function, verbatim.
pub(in crate::layout) fn empty_line() -> Segment {
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
pub(in crate::layout) fn split_piece_that_does_not_fit(
    cur: &mut Vec<Segment>,
    lines: &mut Vec<Vec<Segment>>,
    cur_w: &mut f32,
    at_line_start: &mut bool,
    max_w: &mut dyn FnMut(usize) -> f32,
    run: &InlineRun,
    piece_run: usize,
    text: &str,
    gap: f32,
    font_size: f32,
    mono: bool,
    ahem: bool,
    fonts: &super::run_font::Fonts,
    m: &dyn TextMeasurer,
) {
    let mut rest = text;
    let mut lead = gap;
    while !rest.is_empty() {
        let disp = max_w(lines.len()) - *cur_w;
        let (mut n, mut w) = crate::inline_box::prefixo_que_cabe(
            rest, disp, font_size, mono, run.bold, run.italic, ahem, m,
        );
        if n == 0 && *at_line_start {
            // Numa caixa mais estreita que um glifo, nada cabe e descer de
            // linha não muda isso: sem um carácter forçado o laço não
            // termina. Transbordar um carácter é o que o browser também faz.
            n = rest.chars().next().map_or(0, char::len_utf8);
            w = fonts.width(m, piece_run, &rest[..n], run.bold, run.italic);
        }
        if n == 0 {
            lines.push(std::mem::take(cur));
            *cur_w = 0.0;
            *at_line_start = true;
            continue;
        }
        push_segment(cur, run, &rest[..n], w, lead);
        lead = 0.0;
        *cur_w += w;
        *at_line_start = false;
        rest = &rest[n..];
        if !rest.is_empty() {
            lines.push(std::mem::take(cur));
            *cur_w = 0.0;
            *at_line_start = true;
        }
    }
}

/// The whole-run fast path (FAST PATH 2 in `line_break.rs`): when `run` both
/// OPENS a cluster (nothing pending) and CLOSES one (ends in whitespace),
/// measuring it as one string answers for every word inside — the caller
/// only reaches the per-word scanner when this returns `false`.
///
/// `abre_cluster`/`fecha_cluster` and the `word_spacing`/SHY guards are
/// decided by the caller, which already has `cluster`/`run.text` at hand;
/// this only does the measuring, the fit check, and the emission.
#[allow(clippy::too_many_arguments)]
pub(in crate::layout) fn whole_run_fits(
    cur: &mut Vec<Segment>,
    cur_w: &mut f32,
    at_line_start: &mut bool,
    pending_space: &mut bool,
    space_outside: &mut bool,
    max_w: &mut dyn FnMut(usize) -> f32,
    lines_len: usize,
    run: &InlineRun,
    i: usize,
    normalized: &str,
    fonts: &super::run_font::Fonts,
    m: &dyn TextMeasurer,
) -> bool {
    if normalized.is_empty() {
        return false;
    }
    let w = fonts.width(m, i, normalized, run.bold, run.italic);
    if *at_line_start || *cur_w + w > max_w(lines_len) {
        return false;
    }
    let gap = if *pending_space && *space_outside {
        fonts.width(m, i.wrapping_sub(1), " ", false, false)
    } else {
        0.0
    };
    push_segment(cur, run, normalized, w, gap);
    *cur_w += w;
    *at_line_start = false;
    *pending_space = true;
    *space_outside = true;
    true
}

/// The candidate line measured the way it is SHAPED: when the per-piece sum
/// says a cluster does not fit, the line's last segment, the separating space
/// and the cluster are measured again as ONE string, and the cluster stays if
/// that fits. Returns the line's total width with the cluster in it.
///
/// Why: a measurer is not additive. Kerning pairs cross a space (Times New
/// Roman kerns " T"), so "AVATAR" + " " + "Toy" + " " + "To." is 0.58 px
/// wider than "AVATAR Toy To." at 16px — and the max-content width
/// (`measure/text.rs`) measures the whole string, as CSS Sizing 3 §4.1.1 and
/// Blink do. A shrink-to-fit box sized by that width then wrapped its own last
/// word. Summing pieces in `measure/text.rs` instead was the alternative: it
/// would size every box by a width no line is painted at.
///
/// Only asked at a would-be break, so the per-word cost of the breaker is
/// unchanged. The opposite error — a positive kern making the whole WIDER than
/// the sum — is not caught here, since a cluster that fits by the sum never
/// asks. `None` also when the cluster cannot merge into the last segment
/// (another font, colour or owner, an atomic, a soft hyphen, word-spacing).
#[allow(clippy::too_many_arguments)]
pub(in crate::layout) fn shaped_join_fits<'t>(
    cur: &[Segment],
    cur_w: f32,
    runs: &[InlineRun],
    pieces: impl Iterator<Item = Option<(usize, &'t str)>>,
    sep_run: Option<usize>,
    (hang, word_spacing, available): (f32, f32, f32),
    fonts: &super::run_font::Fonts,
    m: &dyn TextMeasurer,
) -> Option<f32> {
    let last = cur.last().filter(|s| s.atomic.is_none() && !s.text.is_empty())?;
    let merges = |i: usize| {
        runs.get(i).is_some_and(|r| {
            r.atomic.is_none() && r.color == last.color && r.bold == last.bold && r.italic == last.italic && r.deco == last.deco && r.owners == last.owners
        })
    };
    if word_spacing != 0.0 || sep_run.is_some_and(|i| !merges(i)) {
        return None;
    }
    let mut joined = last.text.clone();
    if sep_run.is_some() {
        joined.push(' ');
    }
    let mut run = None;
    for piece in pieces {
        let (i, text) = piece?;
        if !merges(i) || text.contains(super::hyphen::SHY) {
            return None;
        }
        run = Some(i);
        joined.push_str(text);
    }
    let whole = cur_w - last.text_width + fonts.width(m, run?, &joined, last.bold, last.italic);
    (whole - hang <= available).then_some(whole)
}
