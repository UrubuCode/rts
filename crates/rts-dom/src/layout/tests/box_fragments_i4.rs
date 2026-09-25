//! BT-2c, invariant I4: the fragments of an inline exist as such. A box keeps
//! one rect per line it appears on; the union is a view taken at the boundary
//! (`rect_of`, the hit-test geometry), never the only thing recorded.

use super::*;

fn laid_out(html: &str) -> (crate::dom::Dom, DisplayList) {
    let dom = parse_html_to_dom(html);
    let ctx = LayoutCtx { viewport_w: 800.0, viewport_h: 600.0, measurer: &ApproxMeasurer };
    let list = layout_document(&dom, &ctx);
    (dom, list)
}

fn node(dom: &crate::dom::Dom, sel: &str) -> NodeIdx {
    dom.resolve(dom.query(sel).expect(sel)).expect("a live node")
}

fn union_of(rects: &[Rect]) -> Rect {
    rects[1..].iter().fold(rects[0], |acc, r| acc.union(*r))
}

#[test]
fn an_inline_on_two_lines_has_two_fragments_and_one_union_at_the_boundary() {
    let (dom, list) = laid_out("<div><span id=s>first<br>second</span></div>");
    let span = node(&dom, "#s");
    let [box_id] = list.tree.boxes_of(span) else { panic!("an unsplit span has one box") };
    let fragments = list.rects_of_box(*box_id);
    assert_eq!(fragments.len(), 2, "one rect per line: {fragments:?}");
    assert!(fragments[1].y > fragments[0].y, "in line order: {fragments:?}");
    let union = union_of(&fragments);
    assert_eq!(list.rect_of(span), Some(union));
    assert_eq!(list.rect_of_box(*box_id), Some(union));
    assert_eq!(list.geometry().rects.get(&span).copied(), Some(union));
}

/// Three segments of one owner on ONE line are one fragment, not three: the
/// line, not the segment, is the unit.
#[test]
fn several_segments_of_an_inline_on_one_line_make_one_fragment() {
    let (dom, list) = laid_out("<div><span id=s>a <b>b</b> c</span></div>");
    let span = node(&dom, "#s");
    let [box_id] = list.tree.boxes_of(span) else { panic!("an unsplit span has one box") };
    assert_eq!(list.rects_of_box(*box_id).len(), 1);
}

/// A split inline (CSS 2.1 §9.2.1.1) has one box per fragment, each with its
/// own rect. `rect_of` still takes in the block that split it
/// (`query/rect.rs`); the hit-test rect does not, or the second fragment
/// would win a click on the whole `<div>`.
#[test]
fn a_split_inline_has_a_rect_per_fragment_and_only_the_dom_rect_holds_the_block() {
    let (dom, list) = laid_out("<div><span id=s style=\"border:1px solid red\">a<div id=d style=\"height:30px\">b</div>c</span></div>");
    let span = node(&dom, "#s");
    let block = list.rect_of(node(&dom, "#d")).expect("the block has a rect");
    let boxes = list.tree.boxes_of(span).to_vec();
    assert!(boxes.len() >= 2, "the span is split: {boxes:?}");
    let mut fragments = Vec::new();
    for box_id in &boxes {
        let own = list.rects_of_box(*box_id);
        assert!(own.len() <= 1, "a fragment box on one line has one rect: {own:?}");
        fragments.extend(own);
    }
    assert!(fragments.len() >= 2, "the text before and after the block: {fragments:?}");
    let hit = list.geometry().rects.get(&span).copied().expect("the span is hit-testable");
    assert_eq!(hit, union_of(&fragments));
    let dom_rect = list.rect_of(span).expect("the span has a DOM rect");
    assert_eq!(dom_rect, hit.union(block));
    assert!(dom_rect.w > hit.w || dom_rect.h > hit.h, "the block widens only the DOM rect");
}
