//! PRESERVED white space in line breaking — `pre`, `pre-wrap` and
//! `break-spaces` (CSS Text 3 §4.1.3, §5.4.2).
//!
//! `quebra.rs` owns where a line breaks; this module only says what the
//! preserved text is made of and how wide a tab is, so the decision stays in
//! one place (`box-tree.md` §10). It lives apart because `quebra.rs` sits at
//! its 500-line ceiling, and because the collapsing scanner there answers a
//! different question: under `normal` a run of spaces is ONE separator that
//! may vanish, here every space is content that takes room.

use super::{AtomicKind, InlineRun, NodeIdx, Segment};
use crate::style::{ComputedStyle, WhiteSpace};

/// What `wrap_runs` needs to know of the container's `white-space` and
/// `tab-size`. One value built from the style, so each caller passes the
/// same answer instead of deriving booleans of its own.
#[derive(Clone, Copy)]
pub(in crate::layout) struct Spaces {
    ws: WhiteSpace,
    tab_size: f32,
}

impl Spaces {
    pub(in crate::layout) fn from_css(css: &ComputedStyle) -> Spaces {
        Spaces {
            ws: css.white_space.unwrap_or(WhiteSpace::Normal),
            tab_size: css.tab_size.unwrap_or(8.0).max(0.0),
        }
    }

    /// Spaces and tabs are content: they neither collapse nor vanish at a
    /// line edge.
    pub(in crate::layout) fn preserves(self) -> bool {
        self.ws.preserves_spaces()
    }

    pub(in crate::layout) fn preserves_newlines(self) -> bool {
        self.ws.preserves_newlines()
    }

    /// `break-spaces`: an opportunity after EVERY space and tab, and the
    /// spaces take room at the end of the line. Otherwise (`pre-wrap`, `pre`)
    /// the opportunity is after the whole sequence and the sequence HANGS.
    pub(in crate::layout) fn breaks_after_each(self) -> bool {
        self.ws == WhiteSpace::BreakSpaces
    }

    /// A tab advances to the next tab stop, measured from the start of the
    /// LINE (`pos` px into it) — not from the start of the run, which was the
    /// cut the old per-run expansion in `linha.rs` declared. The text is
    /// spaces so the painter, which has no idea of tabs, draws blank.
    pub(in crate::layout) fn tab(self, pos: f32, space: f32) -> (String, f32) {
        let w = tab_advance(pos, self.tab_size, space);
        (" ".repeat((w / space).round() as usize), w)
    }
}

/// How far a tab at `pos` advances: to the next multiple of `tab_size`
/// spaces of width `space`, skipping a stop closer than half a space
/// (CSS Text 3 §4.2). `tab-size: 0` renders tabs as nothing.
pub(in crate::layout) fn tab_advance(pos: f32, tab_size: f32, space: f32) -> f32 {
    let step = tab_size * space;
    if step <= 0.0 {
        return 0.0;
    }
    let w = step - pos.rem_euclid(step);
    if w < space * 0.5 { w + step } else { w }
}

/// One unit of preserved text.
#[derive(Clone, Copy)]
pub(in crate::layout) enum Token<'a> {
    Word(&'a str),
    Space,
    Tab,
    Break,
}

impl Token<'_> {
    pub(in crate::layout) fn is_white(&self) -> bool {
        matches!(self, Token::Space | Token::Tab)
    }
}

/// Splits preserved text into words, single spaces, tabs and forced breaks.
/// A space is one unit per character because `break-spaces` breaks between
/// any two of them; `pre-wrap` joins them again by not closing the cluster
/// until the sequence ends.
pub(in crate::layout) fn tokens(text: &str) -> impl Iterator<Item = Token<'_>> {
    let mut rest = text;
    std::iter::from_fn(move || {
        let c = rest.chars().next()?;
        let is_white = |c: char| c == '\t' || c == '\n' || crate::inline_box::e_espaco_css(c);
        if is_white(c) {
            rest = &rest[c.len_utf8()..];
            return Some(match c {
                '\n' => Token::Break,
                '\t' => Token::Tab,
                _ => Token::Space,
            });
        }
        let end = rest.find(is_white).unwrap_or(rest.len());
        let word = &rest[..end];
        rest = &rest[end..];
        Some(Token::Word(word))
    })
}

/// Drops the spaces that HANG at the end of a line closed by a soft wrap
/// (`pre-wrap`, `pre`): they overflow rather than take room, and CSS Text 3
/// §4.1.3 says a hanging glyph is not considered when aligning the line, so
/// `text-align` must not see them. Removing them from the segment is the whole
/// of that here — the painter draws spaces as nothing, and keeping them with a
/// separate "hanging width" would be a second width every consumer of
/// `text_width` had to learn to subtract. `break-spaces` never hangs
/// (`hang` is 0 there), and a sequence before a FORCED break is left alone:
/// it only hangs conditionally. Only the last segment is trimmed; a hanging
/// sequence split across two inline elements keeps the part in the first.
///
/// ⚠️ CUT: only text with NO inline owner is trimmed. A space inside a
/// `<span>` still paints the span's background where it hangs
/// (`white-space-pre-wrap-trailing-spaces-014/015`), so removing it there
/// erases a box. The right shape is a hanging width that alignment
/// subtracts, and alignment lives in `linha.rs`, outside this change.
pub(in crate::layout) fn trim_hanging(mut line: Vec<Segment>, (hang, space): (f32, f32)) -> Vec<Segment> {
    if hang <= 0.0 || space <= 0.0 {
        return line;
    }
    if let Some(last) = line.last_mut().filter(|s| s.atomic.is_none() && s.owners.is_empty()) {
        let n = (hang / space).round() as usize;
        let whites = last.text.chars().rev().take_while(|&c| c == ' ').count();
        if n > 0 && whites >= n && last.text.len() > n {
            last.text.truncate(last.text.len() - n);
            last.text_width -= hang;
        }
    }
    line
}

/// The segment an atomic inline (or a zero-width marker) occupies on a line.
/// Three sites of `wrap_runs` built it field by field; it moved here to give
/// `quebra.rs` room under its ceiling for the preserved-space branch.
pub(in crate::layout) fn atomic_segment(
    run: &InlineRun,
    atom: (NodeIdx, crate::boxes::BoxId, AtomicKind),
    ww: f32,
    wh: f32,
    lead_w: f32,
) -> Segment {
    Segment {
        text: String::new(),
        text_width: 0.0,
        color: run.color,
        bold: false,
        italic: false,
        deco: 0,
        owners: run.owners.clone(),
        atomic: Some(atom),
        ww,
        wh,
        lead_w,
    }
}
