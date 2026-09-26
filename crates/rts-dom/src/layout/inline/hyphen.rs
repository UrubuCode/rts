//! `hyphens: manual` — o HÍFEN SUAVE (`&shy;`, U+00AD) como oportunidade de
//! quebra dentro de uma palavra.
//!
//! Três regras, e são as do CSS Text 3 §5.4 lidas contra o Blink
//! (`tests/css/claude-hyphens-manual.html`):
//!
//! 1. o U+00AD não ocupa largura e não se pinta enquanto a palavra não quebra
//!    ali — por isso o texto é medido e emitido SEM ele (`sem_shy`);
//! 2. quando a palavra não cabe, a linha corrente recebe o maior prefixo que
//!    termina num hífen suave e cabe COM o hífen visível "-" pintado no fim;
//! 3. com `hyphens: none` o U+00AD é ignorado de todo — a palavra fica
//!    inteira e transborda, como qualquer palavra sem oportunidade.
//!
//! `hyphens: auto` (dicionário) NÃO existe: fica igual a `manual`, que é o que
//! um browser sem dicionário para a língua também faz. A alternativa — trazer
//! um dicionário de padrões — é uma dependência e uma decisão de tamanho que o
//! PLAN §5 pede que se tome à parte.
//!
//! Vive fora de `line_break.rs` porque esse ficheiro está a 30 linhas do teto de
//! 500 e o `wrap_runs` não se parte por dentro.

use super::*;
use std::borrow::Cow;

pub(in crate::layout) const SHY: char = '\u{00AD}';

/// O texto que uma peça guarda: com os hífens suaves quando `hyphens` os deixa
/// ser oportunidade, sem eles quando é `none` (aí nunca serão vistos).
pub(in crate::layout) fn piece_text(s: &str, manual_hyphen: bool) -> String {
    match manual_hyphen {
        true => s.to_string(),
        false => sem_shy(s).into_owned(),
    }
}

/// O texto sem hífens suaves — o que se mede e o que se pinta.
pub(in crate::layout) fn sem_shy(s: &str) -> Cow<'_, str> {
    match s.contains(SHY) {
        true => Cow::Owned(s.chars().filter(|&c| c != SHY).collect()),
        false => Cow::Borrowed(s),
    }
}

/// O maior prefixo de `text` que termina num hífen suave e cuja largura,
/// com o "-" visível, cabe em `disp`. Devolve o prefixo já com o hífen, a
/// largura dele e o resto (que pode ainda ter hífens suaves).
fn longest_fitting_prefix(
    text: &str,
    disp: f32,
    font_size: f32,
    mono: bool,
    bold: bool,
    italic: bool,
    // Ver `inline_box::prefixo_que_cabe`: Ahem mede pelo avanço exato.
    ahem: bool,
    m: &dyn TextMeasurer,
) -> Option<(String, f32, String)> {
    let mut best: Option<(String, f32, String)> = None;
    for (i, c) in text.char_indices() {
        if c != SHY {
            continue;
        }
        let mut prefix = sem_shy(&text[..i]).into_owned();
        if prefix.is_empty() {
            continue;
        }
        prefix.push('-');
        let w = if ahem {
            prefix.chars().count() as f32 * font_size
        } else {
            m.text_width(&prefix, font_size, mono, bold, italic)
        };
        if w <= disp {
            best = Some((prefix, w, text[i + SHY.len_utf8()..].to_string()));
        } else {
            break;
        }
    }
    best
}

/// Emite `text` (que contém pelo menos um hífen suave) a partir da linha
/// corrente, quebrando nos hífens suaves sempre que o que resta não cabe.
/// Devolve `true` se emitiu tudo; `false` se não conseguiu quebrar em lado
/// nenhum (a palavra segue então o caminho normal, inteira).
#[allow(clippy::too_many_arguments)]
pub(in crate::layout) fn emit_with_hyphen(
    cur: &mut Vec<Segment>,
    lines: &mut Vec<Vec<Segment>>,
    cur_w: &mut f32,
    at_line_start: &mut bool,
    run: &InlineRun,
    text: &str,
    whole_width: f32,
    // largura do espaço que separa esta palavra da anterior na linha corrente
    // (0 no início de linha), e se esse espaço veio de um run ANTERIOR — a
    // mesma distinção do `lead_w` do `Segment`.
    sep_w: f32,
    gap_outside: bool,
    max_w: &mut dyn FnMut(usize) -> f32,
    font_size: f32,
    mono: bool,
    ahem: bool,
    m: &dyn TextMeasurer,
) -> bool {
    let disp = max_w(lines.len()) - *cur_w;
    if whole_width + sep_w <= disp {
        return false;
    }
    let Some((prefix, w, rest)) = longest_fitting_prefix(
        text, disp - sep_w, font_size, mono, run.bold, run.italic, ahem, m,
    ) else {
        return false;
    };
    // O espaço viaja no texto e `push_segment` decide se vira vão, exactamente
    // como no caminho sem hífen.
    let (emitted_text, emitted_w, lead) = match sep_w > 0.0 {
        true => (format!(" {prefix}"), w + sep_w, if gap_outside { sep_w } else { 0.0 }),
        false => (prefix, w, 0.0),
    };
    push_segment(cur, run, &emitted_text, emitted_w, lead);
    lines.push(std::mem::take(cur));
    *cur_w = 0.0;
    *at_line_start = true;
    let mut rest = rest;
    loop {
        let clean = sem_shy(&rest).into_owned();
        let w = if ahem {
            clean.chars().count() as f32 * font_size
        } else {
            m.text_width(&clean, font_size, mono, run.bold, run.italic)
        };
        let disp = max_w(lines.len());
        if w <= disp || !rest.contains(SHY) {
            push_segment(cur, run, &clean, w, 0.0);
            *cur_w += w;
            *at_line_start = false;
            return true;
        }
        match longest_fitting_prefix(&rest, disp, font_size, mono, run.bold, run.italic, ahem, m) {
            Some((prefix, w, r)) => {
                push_segment(cur, run, &prefix, w, 0.0);
                lines.push(std::mem::take(cur));
                rest = r;
            }
            None => {
                // nem o primeiro pedaço cabe numa linha inteira: transborda,
                // como o browser.
                push_segment(cur, run, &clean, w, 0.0);
                *cur_w += w;
                *at_line_start = false;
                return true;
            }
        }
    }
}
