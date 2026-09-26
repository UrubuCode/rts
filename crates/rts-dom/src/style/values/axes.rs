//! The map between the LOGICAL axes and the PHYSICAL ones: given a writing
//! mode and a direction, which physical axis is the inline axis, which is the
//! block axis, and which side each of them STARTS at.
//!
//! ## Why this is one function and not a rule per call site
//!
//! This engine lays out physically. Every property that can invert therefore
//! needs its own hand-written mirror, and the ones written so far do not share
//! a question: `coluna.rs::mirror_justify`, `grid.rs::cell_align_offset` and
//! two closures in `posicao_estatica.rs` each answer "which end of the axis is
//! the start" on their own, and three of the four do it with a `_ =>` arm that
//! swallows a value the day one is added (`PLAN.md` §11, LOG).
//!
//! In a box tree each box carries its writing mode and the conversion happens
//! in ONE place. This module is that place — the piece the box will carry —
//! and [`ContainingBlock`](super::ContainingBlock) is its first consumer.
//!
//! ## What it is NOT: a second truth about direction
//!
//! The SENSE of each physical axis is already answered once, by
//! [`eixo_x_forward`] and [`eixo_y_forward`] in `style::text`, and the comment
//! on those functions records what a second copy already cost: the first
//! version of `layout::eixos_flex` kept its own and the `gap-*-lr`/`-rl`
//! reftests regressed, because that copy considered `direction` and forgot
//! `writing-mode`. So everything here COMPOSES those two and decides nothing
//! about sense itself.
//!
//! What is genuinely new is the other half of the question, which no single
//! place answered: which physical axis IS the inline one. Two files rebuild it
//! today — `style::logical::to_physical` from `is_horizontal()`, and
//! `layout::eixos_flex::main_no_eixo_y` as a XOR against the `column` keyword
//! — and they are the two sites this type exists to converge.
//!
//! ## Why it lives in `style/values/` and not in `layout/`
//!
//! Its consumers sit on both sides of the style/layout line: `style::logical`
//! resolves `margin-inline-start` during the cascade, long before any layout
//! runs, while `layout::eixos_flex` asks the same thing mid-layout. A module
//! under `layout/` could not be reached from the first without the style layer
//! depending on the layout layer, which is the direction this crate does not
//! allow. It sits beside `containing_block.rs` because the containing block is
//! where the physical extents enter the logical world.
//!
//! ## Scope, stated plainly
//!
//! Nothing here MAKES the engine lay out vertically. It answers the four
//! combinations correctly; whether a caller asks is that caller's lot. The
//! block flow (`layout/block/vertical_flow.rs`, `bloco.rs`) and the text inside a box
//! still treat everything as `horizontal-tb`, exactly as before.

use crate::style::borders::SideName;
use crate::style::text::{eixo_x_forward, eixo_y_forward};
use crate::style::{Direction, WritingMode};

use super::containing_block::Axis;

/// One of the two PHYSICAL axes of the page — the ones a rectangle is written
/// in. Deliberately not merged with [`Axis`], which names the logical pair:
/// the whole point of this module is that the mapping between the two is a
/// question with an answer, and a single enum would make it unaskable.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PhysicalAxis {
    /// Left↔right. `width` is measured on it.
    X,
    /// Top↔bottom. `height` is measured on it.
    Y,
}

impl PhysicalAxis {
    /// The other one.
    pub fn other(self) -> PhysicalAxis {
        match self {
            PhysicalAxis::X => PhysicalAxis::Y,
            PhysicalAxis::Y => PhysicalAxis::X,
        }
    }
}

/// The writing mode and direction of a box, as the question "which physical
/// axis is which logical one, and where does each start".
///
/// It is `Copy` and two enum values wide on purpose: a box carries one, and
/// carrying the pair rather than a precomputed table keeps the answer derived
/// from the style rather than cached beside it — the same rule the box tree
/// states for every derived value (`box-tree.md` §10).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct AxisMap {
    wm: WritingMode,
    dir: Direction,
}

