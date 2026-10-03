//! A vertical `writing-mode` laid out in a ROTATED FRAME: rotate in, lay out
//! with the horizontal machinery, rotate out (plan
//! `docs/superpowers/plans/2026-09-26-writing-mode.md` F1, lot WM-1).
//!
//! # Why a frame, and not a logical rewrite of the block flow
//!
//! `block.rs` and `vertical_flow.rs` read `avail_w`, `content_w`, `margin.left`
//! and the rest at about a hundred physical call sites. Threading an inline
//! size through all of them is the rewrite the plan rules out; what it does
//! instead is make the subtree LOOK horizontal to that code:
//!
//! 1. **Rotate in.** Every element of the subtree gets the frame's view of its
//!    style (`style::logical::rotate_into_frame`: sides permuted, `width` ↔
//!    `height`, `writing-mode: horizontal-tb`), installed in the Dom's
//!    computed-style memo for the length of the layout and restored after —
//!    so each of those call sites reads logical values without changing.
//! 2. **Lay out** with the unchanged `layout_block`, into a list of its own,
//!    whose `avail_w` is the box's INLINE size.
//! 3. **Rotate out.** One matrix of the frame maps every rect that list
//!    recorded to the page: `vertical-rl` `px = right − (y + h)`, `py = top +
//!    x`; `vertical-lr` `px = left + y`, `py = top + x`. (`sideways-rl` is
//!    `vertical-rl`'s frame, and `sideways-lr` runs its lines up:
//!    `px = left + y`, `py = bottom − (x + w)`.) The box rects go
//!    through `transform_box_rects`, the walk CSS `transform` already uses;
//!    the items through `paint::item::rotate_item`, beside `translate_item`.
//!
//! # When a frame is entered, and what that costs elsewhere
//!
//! Only at a BOUNDARY: a block container whose used writing mode is
//! vertical (any of the four) while its parent's is horizontal — or the root
//! element, whose used mode is the `<body>`'s when there is one (CSS Writing
//! Modes 4 §8, the HTML propagation). A page with no vertical box pays one
//! field test per block and nothing else. Inside a frame every style reads
//! `horizontal-tb`, so no second frame opens.
//!
//! Inside the frame the fragment cache, the measure cache and the intrinsic
//! width cache are neither read nor written (`in_rotated_frame`): all three
//! are keyed by PHYSICAL constraints and would otherwise serve a frame answer
//! outside it, or the reverse. The boundary box itself is cached normally —
//! its output is already physical.
//!
//! # What this lot does not do, stated
//!
//! - A descendant whose writing mode differs from the frame's (an orthogonal
//!   flow, Writing Modes §7.3) is laid out AS IF it had the frame's mode: its
//!   style is rotated with the rest of the subtree. That is WM-2.
//! - `position: absolute/fixed` boxes are laid out by the document's
//!   out-of-flow pass, outside any frame, as before (WM-3).
//! - Flex, grid and table containers never OPEN a frame (flex keeps its own
//!   axis swap, `flex/axes.rs`); inside one opened by an ancestor they run in
//!   the frame like any box (F3, WM-4).
//! - `text-orientation` is out: text is always sideways, never upright —
//!   which is what `sideways-rl`/`-lr` ask for, and what `vertical-*` does
//!   to Latin; CJK set upright is not done.
//! - Pseudo-element boxes (`::before`/`::after` laid out as blocks) do not
//!   open a frame of their own: they have no node to install a style on.
//! - Line baselines recorded inside the frame are dropped: a vertical box
//!   exposes no line baseline to a horizontal parent.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use super::*;
use crate::paint::transform::Mat2d;
use crate::style::values::AxisMap;
use crate::style::{Dimension, WritingMode};

thread_local! {
    /// The mode of the frame being laid out: `None` outside one.
    static IN_FRAME: Cell<Option<WritingMode>> = const { Cell::new(None) };
    /// The box that opened the frame being laid out.
    static FRAME_ROOT: Cell<Option<NodeIdx>> = const { Cell::new(None) };
    /// The frame's view of a style, by the address of the physical one and the
    /// frame's mode. The physical `Rc` is kept beside it so the address cannot
    /// be reused by another style while the entry lives.
    static ROTATED: RefCell<crate::fasthash::FastMap<(usize, u8), (Rc<ComputedStyle>, Rc<ComputedStyle>)>> =
        RefCell::new(Default::default());
}

/// `true` while a rotated frame is being laid out.
pub(crate) fn in_rotated_frame() -> bool {
    IN_FRAME.with(Cell::get).is_some()
}

/// `true` for the box that opened the frame being laid out. A box whose
/// writing mode differs from its parent's establishes an independent block
/// formatting context (Writing Modes 4 §3.1): its children's margins do not
/// collapse through it. Inside the frame its style reads `horizontal-tb`
/// like its parent's, so the style alone cannot say so.
pub(crate) fn is_frame_root(id: NodeIdx) -> bool {
    FRAME_ROOT.with(Cell::get) == Some(id)
}

