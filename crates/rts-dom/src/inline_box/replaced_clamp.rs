//! The used size of a replaced element once `min-`/`max-` constraints apply,
//! in the ORDER the specifications give (CSS 2.1 §10.4, CSS Sizing 4 §5).
//!
//! It left `substituido.rs` because the order is the whole point and it was
//! wrong there: the four clamps ran one after another, each rescaling the
//! other axis when that axis was `auto`. That is right for exactly one
//! constraint and wrong as soon as two apply — a width derived from a height
//! was first cut by `max-width` and THEN rescaled by `max-height`, so the
//! ratio and both limits could not all hold at once.
//!
//! The spec's order has three shapes, and each is a separate arm below:
//!
//! - both sizes definite: each axis is clamped by its own limits, the ratio
//!   is not consulted (the author fixed both);
//! - one size definite: that size is clamped FIRST in its own axis, then
//!   transferred through the ratio, and the transferred size is clamped in
//!   the other axis — which may break the ratio, as the browser does;
//! - neither: the constraint table of §10.4, which keeps the ratio when it
//!   can and gives it up only when two limits contradict it.

/// The limits of one axis, already resolved: `min` of 0 and `max` of
/// infinity when nothing is declared. `max` below `min` is raised to it, as
/// §10.4 says (`min` wins).
#[derive(Clone, Copy, Debug)]
pub(crate) struct AxisLimits {
    pub(crate) min: f32,
    pub(crate) max: f32,
}

impl AxisLimits {
    pub(crate) fn new(min: Option<f32>, max: Option<f32>) -> Self {
        let min = min.unwrap_or(0.0).max(0.0);
        let max = max.unwrap_or(f32::INFINITY).max(min);
        Self { min, max }
    }
    fn clamp(self, v: f32) -> f32 {
        v.min(self.max).max(self.min)
    }
}

/// The used content size. `w0`/`h0` are the definite sizes (declared or
/// forced), `ratio` is `(width, height)` when the element has one, and
/// `natural` is what the element measures with neither size given (its
/// intrinsic size, or the 300×150 default). With one size given and no
/// ratio the other is 0, which is what this engine answered before the
/// order was fixed; the 300×150 default there is a separate question.
pub(crate) fn clamp_replaced(
    w0: Option<f32>,
    h0: Option<f32>,
    ratio: Option<(f32, f32)>,
    natural: (f32, f32),
    lw: AxisLimits,
    lh: AxisLimits,
) -> (f32, f32) {
    let ratio = ratio.filter(|(rw, rh)| *rw > 0.0 && *rh > 0.0);
    match (w0, h0) {
        (Some(w), Some(h)) => (lw.clamp(w), lh.clamp(h)),
        (Some(w), None) => {
            let w = lw.clamp(w);
            let h = ratio.map_or(0.0, |(rw, rh)| w * rh / rw);
            (w, lh.clamp(h))
        }
        (None, Some(h)) => {
            let h = lh.clamp(h);
            let w = ratio.map_or(0.0, |(rw, rh)| h * rw / rh);
            (lw.clamp(w), h)
        }
        (None, None) => match ratio {
            Some(_) => constraint_table(natural.0, natural.1, lw, lh),
            None => (lw.clamp(natural.0), lh.clamp(natural.1)),
        },
    }
}

/// CSS 2.1 §10.4, the table for a replaced element with an intrinsic ratio
/// and both `width` and `height` computing to `auto`. Written as the table
/// rather than derived, so each row can be checked against the spec text.
fn constraint_table(w: f32, h: f32, lw: AxisLimits, lh: AxisLimits) -> (f32, f32) {
    if w <= 0.0 || h <= 0.0 {
        return (lw.clamp(w), lh.clamp(h));
    }
    let (over_w, under_w) = (w > lw.max, w < lw.min);
    let (over_h, under_h) = (h > lh.max, h < lh.min);
    match (over_w, under_w, over_h, under_h) {
        (true, _, true, _) if lw.max / w <= lh.max / h => (lw.max, lh.min.max(lw.max * h / w)),
        (true, _, true, _) => (lw.min.max(lh.max * w / h), lh.max),
        (_, true, _, true) if lw.min / w <= lh.min / h => (lw.max.min(lh.min * w / h), lh.min),
        (_, true, _, true) => (lw.min, lh.max.min(lw.min * h / w)),
        (_, true, true, _) => (lw.min, lh.max),
        (true, _, _, true) => (lw.max, lh.min),
        (true, _, _, _) => (lw.max, lh.min.max(lw.max * h / w)),
        (_, true, _, _) => (lw.min, lh.max.min(lw.min * h / w)),
        (_, _, true, _) => (lw.min.max(lh.max * w / h), lh.max),
        (_, _, _, true) => (lw.max.min(lh.min * w / h), lh.min),
        _ => (w, h),
    }
}

/// The containing block's height as the basis of a replaced element's
/// `height: %`, or `None` when that height is not DEFINITE.
///
/// `avail_h` alone is not the answer. The layout hands its children a height
/// in more cases than CSS calls definite — a `min-height`, a column flex
/// item's post-flexing size in a container of `auto` height — because
/// stretch and non-replaced `height: %` there were measured to want it. A
/// replaced element transfers that height through its ratio into its WIDTH,
/// so an approximated basis moves the box on both axes: the three reftests a
/// plain `avail_h` lost (`flex-aspect-ratio-img-column-004`, a `min-height`
/// container; `percentage-heights-024`, an auto-height column item;
/// `grid-items/replaced-element-015`) were each that. The basis is kept only
/// when the containing block DECLARES a height, which is the case §10.5
/// names; a flex item stretched by a definite container
/// (`percentage-max-height-002`) is definite by Flexbox §9.8 and left out
/// here, stated rather than approximated.
pub(crate) fn definite_cb_height(
    cb_css: &crate::style::ComputedStyle,
    avail_h: Option<f32>,
) -> Option<f32> {
    use crate::style::Dimension;
    match cb_css.height {
        None | Some(Dimension::Auto | Dimension::MaxContent | Dimension::MinContent) => None,
        Some(_) => avail_h,
    }
}
