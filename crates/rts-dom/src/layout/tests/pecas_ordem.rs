//! The paint order as ONE sequence of pieces (BT-2b, `layout/pecas.rs`): three
//! answers the index arithmetic it replaced got wrong, each pinned on the
//! materialized list — the order a backend paints in.

use crate::layout::DisplayItem;
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
