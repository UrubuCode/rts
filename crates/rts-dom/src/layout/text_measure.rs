//! The TEXT half of the intrinsic widths: the lines an inline formatting
//! context makes when only forced breaks break (max-content, CSS Sizing 3
//! §4.1.1) or when every soft wrap opportunity does too (min-content).
//!
//! **Why per run.** The walk this replaces flattened an element's text and
//! collapsed it by the CONTAINER's `white-space`, so
//! `a <span style="white-space:pre">B&#10;C</span> d` measured as the one line
//! "a B C d". CSS Text 3 §4.1 applies each run's own `white-space` — the
//! nearest element that declares it, inherited — so the span's newline ends a
//! line and its spaces keep their width while the text around it collapses.
//!
//! **Whitespace is not decided here** (`box-tree.md` §10). What a preserved
//! run is made of — words, single spaces, tabs, forced breaks — and how wide a
//! tab is are `preserved_spaces`'s answers (`tokens`, `WhiteSpaceRegime::tab`), the
//! same the line breaker asks; this module only adds them up.
//!
//! **Text is buffered, not summed per word.** Consecutive text of one style
//! is measured as ONE string, as the old whole-text measurement did, because a
//! measurer is not obliged to be additive (rounding, kerning) and a width that
//! moved by a hundredth would move every shrink-to-fit box that contains text.

use super::preserved_spaces::{tokens, Token, WhiteSpaceRegime};
use super::*;

/// The font a run of text is measured in: everything the measurer and the
/// two spacings need. Two runs with an equal `RunStyle` share one buffer.
#[derive(Clone, PartialEq)]
struct RunStyle {
    font: f32,
    family: Option<String>,
    mono: bool,
    bold: bool,
    italic: bool,
    letter: f32,
    word: f32,
}

/// The lines of one inline formatting context, built run by run.
pub(in crate::layout) struct Lines {
    /// `true`: every soft wrap opportunity breaks (min-content).
    min: bool,
    /// Width of the current line already measured (everything before `buf`).
    line: f32,
    buf: String,
    style: Option<RunStyle>,
    /// A collapsible space seen but not yet placed: it only takes room if
    /// content follows on the same line (§4.1.1 phase II removes it at the
    /// end), and it keeps the style of the run it came from.
    space: Option<RunStyle>,
    /// The line so far ends in collapsible white space, or is empty: a further
    /// collapsible space collapses into it (§4.1.1 phase I, across runs).
    after_space: bool,
    widest: f32,
}

impl Lines {
    pub(in crate::layout) fn new(min: bool) -> Lines {
        Lines { min, line: 0.0, buf: String::new(), style: None, space: None, after_space: true, widest: 0.0 }
    }

    /// The widest line, once the last one is closed.
    pub(in crate::layout) fn finish(mut self, ctx: &LayoutCtx) -> f32 {
        self.forced_break(ctx);
        self.widest
    }

    /// A forced break (`<br>`, a preserved newline): the line ends, and a
    /// pending collapsible space ends with it.
    pub(in crate::layout) fn forced_break(&mut self, ctx: &LayoutCtx) {
        self.flush(ctx);
        self.widest = self.widest.max(self.line);
        self.line = 0.0;
        self.space = None;
        self.after_space = true;
    }

    /// A block-level child: it closes the line before it and is a line of its
    /// own, `w` wide.
    pub(in crate::layout) fn block(&mut self, w: f32, ctx: &LayoutCtx) {
        self.forced_break(ctx);
        self.widest = self.widest.max(w);
    }

    /// Something of fixed width on the line — an atomic inline, or an inline
    /// box's margin, border and padding on one side. A space before it counts;
    /// a space after it is not collapsed into it.
    pub(in crate::layout) fn atom(&mut self, w: f32, ctx: &LayoutCtx) {
        if self.min {
            self.forced_break(ctx);
            self.widest = self.widest.max(w);
            return;
        }
        self.place_space(ctx);
        self.flush(ctx);
        self.line += w;
        self.after_space = false;
    }

    /// One text node, under its parent element's style.
    pub(in crate::layout) fn text(&mut self, dom: &Dom, id: NodeIdx, font: f32, ctx: &LayoutCtx) {
        let NodeKind::Text(t) = &dom.node(id).kind else { return };
        let parent = dom.node(id).parent;
        let css = parent.and_then(|p| dom.computed_style_idx(p));
        let css = css.as_deref();
        let family = css.and_then(|c| c.font_family.clone());
        let style = RunStyle {
            font,
            mono: family.as_deref().is_some_and(crate::style::is_mono_family),
            family,
            bold: css.and_then(|c| c.bold).unwrap_or(false),
            italic: italico(css, parent.and_then(|p| tag_de(dom, p)), false),
            letter: css.and_then(|c| c.letter_spacing).unwrap_or(0.0),
            word: css.and_then(|c| c.word_spacing).unwrap_or(0.0),
        };
        let regime = css.map(WhiteSpaceRegime::from_css).unwrap_or_else(|| WhiteSpaceRegime::from_css(&Default::default()));
        let ws = css.and_then(|c| c.white_space).unwrap_or(crate::style::WhiteSpace::Normal);
        let soft_breaks = self.min && !matches!(ws, crate::style::WhiteSpace::Nowrap | crate::style::WhiteSpace::Pre);
        // The soft hyphen has no width unless a line breaks at it (`hifen.rs`, rule 1).
        let t = super::hifen::sem_shy(t);
        if regime.preserves() {
            for token in tokens(&t) {
                match token {
                    Token::Word(p) => self.content(&style, p, ctx),
                    Token::Break => self.forced_break(ctx),
                    // `break-spaces`: the opportunity is AFTER the space, which
                    // stays on the line; `pre-wrap`: before it, and the space hangs.
                    Token::Space if soft_breaks && regime.breaks_after_each() => {
                        self.content(&style, " ", ctx);
                        self.forced_break(ctx);
                    }
                    Token::Space | Token::Tab if soft_breaks => self.forced_break(ctx),
                    Token::Space => self.content(&style, " ", ctx),
                    Token::Tab => {
                        self.place_space(ctx);
                        self.flush(ctx);
                        let space_w = measure(ctx, &style, " ");
                        self.line += regime.tab(self.line, space_w).1;
                        self.after_space = false;
                    }
                }
            }
            return;
        }
        // Collapsible: a run of white space is one separator, and under
        // `pre-line` a newline in it is a forced break that also removes the
        // spaces around it (§4.1.1).
        let mut rest: &str = &t;
        while let Some(c) = rest.chars().next() {
            let is_white = |c: char| c == '\t' || c == '\n' || crate::inline_box::e_espaco_css(c);
            let end = if is_white(c) { rest.find(|c| !is_white(c)) } else { rest.find(is_white) }.unwrap_or(rest.len());
            let (chunk, after) = rest.split_at(end);
            rest = after;
            if !is_white(c) {
                self.content(&style, chunk, ctx);
            } else if regime.preserves_newlines() && chunk.contains('\n') {
                for _ in chunk.matches('\n') {
                    self.forced_break(ctx);
                }
            } else if soft_breaks {
                self.forced_break(ctx);
            } else if !self.after_space {
                self.space = Some(style.clone());
                self.after_space = true;
            }
        }
    }