impl AxisMap {
    /// From the two properties, which is how a box will build one: both are
    /// inherited and both are already on `ComputedStyle`.
    pub fn new(wm: WritingMode, dir: Direction) -> AxisMap {
        AxisMap { wm, dir }
    }

    /// `horizontal-tb` + `ltr` — the initial values, and what every caller of
    /// this engine assumed before this module existed. It is a NAME for that
    /// assumption rather than a change to it: a site that still cannot ask
    /// says so by constructing this, and is greppable.
    pub fn horizontal_tb() -> AxisMap {
        AxisMap { wm: WritingMode::HorizontalTb, dir: Direction::Ltr }
    }

    /// The writing mode this map was built from.
    pub fn writing_mode(self) -> WritingMode {
        self.wm
    }

    /// The direction this map was built from.
    pub fn direction(self) -> Direction {
        self.dir
    }

    /// `true` when the inline axis is the horizontal one — i.e. when `width`
    /// is the inline size. The common case, and the one every unconverted
    /// site assumes.
    pub fn inline_is_horizontal(self) -> bool {
        self.wm.is_horizontal()
    }

    /// Which PHYSICAL axis carries a logical one.
    ///
    /// `horizontal-tb`: inline is X, block is Y. Any vertical mode: the two
    /// are SWAPPED (CSS Writing Modes 4 §1.2). `direction` has no part in
    /// this — it decides a sense, never an axis.
    pub fn physical(self, axis: Axis) -> PhysicalAxis {
        let inline = if self.wm.is_horizontal() { PhysicalAxis::X } else { PhysicalAxis::Y };
        match axis {
            Axis::Inline => inline,
            Axis::Block => inline.other(),
        }
    }

    /// The inverse: which LOGICAL axis a physical one is.
    ///
    /// Kept beside `physical` rather than left to the caller because a caller
    /// that holds a width and needs to know what it is would otherwise invert
    /// the mapping by hand, which is the per-site mirror this module exists to
    /// remove.
    pub fn logical(self, axis: PhysicalAxis) -> Axis {
        if axis == self.physical(Axis::Inline) { Axis::Inline } else { Axis::Block }
    }

    /// `true` when a logical axis runs in the POSITIVE sense of the physical
    /// axis it lands on — left→right for X, top→bottom for Y.
    ///
    /// This is [`eixo_x_forward`]/[`eixo_y_forward`] and nothing else: the
    /// sense of a physical axis is their answer, and this only picks which of
    /// the two to ask.
    pub fn forward(self, axis: Axis) -> bool {
        match self.physical(axis) {
            PhysicalAxis::X => eixo_x_forward(self.wm, self.dir),
            PhysicalAxis::Y => eixo_y_forward(self.wm, self.dir),
        }
    }

    /// The physical side a logical axis STARTS at — where `inline-start` and
    /// `block-start` point.
    pub fn start(self, axis: Axis) -> SideName {
        match (self.physical(axis), self.forward(axis)) {
            (PhysicalAxis::X, true) => SideName::Left,
            (PhysicalAxis::X, false) => SideName::Right,
            (PhysicalAxis::Y, true) => SideName::Top,
            (PhysicalAxis::Y, false) => SideName::Bottom,
        }
    }

