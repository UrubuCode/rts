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

/// What `wrap_runs` needs to know of `white-space` and `tab-size`, per RUN.
///
/// Both are inherited and apply to each inline box (CSS Text 3 §3), so a
/// `<span style="white-space:pre">` inside a `normal` paragraph keeps ITS
/// spaces while the text around it collapses. One value per call — the
/// container's — was the shape before, and it made the span's declaration
/// invisible (WPT `white-space/*-051/052`).
///
/// A run does not carry the value in a new field: like its font
/// (`fonte_do_trecho.rs`), it is the computed style of the INNERMOST owner,
/// which already inherits from everything above it; a run with no owner is
/// the container's. A field on `InlineRun` was the alternative and lost
/// because every constructor of a run (floats, anchors, pseudo boxes) would
/// have to fill it with the same lookup this does once.
pub(in crate::layout) struct Spaces {
    base: WhiteSpaceRegime,
    per_run: Vec<WhiteSpaceRegime>,
}

impl Spaces {
    /// One value for every run: a flow whose text has no inline owners, like
    /// a generated box's own text.
    pub(in crate::layout) fn from_css(css: &ComputedStyle) -> Spaces {
        Spaces { base: WhiteSpaceRegime::from_css(css), per_run: Vec::new() }
    }

    pub(in crate::layout) fn from_flow(dom: &crate::Dom, runs: &[InlineRun], css: &ComputedStyle) -> Spaces {
        let base = WhiteSpaceRegime::from_css(css);
        let per_run = runs
            .iter()
            .map(|r| r.owners.last().and_then(|&o| dom.computed_style_idx(o)).map_or(base, |c| WhiteSpaceRegime::from_css(&c)))
            .collect();
        Spaces { base, per_run }
    }

    /// The value of run `i`; any index past the runs is the container's.
    pub(in crate::layout) fn of(&self, i: usize) -> WhiteSpaceRegime {
        self.per_run.get(i).copied().unwrap_or(self.base)
    }
}

/// One element's `white-space` and `tab-size`.
#[derive(Clone, Copy)]
pub(in crate::layout) struct WhiteSpaceRegime {
    ws: WhiteSpace,
    tab_size: f32,
}

impl WhiteSpaceRegime {
    pub(in crate::layout) fn from_css(css: &ComputedStyle) -> WhiteSpaceRegime {
        WhiteSpaceRegime {
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

    /// Does a run under THIS regime ever offer an automatic soft-wrap
    /// opportunity? False for `nowrap` and `pre` (CSS Text 3 §4.1.3 — both
    /// forbid line breaking except at a forced `\n`, which `pre` still
    /// preserves). This is what makes `white-space` apply per INLINE box
    /// rather than once for the whole flow: `wrap_runs` asks it of the RUN
    /// that owns the boundary being closed, not of the container, so
    /// `<span style="white-space:nowrap">` glues only its own spaces while
    /// the text around it keeps wrapping normally, and a `normal` span
    /// inside a `<pre>` wraps on its own even though the `<pre>` around it
    /// never would.
    pub(in crate::layout) fn wraps(self) -> bool {
        !matches!(self.ws, WhiteSpace::Nowrap | WhiteSpace::Pre)
    }

    /// A tab advances to the next tab stop, measured from `pos` — not from the
    /// start of the run, which was the cut the old per-run expansion in
    /// `linha.rs` declared. The text is spaces so the painter, which has no
    /// idea of tabs, draws blank.
    ///
    /// `pos` itself is `wrap_runs`'s `line_offset(i) + cur_w + cluster_w`: CSS
    /// Text 3 §4.2 counts a tab stop from "the start edge of the line box's
    /// containing block" — the block's CONTENT edge — not from where line `i`
    /// happens to start. A float shortens a line from the left without moving
    /// the content edge, so `line_offset` (the band's left edge minus the
    /// content edge, from `linha.rs`'s `offset_da_linha`) is what keeps a
    /// tab-stop-with-float line agreeing with a plain one on where stop N is.
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

/// CSS Text 3 §4.1.3 phase II: a collapsible space is removed at the START of
/// a line under EVERY `white-space` value — `nowrap` and `pre` withhold the
/// soft-wrap OPPORTUNITY a space would otherwise offer (`WhiteSpaceRegime::
/// wraps`), they do not stop the space from collapsing. `quebra.rs`'s
/// wrapping branches get this for free: `fechar_cluster!`'s `sep =
/// cluster_espaco && !at_line_start` already drops a `pending_space` that
/// would open a line. A non-wrapping run's space skips that path — it is
/// glued straight into the open cluster as an ordinary piece instead of
/// closing it (`glue_space!`, so a later wrap-allowed boundary cannot split
/// the glued run in the middle) — so it needs the same guard applied at the
/// point it would be glued: nothing open (`cluster.is_empty()`) and nothing
/// on the line yet (`at_line_start`) means this space opens the line and is
/// dropped rather than becoming its first piece.
pub(in crate::layout) fn collapses_at_line_start(cluster_is_empty: bool, at_line_start: bool) -> bool {
    cluster_is_empty && at_line_start
}

/// The other half of the same rule: one run of collapsible whitespace never
/// contributes MORE than one collapsed space, even when it is split across a
/// wrapping run and a non-wrapping one at the boundary between them (CSS Text
/// 3 §4.1.1 — adjacent collapsible spaces collapse to a single space
/// regardless of how many elements or text nodes they are spread across).
///
/// `wrap_runs` already tracks this with `pending_space`: a WRAPPING run that
/// ends in whitespace sets it and clears its own cluster (`fechar_cluster!`
/// then `pending_space = true`), and the NEXT run that opens a cluster
/// (`juntar!`) turns that flag into the separator ONE cluster is drawn
/// with. A non-wrapping run's OWN leading/whole-run whitespace is glued as an
/// ordinary piece instead (`glue_space!`) so a later wrap-allowed boundary
/// cannot split the glued run — but if `pending_space` is already true when
/// that piece is about to be glued, the piece and the pending flag are the
/// SAME run of whitespace on either side of the regime change, and `juntar!`
/// would otherwise turn the flag into a SECOND separator on top of the piece
/// (`float-nowrap-4.html`, WPT `CSS2/floats`: "Some" + this glued run's own
/// leading space landed as ONE-AND-A-HALF spaces of width, which was enough
/// extra room to push the float's anchor a whole line down). The caller
/// consumes `pending_space` before gluing in that case, so the glued piece
/// alone stands for the collapsed run.
pub(in crate::layout) fn glued_space_absorbs_pending(cluster_is_empty: bool, pending_space: bool) -> bool {
    cluster_is_empty && pending_space
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