    fn content(&mut self, style: &RunStyle, s: &str, ctx: &LayoutCtx) {
        self.place_space(ctx);
        self.append(style, s, ctx);
        self.after_space = false;
    }

    fn place_space(&mut self, ctx: &LayoutCtx) {
        if let Some(e) = self.space.take() {
            self.append(&e, " ", ctx);
        }
    }

    fn append(&mut self, style: &RunStyle, s: &str, ctx: &LayoutCtx) {
        if self.style.as_ref() != Some(style) {
            self.flush(ctx);
            self.style = Some(style.clone());
        }
        self.buf.push_str(s);
    }

    fn flush(&mut self, ctx: &LayoutCtx) {
        if let Some(e) = &self.style {
            if !self.buf.is_empty() {
                self.line += measure(ctx, e, &self.buf);
                self.buf.clear();
            }
        }
    }
}

/// The width of `s` in `e`: the measurer's advance plus one letter-spacing
/// per character and one word-spacing per space — the same sums `wrap_runs`
/// adds to a line, so a box sized by this width fits the line it lays out.
fn measure(ctx: &LayoutCtx, e: &RunStyle, s: &str) -> f32 {
    ctx.measurer.text_width_family(s, e.font, e.family.as_deref(), e.mono, e.bold, e.italic)
        + crate::style::spacing_width(s.chars().count(), e.letter)
        + s.matches(' ').count() as f32 * e.word
}

/// The width of one text node standing alone — a loose text that is its own
/// flex item or anonymous cell, or the min-content of a table's text.
pub(crate) fn intrinsic_text_width(dom: &Dom, id: NodeIdx, font: f32, min: bool, ctx: &LayoutCtx) -> f32 {
    let mut lines = Lines::new(min);
    lines.text(dom, id, font, ctx);
    lines.finish(ctx)
}

/// `true` for an inline box whose content joins the parent's lines: a
/// non-replaced `display: inline` element inside a FLOW container. Its text is
/// measured run by run inside the parent's `Lines` instead of as one opaque
/// width, which is what lets a forced break inside it split the parent's line.
///
/// The container is asked through the box tree because a flex or grid
/// container blockifies its children: a custom element there keeps
/// `display: inline` in its style and is nonetheless a block with a `width`
/// (`flex_column_wrap_corpus`, three 15px `innerItem`s).
pub(in crate::layout) fn is_open_inline(
    dom: &Dom,
    tree: &crate::boxes::BoxTree,
    (parent, child_box): (crate::boxes::BoxId, crate::boxes::BoxId),
    id: NodeIdx,
    ctx: &LayoutCtx,
) -> bool {
    let NodeKind::Element { tag } = &dom.node(id).kind else { return false };
    if tag == "br" || is_non_rendered_tag(tag) {
        return false;
    }
    let Some(css) = dom.computed_style_idx(id) else { return false };
    let fc = tree.formatting_context(dom, child_box);
    tree.formatting_context(dom, parent).inner == crate::boxes::InnerDisplay::Flow
        && fc.is_inline_level()
        && !fc.is_atomic_inline()
        && float_of(dom, id) == crate::style::FloatSide::None
        && crate::inline_box::replaced_inline_size(dom, id, &css, f32::INFINITY, (None, None), ctx).is_none()
        && super::input::tamanho_natural_controlo(dom, id, &css, ctx).is_none()
}

/// An inline box's margin + border + padding on its start and end sides:
/// they sit on the first and the last of its lines, not on every one.
pub(in crate::layout) fn frame_inline(css: &crate::style::ComputedStyle, resolve: &ResolveCtx) -> (f32, f32) {
    let side = |s: &crate::style::Side| match s {
        crate::style::Side::Len(d) => crate::style::dimensao_absoluta(*d, resolve).unwrap_or(0.0),
        _ => 0.0,
    };
    let [_, br, _, bl] = crate::style::borders::used_widths(css);
    (
        side(&css.margin.left) + bl + side(&css.padding.left),
        side(&css.margin.right) + br + side(&css.padding.right),
    )
}
