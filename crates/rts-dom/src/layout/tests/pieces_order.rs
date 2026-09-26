//! The paint order as ONE sequence of pieces (BT-2b, `layout/pieces.rs`): three
//! answers the index arithmetic it replaced got wrong, each pinned on the
//! materialized list — the order a backend paints in.

use crate::paint::DisplayItem;
use crate::table::tests::geometria;

/// Index in `itens` of the first item matching `f`.
fn index_of(itens: &[DisplayItem], f: impl Fn(&DisplayItem) -> bool) -> usize {
    itens.iter().position(f).unwrap_or_else(|| panic!("no such item in {itens:?}"))
}

const BLUE: u32 = 0x0000_FFFF;

/// A positioned `overflow:hidden` box is laid out in a list of its own and
/// appended after the page. Its `EndClip` used to carry a count of subtrees in
/// ITS list, which the append did not translate: against the page's subtrees
/// it let the box's own block child be drawn after the clip closed — the whole
/// 100×100 child over a 20×20 box.
#[test]
fn a_positioned_overflow_box_clips_its_block_children_after_the_page() {
    let (_, list) = geometria(
        "<div style='height:50px;background:#eee'></div>\
         <div style='position:absolute;top:0;left:0;width:20px;height:20px;overflow:hidden'>\
         <div style='width:100px;height:100px;background:#00f'></div></div>",
        400.0,
    );
    let itens = list.materialized();
    let begin = index_of(&itens, |it| matches!(it, DisplayItem::BeginClip { rect, .. } if rect.w == 20.0));
    let child = index_of(&itens, |it| matches!(it, DisplayItem::SolidRect { color, .. } if *color == BLUE));
    let end = begin + index_of(&itens[begin..], |it| matches!(it, DisplayItem::EndClip));
    assert!(begin < child && child < end, "the child must lie inside its clip: {itens:?}");
}

/// A `mask-image` suppresses the background, and the overflow clip used to
/// find where the children start by RECOUNTING the box items with the
/// background included: one past the end of a list whose child was a reused
/// subtree, which panicked. The clip now opens where the box items ended.
#[test]
fn a_masked_overflow_box_with_a_background_opens_its_clip_after_the_box_items() {
    let (_, list) = geometria(
        "<div style='width:50px;height:50px;overflow:hidden;background:#f00;mask-image:url(x.png)'>\
         <div style='width:100px;height:100px;background:#00f'></div></div>",
        400.0,
    );
    let itens = list.materialized();
    let begin = index_of(&itens, |it| matches!(it, DisplayItem::BeginClip { .. }));
    let child = index_of(&itens, |it| matches!(it, DisplayItem::SolidRect { color, .. } if *color == BLUE));
    let end = index_of(&itens, |it| matches!(it, DisplayItem::EndClip));
    assert!(begin < child && child < end, "the child must lie inside the clip: {itens:?}");
}

/// A rotated box laid out straight into its parent's list (a float) after a
/// sibling served from the fragment cache: flattening the WHOLE list put the
/// `PushTransform` at the box's old item index, inside the sibling, and the
/// sibling's text turned with it. Only the box's own range is flattened now.
#[test]
fn a_rotated_float_after_a_cached_sibling_turns_only_itself() {
    let (dom, list) = geometria(
        "<div><div id=a><span>A</span></div>\
         <div style='float:left;transform:rotate(10deg);width:10px;height:10px;background:#f00'></div></div>",
        400.0,
    );
    let itens = list.materialized();
    let texto = index_of(&itens, |it| matches!(it, DisplayItem::Text { text, .. } if &**text == "A"));
    let push = index_of(&itens, |it| matches!(it, DisplayItem::PushTransform { .. }));
    assert!(texto < push, "the sibling's text must be painted before the transform opens: {itens:?}");
    // The sibling keeps its geometry: only the rotated box's subtrees are flattened.
    let a = dom.resolve(dom.query_all("#a")[0]).expect("live node");
    assert!(list.geometry_now().rects.contains_key(&a), "#a lost its rect");
}