    /// The physical side a logical axis ENDS at — the opposite of
    /// [`start`](AxisMap::start), and spelled out rather than derived by a
    /// `match` at each call site.
    pub fn end(self, axis: Axis) -> SideName {
        match self.start(axis) {
            SideName::Left => SideName::Right,
            SideName::Right => SideName::Left,
            SideName::Top => SideName::Bottom,
            SideName::Bottom => SideName::Top,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::containing_block::{ContainingBlock, PercentBasis};
    use super::super::dimensao::{Dimension, ResolveCtx};

    fn vertical_rl() -> AxisMap {
        AxisMap::new(WritingMode::VerticalRl, Direction::Ltr)
    }

    // ---- horizontal-tb + ltr: the case that must not move ----

    /// The initial values answer exactly what every unconverted site assumes.
    /// This is the test that says the piece is a generalisation and not a
    /// change: inline is the width axis, block is the height axis.
    #[test]
    fn horizontal_tb_ltr_keeps_inline_on_the_width_axis() {
        let m = AxisMap::horizontal_tb();
        assert_eq!(m.physical(Axis::Inline), PhysicalAxis::X);
        assert_eq!(m.physical(Axis::Block), PhysicalAxis::Y);
        assert_eq!(m.start(Axis::Inline), SideName::Left);
        assert_eq!(m.end(Axis::Inline), SideName::Right);
        assert_eq!(m.start(Axis::Block), SideName::Top);
        assert_eq!(m.end(Axis::Block), SideName::Bottom);
    }

    /// And the default is that same case, so a box that never sets a writing
    /// mode behaves as the engine did.
    #[test]
    fn the_default_map_is_horizontal_tb_ltr() {
        assert_eq!(AxisMap::default(), AxisMap::horizontal_tb());
    }

    // ---- rtl: the inline axis starts on the RIGHT ----

    /// `direction: rtl` inverts the SENSE of the inline axis and nothing else.
    /// The axis is still X — `rtl` never makes a page vertical — and the block
    /// axis is untouched, which is the half a "flip everything" mirror gets
    /// wrong.
    #[test]
    fn rtl_starts_the_inline_axis_on_the_right_and_leaves_the_block_axis_alone() {
        let m = AxisMap::new(WritingMode::HorizontalTb, Direction::Rtl);
        assert_eq!(m.physical(Axis::Inline), PhysicalAxis::X);
        assert_eq!(m.start(Axis::Inline), SideName::Right);
        assert_eq!(m.end(Axis::Inline), SideName::Left);
        assert!(!m.forward(Axis::Inline));
        assert_eq!(m.start(Axis::Block), SideName::Top, "rtl does not touch the block axis");
        assert!(m.forward(Axis::Block));
    }

    // ---- vertical-rl: the inline axis is the VERTICAL one ----

    /// The behaviour the whole module exists for: in `vertical-rl` a line runs
    /// down the page, so the inline axis is Y, and blocks stack leftwards from
    /// the right edge, so the block axis is X starting at `right`.
    #[test]
    fn vertical_rl_puts_the_inline_axis_on_y_and_stacks_blocks_from_the_right() {
        let m = vertical_rl();
        assert_eq!(m.physical(Axis::Inline), PhysicalAxis::Y);
        assert_eq!(m.physical(Axis::Block), PhysicalAxis::X);
        assert_eq!(m.start(Axis::Inline), SideName::Top);
        assert_eq!(m.start(Axis::Block), SideName::Right);
        assert_eq!(m.end(Axis::Block), SideName::Left);
        assert!(!m.inline_is_horizontal());
    }

    /// `vertical-lr` is the same swap with the block axis running the other
    /// way — the pair that makes "vertical" alone an insufficient answer.
    #[test]
    fn vertical_lr_swaps_the_axes_too_but_stacks_blocks_from_the_left() {
        let m = AxisMap::new(WritingMode::VerticalLr, Direction::Ltr);
        assert_eq!(m.physical(Axis::Inline), PhysicalAxis::Y);
        assert_eq!(m.start(Axis::Inline), SideName::Top);
        assert_eq!(m.start(Axis::Block), SideName::Left);
    }

    /// In a vertical mode `direction` moves the INLINE axis, which is now Y —
    /// a `rtl` there starts a line at the BOTTOM, not at the right. This is
    /// the combination a per-site mirror written for `horizontal-tb` gets
    /// exactly backwards.
    #[test]
    fn rtl_in_a_vertical_mode_starts_the_line_at_the_bottom() {
        let m = AxisMap::new(WritingMode::VerticalRl, Direction::Rtl);
        assert_eq!(m.start(Axis::Inline), SideName::Bottom);
        assert_eq!(m.end(Axis::Inline), SideName::Top);
        assert_eq!(m.start(Axis::Block), SideName::Right, "the block axis still ignores direction");
    }

    /// `sideways-lr` is the one value whose inline axis runs bottom→top by
    /// default, and whose `rtl` therefore puts it back to top→bottom. It is
    /// here because it is the case that proves this module delegates: the XOR
    /// lives in `eixo_y_forward`, and getting it by composition rather than by
    /// a fifth hand-written arm is the point.
    #[test]
    fn sideways_lr_runs_its_inline_axis_upwards_and_rtl_puts_it_back() {
        let ltr = AxisMap::new(WritingMode::SidewaysLr, Direction::Ltr);
        assert_eq!(ltr.start(Axis::Inline), SideName::Bottom);
        assert_eq!(ltr.start(Axis::Block), SideName::Left);
        let rtl = AxisMap::new(WritingMode::SidewaysLr, Direction::Rtl);
        assert_eq!(rtl.start(Axis::Inline), SideName::Top);
    }

    // ---- the inverse, and that the two agree ----

    /// `logical` is the inverse of `physical` in all four combinations. Stated
    /// as a round trip because the failure mode of an inverse written by hand
    /// is that it agrees in the common case and not in the swapped one.
    #[test]
    fn the_logical_and_physical_maps_are_inverses_in_every_mode() {
        for m in [
            AxisMap::horizontal_tb(),
            AxisMap::new(WritingMode::HorizontalTb, Direction::Rtl),
            vertical_rl(),
            AxisMap::new(WritingMode::VerticalLr, Direction::Rtl),
            AxisMap::new(WritingMode::SidewaysRl, Direction::Ltr),
        ] {
            for axis in [Axis::Inline, Axis::Block] {
                assert_eq!(m.logical(m.physical(axis)), axis, "{m:?} {axis:?}");
            }
            assert_ne!(
                m.physical(Axis::Inline),
                m.physical(Axis::Block),
                "the two logical axes never land on the same physical one"
            );
        }
    }

    /// `end` is the opposite side of `start`, always — which is what stops a
    /// caller from reaching for a `_ =>` arm when it needs the far end.
    #[test]
    fn end_is_always_the_side_opposite_start() {
        for wm in [
            WritingMode::HorizontalTb,
            WritingMode::VerticalRl,
            WritingMode::VerticalLr,
            WritingMode::SidewaysRl,
            WritingMode::SidewaysLr,
        ] {
            for dir in [Direction::Ltr, Direction::Rtl] {
                let m = AxisMap::new(wm, dir);
                for axis in [Axis::Inline, Axis::Block] {
                    assert_ne!(m.start(axis), m.end(axis));
                    assert_eq!(m.end(m.logical(m.physical(axis))), m.end(axis));
                }
            }
        }
    }

    /// The two sides of one logical axis are on the SAME physical axis — the
    /// invariant that a start/end pair can never be `left`/`bottom`.
    #[test]
    fn a_logical_axis_never_names_sides_from_two_different_physical_axes() {
        for wm in [WritingMode::HorizontalTb, WritingMode::VerticalRl, WritingMode::SidewaysLr] {
            for dir in [Direction::Ltr, Direction::Rtl] {
                let m = AxisMap::new(wm, dir);
                for axis in [Axis::Inline, Axis::Block] {
                    let horizontal = |s| matches!(s, SideName::Left | SideName::Right);
                    assert_eq!(
                        horizontal(m.start(axis)),
                        horizontal(m.end(axis)),
                        "{wm:?} {dir:?} {axis:?}"
                    );
                    assert_eq!(
                        horizontal(m.start(axis)),
                        m.physical(axis) == PhysicalAxis::X,
                        "the side named must be on the axis reported"
                    );
                }
            }
        }
    }

    // ---- the containing block, which is where the map is first USED ----
    //
    // These live here rather than in `containing_block.rs` because what they
    // pin is the MAPPING — which physical extent becomes which logical one —
    // and that is this module's answer. Keeping them beside it also keeps
    // `containing_block.rs` under the crate's 500-line ceiling.

    fn ctx(parent_w: f32) -> ResolveCtx {
        ResolveCtx {
            parent_content_w: parent_w,
            node_font_size: 16.0,
            root_font_size: 16.0,
            viewport_w: 800.0,
            viewport_h: 600.0,
        }
    }

    /// `horizontal-tb` is the particular case and answers exactly what it
    /// answered before the map existed: the width is the inline extent.
    ///
    /// Said against the general constructor rather than against a literal, so
    /// that the two cannot drift: if `physical` ever stopped agreeing with
    /// `horizontal_tb`, every unconverted caller in the engine would be wrong
    /// and nothing else would say so.
    #[test]
    fn the_horizontal_tb_constructor_is_the_general_one_in_its_own_mode() {
        let named = ContainingBlock::horizontal_tb(200.0, Some(50.0));
        let general = ContainingBlock::physical(200.0, Some(50.0), AxisMap::horizontal_tb());
        assert_eq!(named, general);
        assert_eq!(named.basis(Axis::Inline), PercentBasis::Definite(200.0));
        assert_eq!(named.basis(Axis::Block), PercentBasis::Definite(50.0));
    }

    /// The behaviour this lot delivers: in `vertical-rl` a line runs down the
    /// page, so the HEIGHT is the inline extent. A `50%` inline size in a
    /// 200×50 containing block is 25 there and 100 in `horizontal-tb` — the
    /// same declaration, two answers, decided in one place.
    #[test]
    fn a_vertical_writing_mode_makes_the_height_the_inline_extent() {
        let cb = ContainingBlock::physical(200.0, Some(50.0), vertical_rl());
        assert_eq!(cb.basis(Axis::Inline), PercentBasis::Definite(50.0));
        assert_eq!(cb.basis(Axis::Block), PercentBasis::Definite(200.0));
        assert_eq!(cb.resolve(Dimension::Percent(50.0), Axis::Inline, &ctx(200.0)), Some(25.0));
        assert_eq!(cb.resolve(Dimension::Percent(50.0), Axis::Block, &ctx(200.0)), Some(100.0));
    }

    /// And an `auto` height in a vertical mode makes the INLINE axis the
    /// indefinite one — the swap carries the definiteness with it, which is
    /// the part a caller converting by hand forgets.
    #[test]
    fn an_auto_height_in_a_vertical_mode_leaves_the_inline_axis_indefinite() {
        let axes = AxisMap::new(WritingMode::VerticalLr, Direction::Ltr);
        let cb = ContainingBlock::physical(200.0, None, axes);
        assert_eq!(cb.basis(Axis::Inline), PercentBasis::Indefinite);
        assert_eq!(cb.resolve(Dimension::Percent(50.0), Axis::Inline, &ctx(200.0)), None);
        assert_eq!(cb.resolve(Dimension::Percent(50.0), Axis::Block, &ctx(200.0)), Some(100.0));
    }

    /// The physical view is the way back, and it is the SAME rectangle in both
    /// modes: what changed is the name of each extent, never its value.
    #[test]
    fn the_physical_extents_are_unchanged_by_the_writing_mode() {
        for wm in [WritingMode::HorizontalTb, WritingMode::VerticalRl] {
            let cb = ContainingBlock::physical(200.0, Some(50.0), AxisMap::new(wm, Direction::Ltr));
            assert_eq!(cb.extent(PhysicalAxis::X), PercentBasis::Definite(200.0), "{wm:?}");
            assert_eq!(cb.extent(PhysicalAxis::Y), PercentBasis::Definite(50.0), "{wm:?}");
        }
    }

    /// `direction` decides a SENSE and never an extent, so `rtl` moves nothing
    /// here. Stated because a containing block is the natural place to reach
    /// for when a mirror is needed, and it is the wrong one:
    /// [`AxisMap::start`] is where the side lives.
    #[test]
    fn rtl_does_not_move_either_extent() {
        let ltr = ContainingBlock::horizontal_tb(200.0, Some(50.0));
        let rtl = ltr.with_axes(AxisMap::new(WritingMode::HorizontalTb, Direction::Rtl));
        assert_eq!(rtl.basis(Axis::Inline), ltr.basis(Axis::Inline));
        assert_eq!(rtl.basis(Axis::Block), ltr.basis(Axis::Block));
        assert_eq!(rtl.axes().start(Axis::Inline), SideName::Right);
    }
}
