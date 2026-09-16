//! The CONTAINING BLOCK as an entity: two extents, each one either DEFINITE or
//! INDEFINITE, and a percentage that says which axis it is on.
//!
//! ## Why this is a type and not another parameter
//!
//! [`ResolveCtx`] carries one extent, `parent_content_w`, and every percentage
//! resolves against it — `width: 50%` (right), `height: 50%` (the containing
//! block's WIDTH, never the right basis), `top: 50%` (same). The engine already
//! answers that three times by hand, each in its own file and each with its own
//! shape:
//!
//! - `layout/posicionado.rs::resolve_height` takes an `avail_h: Option<f32>`
//!   and rebuilds the `calc()` sum on the block axis;
//! - `layout/posicionado.rs::resolve_inset` takes the extent of the axis the
//!   caller happens to know it is on;
//! - `inline_box/substituido.rs` keeps a `base_de_percentagem_definida` flag
//!   beside a `declarado_altura` closure that drops a percentage height
//!   outright, because that function has no height to resolve against.
//!
//! Threading a fourth `avail_h` through more signatures was tried, MEASURED and
//! REVERTED — it cost two reftests and gained one. This is the other shape: the
//! basis travels as an entity that knows its own axes, so the question "which
//! extent is this percentage against, and is there one at all" has one answer
//! in one place rather than an argument per call site.
//!
//! ## The rule this type exists to make unrepresentable
//!
//! **A percentage against an indefinite basis computes to `auto`, never to a
//! number.** [`PercentBasis::new`] is the only way in from an `f32` and it
//! refuses anything non-finite, so an infinite available width — what a
//! max-content measurement passes — cannot become an infinite used width. That
//! exact path produced `w: inf` and a 65-second raster
//! (`intrinsic-percent-replaced-019`, WPT).

use super::dimensao::{Dimension, ResolveCtx};

/// Which axis a length is being resolved ON.
///
/// Named by the writing mode's own vocabulary (CSS Writing Modes 4 §1.2) and
/// not `Horizontal`/`Vertical`, because the mapping between the two is the
/// writing mode's to decide and this engine will eventually let it:
/// [`ContainingBlock::horizontal_tb`] is where that assumption lives, alone,
/// instead of being spread over every caller that says "width".
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Axis {
    /// The axis lines are laid along — `width` in `horizontal-tb`.
    Inline,
    /// The axis lines stack along — `height` in `horizontal-tb`.
    Block,
}

/// The extent of a containing block on ONE axis, as a percentage's basis.
///
/// `Indefinite` is not an error and not a zero: it is the statement that this
/// engine does not know the extent, which CSS answers by computing the
/// percentage to `auto`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum PercentBasis {
    /// A known, FINITE extent in px.
    Definite(f32),
    /// No known extent. A percentage against it computes to `auto`.
    Indefinite,
}

impl PercentBasis {
    /// The only way in from a number, and the reason the variant is not just a
    /// public field: a non-finite extent is INDEFINITE, not a definite
    /// infinity.
    ///
    /// An unbounded available width is what a max-content measurement passes
    /// around, and `100%` of it used to be `inf`. A negative extent is
    /// indefinite for the same reason — it is not an extent.
    pub fn new(px: f32) -> PercentBasis {
        if px.is_finite() && px >= 0.0 {
            PercentBasis::Definite(px)
        } else {
            PercentBasis::Indefinite
        }
    }

    /// From an extent the caller may or may not have. `None` is indefinite, and
    /// so is a `Some` that is not a finite non-negative number.
    pub fn from_option(px: Option<f32>) -> PercentBasis {
        px.map_or(PercentBasis::Indefinite, PercentBasis::new)
    }

    /// The extent in px, or `None` when there is no basis. Always finite when
    /// `Some`, by construction.
    pub fn px(self) -> Option<f32> {
        match self {
            PercentBasis::Definite(v) => Some(v),
            PercentBasis::Indefinite => None,
        }
    }

    /// `true` when a percentage against this basis resolves to a number.
    pub fn is_definite(self) -> bool {
        matches!(self, PercentBasis::Definite(_))
    }
}

/// A containing block: its extent on each of the two axes, and which of them
/// are known.
///
/// It deliberately does NOT carry an origin. `layout/caixa_contentora.rs`
/// answers where the containing block starts — that is a positioning question —
/// and this answers what a percentage inside it resolves against. Fusing them
/// would put an origin on every measurement that has none.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ContainingBlock {
    inline: PercentBasis,
    block: PercentBasis,
}