/// The writing mode of the frame being laid out, `None` outside one — for
/// the one emitter that places something PHYSICALLY inside a frame, a
/// background image, whose tiles keep the image's own orientation
/// (`paint/background_image.rs`).
pub(crate) fn rotated_frame_mode() -> Option<WritingMode> {
    IN_FRAME.with(Cell::get)
}

fn is_vertical(wm: WritingMode) -> bool {
    !wm.is_horizontal()
}

/// Tags that are replaced or form controls: a vertical mode does not change
/// their box in this lot, and none of them has a flow to rotate.
fn opens_no_frame(tag: &str) -> bool {
    matches!(tag, "img" | "canvas" | "svg" | "input" | "textarea" | "select" | "button" | "video" | "iframe")
}

/// The writing mode of the frame `id` opens, or `None` when it opens none.
fn frame_mode(dom: &Dom, id: NodeIdx, css: &ComputedStyle, tag: &str) -> Option<WritingMode> {
    let own = css.writing_mode.unwrap_or_default();
    let is_root = dom.node(id).parent == Some(dom.root);
    if !is_root && !is_vertical(own) {
        return None;
    }
    if in_rotated_frame() || opens_no_frame(tag) {
        return None;
    }
    use crate::style::DisplayKind as D;
    if matches!(
        css.effective_display(),
        Some(D::Flex | D::FlexWrap | D::InlineFlex | D::InlineFlexWrap | D::Grid | D::InlineGrid)
            | Some(D::Table | D::InlineTable | D::TableRowGroup | D::TableHeaderGroup | D::TableFooterGroup)
            | Some(D::TableRow | D::TableCell | D::TableCaption | D::Inline)
    ) {
        return None;
    }
    if is_root {
        // The root's used writing mode is its `<body>`'s when it has one.
        let body = dom.node(id).children.iter().copied().find(|&c| {
            matches!(&dom.node(c).kind, NodeKind::Element { tag } if tag == "body")
        });
        let wm = body.and_then(|b| dom.computed_style_idx(b)).map_or(own, |b| b.writing_mode.unwrap_or_default());
        return is_vertical(wm).then_some(wm);
    }
    // A vertical parent outside a frame is a box the out-of-flow pass lays
    // out on its own: it stays as it was (WM-3).
    let parent = dom.node(id).parent.and_then(|p| dom.computed_style_idx(p));
    if parent.is_some_and(|p| is_vertical(p.writing_mode.unwrap_or_default())) {
        return None;
    }
    Some(own)
}

/// `true` when laying `id` out opens a frame. Asked by the intrinsic width
/// measure, which otherwise measures the box's text along the page's width.
pub(in crate::layout) fn opens_frame(dom: &Dom, id: NodeIdx, css: &ComputedStyle) -> bool {
    let NodeKind::Element { tag } = &dom.node(id).kind else { return false };
    (css.writing_mode.is_some_and(|wm| !wm.is_horizontal()) || dom.node(id).parent == Some(dom.root))
        && frame_mode(dom, id, css, tag).is_some()
}

/// Installs the frame's view of every element style under `id` and answers
/// what it replaced, for [`restore`].
///
/// The boundary box's own percentage margins and paddings resolve against
/// its containing block's INLINE size, which is outside the frame — the
/// horizontal parent's width, `pct_basis` — where inside the frame they would
/// resolve against the frame's inline size. They are resolved to pixels
/// here, on that one style.
fn install(dom: &Dom, id: NodeIdx, frame: AxisMap, pct_basis: f32) -> Vec<(NodeIdx, Option<Rc<ComputedStyle>>)> {
    let mode = frame.writing_mode();
    let mut saved = Vec::new();
    let mut stack = vec![id];
    while let Some(n) = stack.pop() {
        if let Some(phys) = dom.computed_style_idx(n) {
            let key = (Rc::as_ptr(&phys) as usize, mode as u8);
            let rotated = ROTATED.with(|m| {
                let mut m = m.borrow_mut();
                if m.len() > 4096 {
                    m.clear();
                }
                let entry = m.entry(key).or_insert_with(|| {
                    let r = crate::style::logical::rotate_into_frame(&phys, frame);
                    (Rc::clone(&phys), Rc::new(r))
                });
                Rc::clone(&entry.1)
            });
            let rotated = if n == id { resolve_own_percentages(&rotated, pct_basis) } else { rotated };
            saved.push((n, dom.swap_computed_memo(n, Some(rotated))));
        }
        stack.extend(dom.node(n).children.iter().copied());
    }
    saved
}

fn resolve_own_percentages(css: &Rc<ComputedStyle>, basis: f32) -> Rc<ComputedStyle> {
    use crate::style::values::{Edges, Side};
    let px = |s: Side| match s {
        Side::Len(Dimension::Percent(p)) => Side::Len(Dimension::Px(basis * p / 100.0)),
        other => other,
    };
    let edges = |e: Edges| Edges { top: px(e.top), right: px(e.right), bottom: px(e.bottom), left: px(e.left) };
    let (margin, padding) = (edges(css.margin), edges(css.padding));
    if margin == css.margin && padding == css.padding {
        return Rc::clone(css);
    }
    let mut out = (**css).clone();
    (out.margin, out.padding) = (margin, padding);
    Rc::new(out)
}