const RED: u32 = 0xFF00_00FF;
const GREEN: u32 = 0x00FF_00FF;

/// CSS 2.1 Appendix E, layer 6/8: within one stacking context, a
/// `position:absolute` box with `z-index: auto`/`0` and a `position:relative`
/// sibling of the same layer paint in TREE ORDER — not the absolute always
/// last. An absolute red box followed by a later relative green sibling used
/// to paint the absolute LAST regardless, covering the green box that should
/// be on top.
#[test]
fn an_absolute_box_paints_before_a_later_relative_sibling_of_the_same_layer() {
    let (_, list) = geometria(
        "<div style='position:relative;width:100px;height:100px'>\
         <div style='position:absolute;top:0;left:0;width:100px;height:100px;background:#f00'></div>\
         <div style='position:relative;width:100px;height:100px;background:#0f0'></div>\
         </div>",
        400.0,
    );
    let itens = list.materialized();
    let red = index_of(&itens, |it| matches!(it, DisplayItem::SolidRect { color, .. } if *color == RED));
    let green = index_of(&itens, |it| matches!(it, DisplayItem::SolidRect { color, .. } if *color == GREEN));
    assert!(red < green, "the later relative sibling must paint OVER the absolute box: {itens:?}");
}

/// The mirror case: a `position:relative` box that comes BEFORE the absolute
/// sibling in the DOM keeps its order — tree order already had the absolute
/// on top, and this fix must not touch that.
#[test]
fn a_relative_box_before_an_absolute_sibling_keeps_its_order() {
    let (_, list) = geometria(
        "<div style='position:relative;width:100px;height:100px'>\
         <div style='position:relative;width:100px;height:100px;background:#0f0'></div>\
         <div style='position:absolute;top:0;left:0;width:100px;height:100px;background:#f00'></div>\
         </div>",
        400.0,
    );
    let itens = list.materialized();
    let green = index_of(&itens, |it| matches!(it, DisplayItem::SolidRect { color, .. } if *color == GREEN));
    let red = index_of(&itens, |it| matches!(it, DisplayItem::SolidRect { color, .. } if *color == RED));
    assert!(green < red, "the absolute box still follows the earlier relative sibling: {itens:?}");
}

/// A NEGATIVE `z-index` on the absolute box is unchanged by this fix: it
/// still paints BEHIND the whole normal flow, tree-order splicing among
/// layer 8 siblings notwithstanding.
#[test]
fn a_negative_z_index_absolute_box_still_paints_behind_the_flow() {
    let (_, list) = geometria(
        "<div style='position:relative;width:100px;height:100px;background:#0f0'>\
         <div style='position:relative;width:100px;height:100px'></div>\
         <div style='position:absolute;z-index:-1;top:0;left:0;width:100px;height:100px;background:#f00'></div>\
         </div>",
        400.0,
    );
    let itens = list.materialized();
    let green = index_of(&itens, |it| matches!(it, DisplayItem::SolidRect { color, .. } if *color == GREEN));
    let red = index_of(&itens, |it| matches!(it, DisplayItem::SolidRect { color, .. } if *color == RED));
    assert!(red < green, "negative z-index must still paint behind the flow: {itens:?}");
}