impl ContainingBlock {
    /// Both axes, named.
    pub fn new(inline: PercentBasis, block: PercentBasis) -> ContainingBlock {
        ContainingBlock { inline, block }
    }

    /// The `horizontal-tb` mapping — width is the inline extent, height the
    /// block extent — and the ONE place in this type that assumes it.
    ///
    /// The height is an `Option` because that is how the layout carries it: a
    /// block of `height: auto` has no block extent to give its children, which
    /// is exactly `Indefinite` and exactly why a `height: 50%` inside it
    /// computes to `auto` (CSS 2.1 §10.5).
    pub fn horizontal_tb(width: f32, height: Option<f32>) -> ContainingBlock {
        ContainingBlock {
            inline: PercentBasis::new(width),
            block: PercentBasis::from_option(height),
        }
    }

    /// The basis a percentage on `axis` resolves against.
    pub fn basis(&self, axis: Axis) -> PercentBasis {
        match axis {
            Axis::Inline => self.inline,
            Axis::Block => self.block,
        }
    }

    /// Resolve a declared dimension ON a named axis, clamped at ≥ 0.
    ///
    /// `ctx` still supplies the font and viewport bases — those are not the
    /// containing block's to know — but its `parent_content_w` is IGNORED for
    /// the percentage, which is the whole point: the basis comes from the axis
    /// that was asked for.
    ///
    /// `None` means "the layout decides": `auto`, an intrinsic keyword, or a
    /// percentage with no basis. Those three are the same answer on purpose —
    /// CSS says a percentage against an indefinite basis IS `auto`, so a caller
    /// with one `auto` path is already correct.
    pub fn resolve(&self, d: Dimension, axis: Axis, ctx: &ResolveCtx) -> Option<f32> {
        self.resolve_signed(d, axis, ctx).map(|px| px.max(0.0))
    }

    /// As [`resolve`](ContainingBlock::resolve) but WITHOUT the ≥ 0 clamp — for
    /// margins and positioning offsets, where a negative value is legal.
    pub fn resolve_signed(&self, d: Dimension, axis: Axis, ctx: &ResolveCtx) -> Option<f32> {
        d.resolve_signed(&self.ctx_on(axis, ctx))
    }

