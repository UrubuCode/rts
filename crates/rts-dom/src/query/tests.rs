//! The hit-test read from the fragment tree (Phase B of
//! `docs/superpowers/plans/2026-09-25-paint-and-query.md`): what the per-node
//! table could not answer, pinned by the point that tells the two apart.

use crate::dom::{parse_html_to_dom, Dom, NodeIdx};
use crate::layout::{layout_document, ApproxMeasurer, LayoutCtx};
use crate::paint::list::DisplayList;

fn laid_out(html: &str) -> (Dom, DisplayList) {
    let dom = parse_html_to_dom(html);
    let ctx = LayoutCtx { viewport_w: 800.0, viewport_h: 600.0, measurer: &ApproxMeasurer };
    let list = layout_document(&dom, &ctx);
    (dom, list)
}

fn node(dom: &Dom, sel: &str) -> NodeIdx {
    dom.resolve(dom.query(sel).expect(sel)).expect("a live node")
}

/// A short first line and a long second one: the union of the link's two
/// fragments covers the empty space after "x", and only the fragments do not.
#[test]
fn a_link_wrapped_across_two_lines_is_hit_on_both_lines_and_not_in_the_gap() {
    let (dom, list) = laid_out("<div id=d><a id=a href=#>x<br>a much longer second line</a></div>");
    let a = node(&dom, "#a");
    let d = node(&dom, "#d");
    let [box_id] = list.tree.boxes_of(a) else { panic!("an unsplit link has one box") };
    let frags = list.rects_of_box(*box_id);
    assert_eq!(frags.len(), 2, "one fragment per line: {frags:?}");
    let (first, second) = (frags[0], frags[1]);
    assert!(second.x + second.w > first.x + first.w + 10.0, "the second line is longer: {frags:?}");

    assert_eq!(list.hit_test(first.x + 1.0, first.y + first.h / 2.0), Some(a), "first line");
    assert_eq!(list.hit_test(second.x + second.w - 1.0, second.y + second.h / 2.0), Some(a), "second line");
    let gap = (first.x + first.w + 5.0, first.y + first.h / 2.0);
    assert_eq!(list.hit_test(gap.0, gap.1), Some(d), "after the first line's end: the block, not the link");
}

/// The behaviour change of Phase B: the child sticking out of an
/// `overflow:hidden` box is not painted there, so it is not hit there either.
#[test]
fn an_overflow_hidden_child_is_not_hit_outside_the_clip() {
    let (dom, list) = laid_out(
        "<div id=c style='overflow:hidden;width:100px;height:100px'><div id=k style='width:300px;height:50px'></div></div>",
    );
    let k = node(&dom, "#k");
    let c = node(&dom, "#c");
    let rc = list.rect_of(c).expect("the clip box has a rect");
    let y = rc.y + 10.0;
    assert_eq!(list.hit_test(rc.x + 50.0, y), Some(k), "inside the clip the child answers");
    let outside = list.hit_test(rc.x + 200.0, y);
    assert_ne!(outside, Some(k), "outside the clip the child is not there");
    assert_ne!(outside, Some(c));
    assert_eq!(dom.hit_test_clickable(&list, rc.x + 200.0, y), outside, "the same walk, a filter that refuses nothing here");
}

#[test]
fn a_z_index_positioned_box_over_a_flow_box_wins() {
    let (dom, list) = laid_out(
        "<div id=flow style='height:100px'></div><div id=over style='position:absolute;left:0;top:0;width:50px;height:50px;z-index:1'></div>",
    );
    let flow = node(&dom, "#flow");
    let over = node(&dom, "#over");
    let rf = list.rect_of(flow).expect("the flow box has a rect");
    let ro = list.rect_of(over).expect("the positioned box has a rect");
    assert_eq!(list.hit_test(ro.x + 5.0, ro.y + 5.0), Some(over));
    assert_eq!(list.hit_test(rf.x + rf.w - 5.0, rf.y + rf.h - 5.0), Some(flow));
}
