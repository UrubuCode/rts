//! The second anonymous-block family of CSS 2.1 §9.2.1.1 — inline content in a
//! block container that also holds block-level boxes — with no split involved.

use super::tests::no_com_id;
use super::*;
use crate::dom::NodeKind;

fn box_of_id(dom: &crate::dom::Dom, tree: &BoxTree, id: &str) -> BoxId {
    tree.boxes_of(no_com_id(dom, id))[0]
}

fn is_anonymous(tree: &BoxTree, b: BoxId) -> bool {
    matches!(tree.kind(b), BoxKind::Anonymous { role: AnonymousRole::Block, .. })
}

/// The text of a box's only child, for asserting WHAT a run enclosed.
fn only_text_under(dom: &crate::dom::Dom, tree: &BoxTree, b: BoxId) -> String {
    let children = tree.children(b);
    assert_eq!(children.len(), 1, "{b:?} should enclose exactly one box");
    match tree.node_of(children[0]).map(|n| &dom.node(n).kind) {
        Some(NodeKind::Text(t)) => t.to_string(),
        other => panic!("expected a text box, found {other:?}"),
    }
}

#[test]
fn text_beside_a_block_is_wrapped_in_an_anonymous_block_on_each_side() {
    let dom = crate::parse_html_to_dom("<div id=o>a<div>b</div>c</div>");
    let tree = build_mirror(&dom);
    let outer = box_of_id(&dom, &tree, "o");
    let children = tree.children(outer).to_vec();
    assert_eq!(children.len(), 3, "anon[a], div, anon[c]");
    assert!(is_anonymous(&tree, children[0]));
    assert_eq!(only_text_under(&dom, &tree, children[0]), "a");
    assert!(matches!(tree.kind(children[1]), BoxKind::Element { .. }));
    assert!(is_anonymous(&tree, children[2]));
    assert_eq!(only_text_under(&dom, &tree, children[2]), "c");
    // An anonymous box inherits from the box that encloses it.
    assert_eq!(Some(tree.style_source(children[0])), tree.node_of(outer));
}

#[test]
fn a_container_of_inline_content_only_gets_no_anonymous_box() {
    let dom = crate::parse_html_to_dom("<div id=o>a b</div>");
    let tree = build_mirror(&dom);
    let div = box_of_id(&dom, &tree, "o");
    assert!(tree.children(div).iter().all(|&b| !is_anonymous(&tree, b)));
    assert!(tree.runs_inline_formatting_context(&dom, div));
}

/// Not a split — the span holds no block — and still wrapped, because its
/// SIBLING is block-level.
#[test]
fn an_inline_element_beside_a_block_is_wrapped_without_being_split() {
    let dom = crate::parse_html_to_dom("<div id=o><span id=s>a</span><div>b</div></div>");
    let tree = build_mirror(&dom);
    let span = no_com_id(&dom, "s");
    assert_eq!(tree.boxes_of(span).len(), 1, "the span is not split");
    let anon = tree.parent(tree.boxes_of(span)[0]).unwrap();
    assert!(is_anonymous(&tree, anon));
    let outer = box_of_id(&dom, &tree, "o");
    assert_eq!(tree.parent(anon), Some(outer));
    assert_eq!(tree.children(outer).len(), 2, "anon[span], div");
}

/// Source indentation and comments between blocks stay box-less (§9.2.1.1:
/// white space that would collapse generates no anonymous box).
#[test]
fn indentation_between_blocks_is_not_wrapped() {
    let dom = crate::parse_html_to_dom("<div id=o>\n  <p>x</p>\n  <!--c-->\n  <p>y</p>\n</div>");
    let tree = build_mirror(&dom);
    let o = box_of_id(&dom, &tree, "o");
    assert!(tree.children(o).iter().all(|&b| !is_anonymous(&tree, b)));
}

/// A flex container blockifies its children; there is no line to enclose.
#[test]
fn a_flex_container_mixing_text_and_blocks_gets_no_anonymous_block() {
    let dom = crate::parse_html_to_dom(r#"<div id=o style="display:flex">a<div>b</div></div>"#);
    let tree = build_mirror(&dom);
    let o = box_of_id(&dom, &tree, "o");
    assert!(tree.children(o).iter().all(|&b| !is_anonymous(&tree, b)));
}

/// The anonymous boxes are LAID OUT: each gets its own rect, stacked around
/// the block, full content width.
#[test]
fn each_anonymous_run_is_laid_out_as_a_block_around_the_block_sibling() {
    let dom = crate::parse_html_to_dom(
        r#"<div id=o style="width:200px">a<div id=b style="height:30px">b</div>c</div>"#,
    );
    let ctx = crate::layout::LayoutCtx {
        viewport_w: 800.0,
        viewport_h: 600.0,
        measurer: &crate::layout::ApproxMeasurer,
    };
    let list = crate::layout::layout_document(&dom, &ctx);
    let tree = std::rc::Rc::clone(&list.tree);
    let o = box_of_id(&dom, &tree, "o");
    let children = tree.children(o).to_vec();
    let before = list.rect_of_box(children[0]).expect("the first run has a rect");
    let after = list.rect_of_box(children[2]).expect("the last run has a rect");
    let b = list.rect_of_box(children[1]).expect("the block has a rect");
    assert_eq!(before.w, 200.0);
    assert!(before.h > 0.0);
    assert!((b.y - (before.y + before.h)).abs() < 0.01, "the block starts where the run ends");
    assert!((after.y - (b.y + 30.0)).abs() < 0.01, "the last run starts under the block");
}

/// A float between two blocks is out-of-flow block-level, not inline content:
/// it stays a direct child and no anonymous box appears around it.
#[test]
fn a_float_between_blocks_is_not_wrapped_and_keeps_its_box() {
    let dom = crate::parse_html_to_dom(
        r#"<div id=o>text<p>x</p> <span id=f style="float:left">f</span> <p>y</p></div>"#,
    );
    let tree = build_mirror(&dom);
    let o = box_of_id(&dom, &tree, "o");
    let f = tree.boxes_of(no_com_id(&dom, "f"));
    assert_eq!(f.len(), 1, "the float keeps its box");
    assert_eq!(tree.parent(f[0]), Some(o));
    let anonymous = tree.children(o).iter().filter(|&&b| is_anonymous(&tree, b)).count();
    assert_eq!(anonymous, 1, "only the text run is wrapped");
}

/// CSS 2.1 §8.3.1: an anonymous block whose run makes no line box is
/// zero-height, has no margins of its own, and the margins of the blocks on
/// either side collapse THROUGH it. An empty `<span>` generates no line box
/// (§9.4.2), so the gap is the larger margin, 30, and not 20 + 30.
#[test]
fn margins_collapse_through_an_anonymous_block_that_makes_no_line() {
    let dom = crate::parse_html_to_dom(
        r#"<div id=o style="width:200px"><p id=a style="margin:0 0 20px;height:10px"></p><span></span><p id=b style="margin:30px 0 0;height:10px"></p></div>"#,
    );
    let ctx = crate::layout::LayoutCtx {
        viewport_w: 800.0,
        viewport_h: 600.0,
        measurer: &crate::layout::ApproxMeasurer,
    };
    let list = crate::layout::layout_document(&dom, &ctx);
    let tree = std::rc::Rc::clone(&list.tree);
    let a = list.rect_of_box(box_of_id(&dom, &tree, "a")).unwrap();
    let b = list.rect_of_box(box_of_id(&dom, &tree, "b")).unwrap();
    assert_eq!(b.y - (a.y + a.h), 30.0);
}