    /// `ctx` with the percentage basis of `axis` substituted in, so the single
    /// implementation in `Dimension::resolve_signed` does the arithmetic.
    ///
    /// Rewriting that arithmetic here would be a second truth about what a
    /// `calc()` sums — the engine has already paid for one of those, in
    /// `resolve_height`, which rebuilds the sum term by term and would silently
    /// disagree the day a unit is added.
    ///
    /// An indefinite basis becomes NaN rather than 0: `Dimension::resolve_signed`
    /// refuses a non-finite basis with `None`, which is the `auto` this needs,
    /// while a 0 would answer `0px` — a number, and the wrong one.
    fn ctx_on(&self, axis: Axis, ctx: &ResolveCtx) -> ResolveCtx {
        let mut out = *ctx;
        out.parent_content_w = self.basis(axis).px().unwrap_or(f32::NAN);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(parent_w: f32) -> ResolveCtx {
        ResolveCtx {
            parent_content_w: parent_w,
            node_font_size: 16.0,
            root_font_size: 16.0,
            viewport_w: 800.0,
            viewport_h: 600.0,
        }
    }

    /// A containing block 200 wide and 50 tall, which is the shape that makes
    /// an axis mistake visible: the two extents differ, so a percentage that
    /// reads the wrong one answers a different number rather than the same one.
    fn cb() -> ContainingBlock {
        ContainingBlock::horizontal_tb(200.0, Some(50.0))
    }

    // ---- the axis ----

    /// A WIDTH percentage does not read the height.
    ///
    /// Pins the behaviour and not the function: whatever resolves `width: 50%`
    /// must answer 100 in a containing block of 200×50, and 25 would mean it
    /// read the block axis.
    #[test]
    fn a_width_percentage_does_not_read_the_height() {
        let got = cb().resolve(Dimension::Percent(50.0), Axis::Inline, &ctx(200.0));
        assert_eq!(got, Some(100.0));
    }

    /// And a HEIGHT percentage does not read the width — which is what the
    /// engine did everywhere, because `ResolveCtx` has one extent and it is the
    /// width. `height: 50%` of a 50-tall containing block is 25, not 100.
    #[test]
    fn a_height_percentage_does_not_read_the_width() {
        let got = cb().resolve(Dimension::Percent(50.0), Axis::Block, &ctx(200.0));
        assert_eq!(got, Some(25.0));
    }

    /// The `ResolveCtx` that comes in does not decide the basis. Here it says
    /// 999 and neither axis answers against it: the containing block is the
    /// authority on its own extents, and the field is only a carrier for the
    /// font and viewport bases.
    #[test]
    fn the_resolve_ctx_width_does_not_leak_into_either_axis() {
        let c = ctx(999.0);
        assert_eq!(cb().resolve(Dimension::Percent(100.0), Axis::Inline, &c), Some(200.0));
        assert_eq!(cb().resolve(Dimension::Percent(100.0), Axis::Block, &c), Some(50.0));
    }

    // ---- the indefinite basis ----

    /// A height percentage with NO basis computes to `auto` — the answer CSS
    /// 2.1 §10.5 gives, and `None` is how "the layout decides" is said here.
    #[test]
    fn a_height_percentage_with_no_basis_computes_to_auto() {
        let cb = ContainingBlock::horizontal_tb(200.0, None);
        assert_eq!(cb.resolve(Dimension::Percent(50.0), Axis::Block, &ctx(200.0)), None);
        // …while the inline axis of the SAME containing block still answers.
        assert_eq!(
            cb.resolve(Dimension::Percent(50.0), Axis::Inline, &ctx(200.0)),
            Some(100.0)
        );
    }

    /// An infinite extent is not a definite extent. This is the constructor's
    /// whole job: `100%` of an unbounded available width was `inf`, a border
    /// box came out with `w: inf`, and the rasteriser walked it for 65 seconds.
    #[test]
    fn an_infinite_extent_is_indefinite_and_never_an_infinite_answer() {
        let cb = ContainingBlock::horizontal_tb(f32::INFINITY, None);
        assert_eq!(cb.basis(Axis::Inline), PercentBasis::Indefinite);
        let got = cb.resolve(Dimension::Percent(100.0), Axis::Inline, &ctx(f32::INFINITY));
        assert_eq!(got, None, "a percentage without a basis computes to auto");
    }

    /// NaN is refused for the same reason, and it is the sharper case: an
    /// infinity is at least visible downstream, while a NaN loses every
    /// `min`/`max` it touches without saying anything.
    #[test]
    fn a_nan_extent_is_indefinite() {
        assert_eq!(PercentBasis::new(f32::NAN), PercentBasis::Indefinite);
        assert_eq!(PercentBasis::new(f32::NEG_INFINITY), PercentBasis::Indefinite);
    }

    /// A negative extent is not an extent either. It reaches here from a
    /// subtraction that over-subtracted — a frame wider than the box it is
    /// inside — and `-30 * 50%` would answer a negative used size that the
    /// caller's `max(0.0)` then turns into a silent zero.
    #[test]
    fn a_negative_extent_is_indefinite_and_not_a_negative_answer() {
        assert_eq!(PercentBasis::new(-30.0), PercentBasis::Indefinite);
        assert!(PercentBasis::new(0.0).is_definite(), "zero IS an extent");
    }

    // ---- what an indefinite basis does NOT do ----

    /// An absolute length does not care that the basis is missing. This is the
    /// half that a "drop every percentage" shortcut would get wrong: only the
    /// percentage depends on the containing block.
    #[test]
    fn an_absolute_length_resolves_with_no_basis_at_all() {
        let cb = ContainingBlock::new(PercentBasis::Indefinite, PercentBasis::Indefinite);
        assert_eq!(cb.resolve(Dimension::Px(42.0), Axis::Block, &ctx(f32::INFINITY)), Some(42.0));
        assert_eq!(cb.resolve(Dimension::Em(2.0), Axis::Block, &ctx(f32::INFINITY)), Some(32.0));
        assert_eq!(cb.resolve(Dimension::Rem(1.5), Axis::Inline, &ctx(f32::INFINITY)), Some(24.0));
    }

    /// `vw`/`vh` are against the VIEWPORT and not against the containing block,
    /// so they answer with no basis on either axis — and they answer the same
    /// on both axes, which is the point of their being separate units.
    #[test]
    fn viewport_units_ignore_the_containing_block_entirely() {
        let cb = ContainingBlock::new(PercentBasis::Indefinite, PercentBasis::Indefinite);
        assert_eq!(cb.resolve(Dimension::Vw(50.0), Axis::Block, &ctx(f32::NAN)), Some(400.0));
        assert_eq!(cb.resolve(Dimension::Vh(50.0), Axis::Inline, &ctx(f32::NAN)), Some(300.0));
    }

    // ---- calc(), which is the one that had TWO defects ----

    /// A `calc()` with a percentage term resolves it on the axis asked for.
    /// `calc(100% - 10px)` is 190 on the inline axis of a 200-wide containing
    /// block and 40 on its 50-tall block axis — the generic resolution answered
    /// 190 for both, which is the defect `resolve_height` was written to work
    /// around in one file.
    #[test]
    fn a_calc_percentage_term_follows_the_axis() {
        let c = super::super::CalcLen { px: -10.0, pct: 100.0, ..Default::default() };
        assert_eq!(cb().resolve(Dimension::Calc(c), Axis::Inline, &ctx(200.0)), Some(190.0));
        assert_eq!(cb().resolve(Dimension::Calc(c), Axis::Block, &ctx(200.0)), Some(40.0));
    }

    /// And with no basis it computes to `auto`, like a bare percentage.
    #[test]
    fn a_calc_percentage_term_with_no_basis_computes_to_auto() {
        let cb = ContainingBlock::horizontal_tb(200.0, None);
        let c = super::super::CalcLen { px: -10.0, pct: 100.0, ..Default::default() };
        assert_eq!(cb.resolve(Dimension::Calc(c), Axis::Block, &ctx(200.0)), None);
    }

    /// The silent half: a `calc()` with NO percentage in it must still resolve
    /// when the basis is indefinite.
    ///
    /// It did not. `f32::INFINITY * 0.0` is NaN, so `calc(1rem + 10px)` — which
    /// names no containing block at all — came out NaN during any max-content
    /// measurement, and a NaN survives every `min`/`max` downstream without a
    /// symptom of its own.
    #[test]
    fn a_calc_without_a_percentage_term_resolves_with_no_basis() {
        let cb = ContainingBlock::new(PercentBasis::Indefinite, PercentBasis::Indefinite);
        let c = super::super::CalcLen { px: 10.0, rem: 1.0, ..Default::default() };
        assert_eq!(cb.resolve(Dimension::Calc(c), Axis::Inline, &ctx(f32::INFINITY)), Some(26.0));
    }

    // ---- the guard, read through the plain resolution ----

    /// The rule holds for callers that have NOT been converted yet, which is
    /// most of them: `Dimension::resolve` itself refuses a non-finite basis.
    ///
    /// This is the test that fails without the change — it answered
    /// `Some(inf)`.
    #[test]
    fn the_plain_resolution_also_refuses_an_infinite_basis() {
        assert_eq!(Dimension::Percent(100.0).resolve(&ctx(f32::INFINITY)), None);
        assert_eq!(Dimension::Percent(50.0).resolve_signed(&ctx(f32::NAN)), None);
        // …and a finite basis is untouched.
        assert_eq!(Dimension::Percent(50.0).resolve(&ctx(200.0)), Some(100.0));
    }

    /// Signed resolution keeps the sign — a `margin-left: -50%` is legal — and
    /// the clamp is only on the unsigned path.
    #[test]
    fn a_negative_percentage_keeps_its_sign_only_on_the_signed_path() {
        let c = ctx(200.0);
        assert_eq!(
            cb().resolve_signed(Dimension::Percent(-50.0), Axis::Inline, &c),
            Some(-100.0)
        );
        assert_eq!(cb().resolve(Dimension::Percent(-50.0), Axis::Inline, &c), Some(0.0));
    }

    /// `auto` and the intrinsic keywords are the layout's to decide on either
    /// axis, basis or no basis — unchanged, and stated so that a future guard
    /// on the percentage does not quietly start answering for them.
    #[test]
    fn auto_and_the_intrinsic_keywords_stay_the_layouts_to_decide() {
        let c = ctx(200.0);
        assert_eq!(cb().resolve(Dimension::Auto, Axis::Inline, &c), None);
        assert_eq!(cb().resolve(Dimension::MaxContent, Axis::Inline, &c), None);
        assert_eq!(cb().resolve(Dimension::MinContent, Axis::Block, &c), None);
    }
}