fn restore(dom: &Dom, saved: Vec<(NodeIdx, Option<Rc<ComputedStyle>>)>) {
    for (n, prev) in saved.into_iter().rev() {
        dom.swap_computed_memo(n, prev);
    }
}

/// Lays `id` out in a rotated frame when it is a writing-mode boundary, and
/// answers its OUTER size on the page; `None` — the caller lays it out as
/// always — for every other box.
#[allow(clippy::too_many_arguments)]
pub(in crate::layout) fn layout_if_boundary(
    dom: &Dom,
    id: NodeIdx,
    box_id: crate::boxes::BoxId,
    css: &ComputedStyle,
    tag: &str,
    x: f32,
    y: f32,
    avail_w: f32,
    avail_h: Option<f32>,
    forced_outer_w: Option<f32>,
    forced_outer_h: Option<f32>,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) -> Option<(f32, f32)> {
    let wm = frame_mode(dom, id, css, tag)?;
    let is_root = dom.node(id).parent == Some(dom.root);
    // The frame's constraints. Its available inline size is the physical
    // height available (the ICB's at the root), and below the root the inline
    // size shrinks to fit inside it (Writing Modes §7.3, fit-content). Its
    // block size — the physical width — is the content's, as a height is:
    // a vertical box in a horizontal parent does NOT stretch across it (Blink:
    // `replaced-content-image-004`, `wm-propagation-body-044-ref` put the
    // column at the parent's left edge). What a flex parent imposes still
    // wins, on the axis it was imposed on.
    let (frame_w, frame_h, forced_w, forced_h, shrink) = if is_root {
        (ctx.viewport_h, Some(avail_w), None, None, false)
    } else {
        (avail_h.unwrap_or(ctx.viewport_h), Some(avail_w), forced_outer_h, forced_outer_w, true)
    };
    let saved = install(dom, id, AxisMap::new(wm, css.direction.unwrap_or_default()), avail_w);
    IN_FRAME.with(|f| f.set(Some(wm)));
    FRAME_ROOT.with(|f| f.set(Some(id)));
    let lines = crate::layout::inline::line_baseline::mark();
    let mut own = DisplayList::for_dom(dom);
    let (fw, fh) = layout_block(
        dom,
        id,
        box_id,
        0.0,
        0.0,
        frame_w,
        frame_h,
        forced_w,
        forced_h,
        false,
        shrink,
        &BlockFormattingContext::new(),
        ctx,
        &mut own,
    );
    crate::layout::inline::line_baseline::discard(lines);
    IN_FRAME.with(|f| f.set(None));
    FRAME_ROOT.with(|f| f.set(None));
    restore(dom, saved);

    let (outer_w, outer_h) = (fh, fw);
    // At the root the block axis starts at the ICB's edge; elsewhere at the
    // box's own, where the horizontal parent put it.
    let right = x + if is_root { avail_w } else { outer_w };
    let frame = match wm {
        WritingMode::VerticalLr => Mat2d { a: 0.0, b: 1.0, c: 1.0, d: 0.0, e: x, f: y },
        // Lines run UP the page from its bottom edge, blocks stack rightwards.
        WritingMode::SidewaysLr => Mat2d { a: 0.0, b: -1.0, c: 1.0, d: 0.0, e: x, f: y + outer_h },
        _ => Mat2d { a: 0.0, b: 1.0, c: -1.0, d: 0.0, e: right, f: y },
    };
    rotate_out(own, box_id, &frame, list);
    Some((outer_w, outer_h))
}

/// Maps everything `own` recorded through `frame` and appends it to `list`.
fn rotate_out(mut own: DisplayList, box_id: crate::boxes::BoxId, frame: &Mat2d, list: &mut DisplayList) {
    // No subtree is reused inside a frame, so this is a no-op but for a
    // producer this lot does not know of; flattening keeps its items right.
    own.materialize();
    let tree = Rc::clone(&own.tree);
    crate::layout::fragment::transform_rects::transform_box_rects(&tree, box_id, frame, &mut own);
    for piece in own.pieces.iter_mut() {
        if let Piece::Item(item) = piece {
            crate::paint::item::rotate_item(item, frame);
        }
    }
    for anchor in own.static_anchors.iter_mut() {
        (anchor.1, anchor.2) = frame.apply(anchor.1, anchor.2);
    }
    for region in own.scroll_regions.iter_mut() {
        region.visible = frame.transform_rect_bbox(region.visible);
        (region.content_w, region.content_h) = (region.content_h, region.content_w);
        (region.overflow_x, region.overflow_y) = (region.overflow_y, region.overflow_x);
    }
    let DisplayList { pieces, box_rects, static_anchors, grid_column_tracks, scroll_regions, .. } = own;
    list.pieces.extend(pieces);
    list.box_rects.extend(box_rects);
    list.static_anchors.extend(static_anchors);
    list.grid_column_tracks.extend(grid_column_tracks);
    list.scroll_regions.extend(scroll_regions);
}
