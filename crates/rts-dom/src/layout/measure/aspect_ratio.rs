//! The `aspect-ratio` property on a NON-replaced box (CSS Sizing 4 §5): the
//! one transfer every formatting context asks for, in either direction.
//!
//! It lives here, beside the other sizing questions, because two contexts read
//! it: `block.rs` transfers a known WIDTH to the height, and the grid
//! (`grid/aspect.rs`) transfers a height that is definite against the item's
//! grid area to the width. Written inline in `block.rs` it answered one of the
//! two and could not be reached by the other, which is how grid items came to
//! ignore the property entirely.
//!
//! The ratio sizes the box `box-sizing` names (Sizing 4 §5.1, "the
//! box-sizing property determines which box the aspect ratio is applied to"),
//! so both functions take the content size of the known axis and the
//! padding+border of each axis, and answer a CONTENT size again.

use super::*;

/// The preferred aspect ratio (width / height) `css` declares, or `None` for
/// `auto` and for a degenerate ratio, which Sizing 4 §5.1 treats as `auto`.
pub(in crate::layout) fn declared_ratio(css: &ComputedStyle) -> Option<f32> {
    css.aspect_ratio.filter(|r| *r > 0.0 && r.is_finite())
}

/// The content height the ratio gives a box whose content width is
/// `content_w`. `frame_h`/`frame_v` are padding+border of each axis.
pub(in crate::layout) fn height_for_width(
    css: &ComputedStyle,
    content_w: f32,
    frame_h: f32,
    frame_v: f32,
) -> Option<f32> {
    let r = declared_ratio(css)?;
    Some(if css.border_box.unwrap_or(false) {
        ((content_w + frame_h) / r - frame_v).max(0.0)
    } else {
        (content_w / r).max(0.0)
    })
}

/// The content width the ratio gives a box whose content height is
/// `content_h` — the other direction of [`height_for_width`].
pub(in crate::layout) fn width_for_height(
    css: &ComputedStyle,
    content_h: f32,
    frame_h: f32,
    frame_v: f32,
) -> Option<f32> {
    let r = declared_ratio(css)?;
    Some(if css.border_box.unwrap_or(false) {
        ((content_h + frame_v) * r - frame_h).max(0.0)
    } else {
        (content_h * r).max(0.0)
    })
}
