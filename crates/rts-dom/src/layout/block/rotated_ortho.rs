//! Orthogonal flow handling and display list rotation-out for rotated frames.
//!
//! Separated from `rotated.rs` to maintain module size limits.

use std::cell::RefCell;
use std::rc::Rc;

use super::rotated::{push_frame_root, push_in_frame, set_frame_root, set_in_frame};
use super::*;
use crate::paint::transform::Mat2d;

pub(super) struct OrthoChild {
    pub(super) box_id: crate::boxes::BoxId,
    pub(super) frame_x: f32,
    pub(super) frame_y: f32,
    pub(super) frame_w: f32,
    pub(super) frame_h: f32,
    pub(super) list: DisplayList,
}

thread_local! {
    /// Stack of orthogonal children lists per active rotated frame.
    static ORTHO_STACK: RefCell<Vec<Vec<OrthoChild>>> = RefCell::new(Vec::new());
}

pub(super) fn push_ortho_stack() {
    ORTHO_STACK.with(|s| s.borrow_mut().push(Vec::new()));
}

pub(super) fn pop_ortho_stack() -> Vec<OrthoChild> {
    ORTHO_STACK.with(|s| s.borrow_mut().pop().unwrap_or_default())
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct OrthoMark {
    frame_depth: usize,
    child_count: usize,
}

pub(crate) fn mark() -> OrthoMark {
    ORTHO_STACK.with(|s| {
        let s = s.borrow();
        OrthoMark {
            frame_depth: s.len(),
            child_count: s.last().map(|v| v.len()).unwrap_or(0),
        }
    })
}

pub(crate) fn restore(mark: OrthoMark) {
    ORTHO_STACK.with(|s| {
        let mut s = s.borrow_mut();
        s.truncate(mark.frame_depth);
        if let Some(stack) = s.last_mut() {
            stack.truncate(mark.child_count);
        }
    });
}

#[allow(clippy::too_many_arguments)]
pub(super) fn layout_orthogonal(
    dom: &Dom,
    id: NodeIdx,
    box_id: crate::boxes::BoxId,
    _css: &ComputedStyle,
    x: f32,
    y: f32,
    _avail_w: f32,
    avail_h: Option<f32>,
    forced_outer_w: Option<f32>,
    forced_outer_h: Option<f32>,
    ctx: &LayoutCtx,
    list: &mut DisplayList,
) -> Option<(f32, f32)> {
    // In the parent vertical frame:
    // - frame block axis is y (physical X). avail_h is containing block block-size.
    // - frame inline axis is x (physical Y).
    // An orthogonal horizontal child has its inline axis on physical X.
    // CSS Writing Modes 4 §7.3.1:
    // available inline size is containing block block size if definite, else ICB width.
    let child_avail_w = forced_outer_h.or(avail_h).unwrap_or(ctx.viewport_w);
    let (child_avail_h, child_forced_w, child_forced_h) =
        (forced_outer_w, forced_outer_h, forced_outer_w);

    let prev_frame = push_in_frame(None);
    let prev_root = push_frame_root(None);

    let mut child_list = DisplayList::for_dom(dom);
    let (child_phys_w, child_phys_h) = layout_block(
        dom,
        id,
        box_id,
        0.0,
        0.0,
        child_avail_w,
        child_avail_h,
        child_forced_w,
        child_forced_h,
        false,
        true,                           // shrink-to-fit (Writing Modes 4 §7.3)
        &BlockFormattingContext::new(), // independent BFC (§7.3)
        ctx,
        &mut child_list,
    );

    set_in_frame(prev_frame);
    set_frame_root(prev_root);

    let (frame_w, frame_h) = (child_phys_h, child_phys_w);

    list.pieces.push(Piece::Rect(box_id));
    ORTHO_STACK.with(|s| {
        if let Some(stack) = s.borrow_mut().last_mut() {
            stack.push(OrthoChild {
                box_id,
                frame_x: x,
                frame_y: y,
                frame_w,
                frame_h,
                list: child_list,
            });
        }
    });

    Some((frame_w, frame_h))
}

/// Maps everything `own` recorded through `frame` and appends it to `list`.
pub(super) fn rotate_out(
    mut own: DisplayList,
    box_id: crate::boxes::BoxId,
    frame: &Mat2d,
    mut ortho_children: Vec<OrthoChild>,
    list: &mut DisplayList,
) {
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

    for child in ortho_children.iter_mut() {
        let phys_rect = frame.transform_rect_bbox(Rect::new(
            child.frame_x,
            child.frame_y,
            child.frame_w,
            child.frame_h,
        ));
        child.list.materialize();
        child.list.translate(phys_rect.x, phys_rect.y);
    }

    let merge = |child: OrthoChild, own: &mut DisplayList| {
        own.box_rects.extend(child.list.box_rects);
        own.static_anchors.extend(child.list.static_anchors);
        own.scroll_regions.extend(child.list.scroll_regions);
        own.grid_column_tracks.extend(child.list.grid_column_tracks);
        child.list.pieces
    };
    let orig_pieces = std::mem::take(&mut own.pieces);
    let mut final_pieces = Vec::with_capacity(orig_pieces.len() + 16);
    for piece in orig_pieces {
        if let Piece::Rect(bid) = piece {
            if let Some(pos) = ortho_children.iter().position(|c| c.box_id == bid) {
                final_pieces.extend(merge(ortho_children.remove(pos), &mut own));
                continue;
            }
        }
        final_pieces.push(piece);
    }
    for c in ortho_children {
        final_pieces.extend(merge(c, &mut own));
    }
    own.pieces = final_pieces;

    list.pieces.extend(own.pieces);
    list.box_rects.extend(own.box_rects);
    list.static_anchors.extend(own.static_anchors);
    list.grid_column_tracks.extend(own.grid_column_tracks);
    list.scroll_regions.extend(own.scroll_regions);
}