/// The WPT shape (`css-position/position-relative-table-td-left.html`): a
/// `position:relative` `<td>` — background painted by an INSERT at its own
/// `box_start` after its children are laid out, which lands the `<td>`'s
/// background one slot before its own `Piece::Rect` mark (`bloco.rs`, the
/// same `box_start`/`record_box_rect` insert every box with a background
/// goes through). `splice_layer8` has to land the absolute BEFORE that
/// background too, not just before the `Rect` mark, or the relative `<td>`'s
/// own background stays ahead of it and hides it exactly as it did before
/// this fix — just one layer deeper than the two-`<div>` case above.
#[test]
fn an_absolute_box_paints_before_a_later_relative_table_cell_with_its_own_background() {
    let html = "<style>\
table { border-collapse:collapse; }\
td { padding: 0; }\
td > div { height: 50px; width: 50px; }\
.group { display: inline-block; position: relative; width: 150px; height: 200px; }\
.indicator { position: absolute; background-color: red; left: 100px; height: 50px; width: 50px; }\
.relative { position: relative; left: 100px; background-color: green; }\
</style>\
<div class=\"group\">\
  <div>\
    <div class=\"indicator\"></div>\
    <table>\
      <tbody>\
        <tr><td class=\"relative\"><div></div></td></tr>\
        <tr><td><div></div></td></tr>\
      </tbody>\
    </table>\
  </div>\
</div>";
    // CSS `green` is (0, 128, 0), not `#0f0` — a different constant from the
    // two tests above, which use a bright green background instead.
    const CSS_GREEN: u32 = 0x0080_00FF;
    let (_, list) = geometria(html, 1280.0);
    let itens = list.materialized();
    let red = index_of(&itens, |it| matches!(it, DisplayItem::SolidRect { color, .. } if *color == RED));
    let green = index_of(&itens, |it| matches!(it, DisplayItem::SolidRect { color, .. } if *color == CSS_GREEN));
    assert!(red < green, "the relative cell's OWN background must not paint ahead of the absolute box it follows in the DOM: {itens:?}");
}

/// The WPT shape of `position-relative-table-tbody-left-absolute-child.html`:
/// an absolute box is a DESCENDANT of a `position:relative` ancestor (its own
/// containing block), not a sibling. Appendix E paints a positioned box's
/// descendants INSIDE that box's own place in the paint order — never spliced
/// to before it, however much earlier an out-of-flow layer-8 SIBLING of the
/// ancestor has to land. `splice_layer8` used to match the relative ancestor
/// itself as a candidate for such a sibling and, separately, to corrupt the
/// position of what it spliced one level into a cached fragment (see the
/// module doc): this pins both — the sibling still lands before the relative
/// ancestor, AND the descendant paints at the SAME place as what it must
/// cover.
#[test]
fn an_absolute_descendant_of_a_relative_ancestor_paints_after_the_ancestors_own_background() {
    let html = "<style>\
table { border-collapse:collapse; }\
td { padding: 0; }\
td > div { height: 50px; width: 50px; }\
.group { display: inline-block; position: relative; width: 150px; height: 200px; }\
.indicator { position: absolute; background-color: red; left: 100px; height: 50px; width: 50px; }\
.relative { position: relative; left: 50px; background-color: green; }\
.absolute { position: absolute; left: 50px; background-color: green; }\
</style>\
<div class=\"group\">\
  <div>\
    <div class=\"indicator\"></div>\
    <table>\
      <tbody class=\"relative\">\
        <tr><td><div class=\"absolute\"></div></td></tr>\
      </tbody>\
    </table>\
  </div>\
</div>";
    const CSS_GREEN: u32 = 0x0080_00FF;
    let (dom, list) = geometria(html, 1280.0);
    let itens = list.materialized();
    let red = index_of(&itens, |it| matches!(it, DisplayItem::SolidRect { color, .. } if *color == RED));
    let green = index_of(&itens, |it| matches!(it, DisplayItem::SolidRect { color, .. } if *color == CSS_GREEN));
    assert!(red < green, "the absolute descendant of the later relative ancestor must paint on top of the earlier indicator: {itens:?}");
    // Both boxes must land at the EXACT same rectangle — `.indicator`'s inset
    // is computed against `.group`, and `.absolute`'s against `tbody` shifted
    // by its own `left`, and the two are meant to coincide pixel for pixel.
    let indicator_rect = crate::table::tests::rect(&dom, &list, ".indicator", 0);
    let absolute_rect = crate::table::tests::rect(&dom, &list, ".absolute", 0);
    assert_eq!(
        indicator_rect, absolute_rect,
        "the absolute descendant must be painted at the SAME rect as the indicator it covers, \
         not shifted by an ancestor fragment's own reuse offset"
    );
}
