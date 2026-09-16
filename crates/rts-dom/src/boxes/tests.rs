use super::*;
use crate::dom::NodeKind;

/// An element box knows which node it came from, and the node knows which
/// boxes it generated. The translation lives in one place: that is what stops
/// `NodeIdx` becoming the identity again by accident in some other consumer.
#[test]
fn the_translation_between_node_and_box_is_one_to_one_in_the_mirror() {
    let mut tree = BoxTree::default();
    let root = tree.push_element(0, None);
    let child = tree.push_element(1, Some(root));

    assert_eq!(tree.boxes_of(1), &[child]);
    assert_eq!(tree.node_of(child), Some(1));
    assert_eq!(tree.children(root), &[child]);
    assert_eq!(tree.parent(child), Some(root));
}

/// An ANONYMOUS box has no node. None are created in this lot, but the type has
/// to admit them from the start — otherwise the next lot goes back and changes
/// everything that reads it.
#[test]
fn an_anonymous_box_answers_no_node() {
    let mut tree = BoxTree::default();
    let parent = tree.push_element(0, None);
    let anon = tree.push_anonymous(0, parent);

    assert_eq!(tree.node_of(anon), None);
    assert_eq!(tree.children(parent), &[anon]);
    assert!(
        tree.boxes_of(0).iter().all(|&b| b != anon),
        "an anonymous box must not be reachable from any node"
    );
}

/// One element can own SEVERAL boxes, and the map keeps them all in creation
/// order. This is the case `record_node_rect` could not represent: it inserted
/// into a map keyed by node, so the second box silently replaced the first.
#[test]
fn one_node_can_own_several_boxes_and_the_map_keeps_the_order() {
    let mut tree = BoxTree::default();
    let first = tree.push_element(7, None);
    let second = tree.push_element(7, Some(first));

    assert_eq!(tree.boxes_of(7), &[first, second]);
    assert_eq!(tree.node_of(first), Some(7));
    assert_eq!(tree.node_of(second), Some(7));
}

/// The mirror has exactly one box per ELEMENT, and none for a text or comment
/// node. A text node generates no box in this lot: its style is the enclosing
/// inline's and what lays it out is `collect_runs`, which still reads the DOM.
/// Giving text a box of its own is BT-3.
#[test]
fn the_mirror_has_one_box_per_element_and_none_for_text() {
    let dom = crate::parse_html_to_dom("<div><p>a</p><!--c--><span></span></div>");
    let tree = build_mirror(&dom);

    let mut elements = 0;
    for idx in 0..dom.node_count() {
        let is_element = matches!(&dom.node(idx).kind, NodeKind::Element { .. });
        let boxes = tree.boxes_of(idx).len();
        if is_element {
            elements += 1;
            assert!(
                boxes <= 1,
                "element {idx} has {boxes} boxes; the mirror allows at most one"
            );
        } else {
            assert_eq!(boxes, 0, "a non-element node generates no box in the mirror");
        }
    }
    assert!(elements >= 3, "the fixture has div, p and span");
}

/// Every box in the mirror is reachable from a root by following children, and
/// the parent link agrees with the child link. A tree whose two directions
/// disagree is the kind of thing that answers correctly until the day someone
/// traverses it the other way.
#[test]
fn the_mirror_is_a_tree_whose_two_directions_agree() {
    let dom = crate::parse_html_to_dom("<div><p><b>a</b></p><span></span></div>");
    let tree = build_mirror(&dom);
    assert!(!tree.is_empty(), "the fixture generates boxes");

    let mut seen = vec![false; tree.len()];
    let mut stack: Vec<BoxId> = tree.roots().collect();
    while let Some(id) = stack.pop() {
        assert!(!seen[id.0 as usize], "box {id:?} reached twice");
        seen[id.0 as usize] = true;
        for &child in tree.children(id) {
            assert_eq!(
                tree.parent(child),
                Some(id),
                "the child link and the parent link must agree"
            );
            stack.push(child);
        }
    }
    assert!(
        seen.iter().all(|&s| s),
        "every box must be reachable from a root"
    );
}

/// The central case the box tree exists for (CSS 2.1 §9.2.1.1, 148 CSS2
/// reftests): a `<div>` inside a `<span>` splits the inline into an anonymous
/// block before it, the `<div>` itself, and an anonymous block after —
/// instead of the block-level `<div>` being descended into as though the
/// `<span>` were a transparent wrapper.
#[test]
fn a_block_child_of_an_inline_splits_it_around_an_anonymous_block() {
    let dom = crate::parse_html_to_dom(
        "<p><span><b>before</b><div>block</div><b>after</b></span></p>",
    );
    let tree = build_mirror(&dom);

    // The `<span>` is display:inline by the UA stylesheet default.
    let span_node = (0..dom.node_count())
        .find(|&i| matches!(&dom.node(i).kind, NodeKind::Element { tag } if tag == "span"))
        .expect("fixture has a span");
    let span_box = tree.boxes_of(span_node)[0];

    let div_node = (0..dom.node_count())
        .find(|&i| matches!(&dom.node(i).kind, NodeKind::Element { tag } if tag == "div"))
        .expect("fixture has a div");
    let div_box = tree.boxes_of(div_node)[0];

    let span_children = tree.children(span_box);
    assert_eq!(
        span_children.len(),
        3,
        "before-anon, the div, after-anon: {span_children:?}"
    );

    let before_anon = span_children[0];
    let middle = span_children[1];
    let after_anon = span_children[2];

    assert!(tree.node_of(before_anon).is_none(), "the before run is anonymous");
    assert_eq!(middle, div_box, "the block child is a sibling, not nested");
    assert!(tree.node_of(after_anon).is_none(), "the after run is anonymous");

    // The anonymous box takes its style from the element that generated it —
    // the span. It does not STORE that style: it stores where the style comes
    // from, so an animation frame is never read one frame late. Asserting on
    // the source is therefore the assertion that survives.
    assert_eq!(tree.style_source(before_anon), tree.style_source(span_box));
    assert_eq!(tree.style_source(after_anon), tree.style_source(span_box));
    assert_eq!(tree.style_source(span_box), span_node);

    // The `<b>` elements land inside their anonymous wrapper, not as direct
    // children of the span's box.
    let b_before = tree
        .boxes_of(
            (0..dom.node_count())
                .find(|&i| {
                    matches!(&dom.node(i).kind, NodeKind::Element { tag } if tag == "b")
                })
                .unwrap(),
        )[0];
    assert_eq!(tree.parent(b_before), Some(before_anon));
}

/// A block-level child as the FIRST child of an inline: there is no inline
/// content before it, so no anonymous box is created on that side — only the
/// trailing run gets one.
#[test]
fn a_block_as_the_first_child_gets_no_anonymous_box_before_it() {
    let dom = crate::parse_html_to_dom("<span><div>block</div><b>after</b></span>");
    let tree = build_mirror(&dom);

    let span_node = (0..dom.node_count())
        .find(|&i| matches!(&dom.node(i).kind, NodeKind::Element { tag } if tag == "span"))
        .unwrap();
    let span_box = tree.boxes_of(span_node)[0];
    let div_node = (0..dom.node_count())
        .find(|&i| matches!(&dom.node(i).kind, NodeKind::Element { tag } if tag == "div"))
        .unwrap();
    let div_box = tree.boxes_of(div_node)[0];

    let children = tree.children(span_box);
    assert_eq!(children.len(), 2, "the div, then one trailing anon: {children:?}");
    assert_eq!(children[0], div_box);
    assert!(tree.node_of(children[1]).is_none());
}

/// A block-level child as the LAST child of an inline: the mirror image of
/// the case above, no anonymous box after it.
#[test]
fn a_block_as_the_last_child_gets_no_anonymous_box_after_it() {
    let dom = crate::parse_html_to_dom("<span><b>before</b><div>block</div></span>");
    let tree = build_mirror(&dom);

    let span_node = (0..dom.node_count())
        .find(|&i| matches!(&dom.node(i).kind, NodeKind::Element { tag } if tag == "span"))
        .unwrap();
    let span_box = tree.boxes_of(span_node)[0];
    let div_node = (0..dom.node_count())
        .find(|&i| matches!(&dom.node(i).kind, NodeKind::Element { tag } if tag == "div"))
        .unwrap();
    let div_box = tree.boxes_of(div_node)[0];

    let children = tree.children(span_box);
    assert_eq!(children.len(), 2, "one leading anon, then the div: {children:?}");
    assert!(tree.node_of(children[0]).is_none());
    assert_eq!(children[1], div_box);
}

/// Two block-level children in a row: no anonymous box is sandwiched between
/// them, because there is no inline content between them to enclose.
#[test]
fn two_consecutive_block_children_get_no_anonymous_box_between_them() {
    let dom = crate::parse_html_to_dom("<span><div>a</div><div>b</div></span>");
    let tree = build_mirror(&dom);

    let span_node = (0..dom.node_count())
        .find(|&i| matches!(&dom.node(i).kind, NodeKind::Element { tag } if tag == "span"))
        .unwrap();
    let span_box = tree.boxes_of(span_node)[0];

    let children = tree.children(span_box);
    assert_eq!(children.len(), 2, "no anon box between two blocks: {children:?}");
    assert!(tree.node_of(children[0]).is_some(), "the first div");
    assert!(tree.node_of(children[1]).is_some(), "the second div");
}

/// An inline element with no block-level child at all never splits — the
/// mirror stays exact for the ordinary case.
#[test]
fn an_inline_with_no_block_child_is_not_split() {
    let dom = crate::parse_html_to_dom("<span><b>a</b><i>b</i></span>");
    let tree = build_mirror(&dom);

    let span_node = (0..dom.node_count())
        .find(|&i| matches!(&dom.node(i).kind, NodeKind::Element { tag } if tag == "span"))
        .unwrap();
    let span_box = tree.boxes_of(span_node)[0];

    // Two element children (`<b>`, `<i>`), no anonymous box introduced.
    let children = tree.children(span_box);
    assert_eq!(children.len(), 2);
    assert!(children.iter().all(|&c| tree.node_of(c).is_some()));
}

/// `display:inline-block` is inline-LEVEL but not an inline BOX: it
/// establishes its own block-formatting context, so a block-level child
/// inside it is ordinary content and does not trigger the split. Only plain
/// `display:inline` does.
#[test]
fn an_inline_block_with_a_block_child_is_not_split() {
    let dom = crate::parse_html_to_dom(
        r#"<span style="display:inline-block"><div>block</div></span>"#,
    );
    let tree = build_mirror(&dom);

    let span_node = (0..dom.node_count())
        .find(|&i| matches!(&dom.node(i).kind, NodeKind::Element { tag } if tag == "span"))
        .unwrap();
    let span_box = tree.boxes_of(span_node)[0];
    let div_node = (0..dom.node_count())
        .find(|&i| matches!(&dom.node(i).kind, NodeKind::Element { tag } if tag == "div"))
        .unwrap();
    let div_box = tree.boxes_of(div_node)[0];

    let children = tree.children(span_box);
    assert_eq!(children, &[div_box], "the div nests directly, no anonymous box");
}

/// The tree is memoised on the `Dom`, and the memo survives a second call
/// without rebuilding — two calls on an unchanged document hand back the same
/// allocation.
///
/// It lives on the `Dom` and not in `LayoutCtx` because 111 sites construct a
/// `LayoutCtx`; the reason is worth a test because the next person to look for
/// the tree will look in the context first.
#[test]
fn the_tree_is_memoised_on_the_document() {
    let dom = crate::parse_html_to_dom("<div><p>a</p></div>");
    let first = dom.box_tree();
    let second = dom.box_tree();
    assert!(
        Rc::ptr_eq(&first, &second),
        "an unchanged document must hand back the same tree, not an equal one"
    );
}

/// Two boxes of one node aggregate to their bounding rectangle through
/// `DisplayList::rect_of_node` — the boundary box-tree.md §6 names: "Internally
/// there may be N boxes per element; at that boundary they aggregate."
#[test]
fn rect_of_node_unions_the_rectangles_of_a_nodes_several_boxes() {
    use crate::layout::{DisplayList, Rect};

    let mut tree = BoxTree::default();
    let first = tree.push_element(3, None);
    let second = tree.push_element(3, Some(first));

    let mut list = DisplayList::default();
    list.tree = Rc::new(tree);
    list.box_rects.insert(first, Rect::new(10.0, 10.0, 20.0, 5.0));
    list.box_rects.insert(second, Rect::new(0.0, 0.0, 5.0, 5.0));

    let united = list.rect_of_node(3).expect("node 3 has boxes");
    assert_eq!(united, Rect::new(0.0, 0.0, 30.0, 15.0));
}

/// A box that was never written to `box_rects` has no entry at all — it is
/// skipped, not treated as a `0,0,0,0` fragment that would drag the union
/// toward the origin. That sentinel was `union_rect`'s problem, born of a
/// single slot shared by every box of a node; with one key per box the
/// ambiguity between "empty box at the origin" and "no box yet" cannot arise
/// by construction, which is what this test pins.
#[test]
fn a_box_never_written_is_absent_from_the_union_not_pulled_to_the_origin() {
    use crate::layout::{DisplayList, Rect};

    let mut tree = BoxTree::default();
    let first = tree.push_element(5, None);
    let second = tree.push_element(5, Some(first));

    let mut list = DisplayList::default();
    list.tree = Rc::new(tree);
    // Only `second` was ever laid out; `first` has no entry at all, not a
    // zeroed placeholder.
    let _ = first;
    list.box_rects.insert(second, Rect::new(100.0, 200.0, 10.0, 10.0));

    let rect = list.rect_of_node(5).expect("node 5 has one written box");
    assert_eq!(
        rect,
        Rect::new(100.0, 200.0, 10.0, 10.0),
        "the unwritten box must not pull the union toward the origin"
    );
}

/// A box does not STORE its style — it stores where the style comes from, and
/// asks the document each time.
///
/// This is the test for the hole that an agent found while migrating the
/// layout, and it is worth pinning because the failure it prevents is
/// invisible: the tree is memoised WITHOUT `anim_epoch` on purpose, so a style
/// captured at build time is one frame behind for the whole of a transition,
/// and every reader would be wrong with nothing to say so.
///
/// What it asserts is the observable consequence: change the style without
/// touching structure, and the box answers the NEW style through the same tree.
#[test]
fn a_box_reads_its_style_fresh_and_never_from_a_capture() {
    let mut dom = crate::parse_html_to_dom("<div></div>");
    let node = (0..dom.node_count())
        .find(|&i| matches!(&dom.node(i).kind, NodeKind::Element { tag } if tag == "div"))
        .expect("fixture has a div");
    let id = dom.id_of_idx(node);

    let tree = dom.box_tree();
    let caixa = tree.boxes_of(node)[0];
    let antes = tree
        .style(&dom, caixa)
        .expect("an element box has a style")
        .width;

    dom.set_style_property(id, "width", "123px");

    // The SAME tree instance, on purpose: if the box had captured the style,
    // this would still answer the old width.
    let depois = tree
        .style(&dom, caixa)
        .expect("an element box has a style")
        .width;

    assert_ne!(
        antes, depois,
        "the box must read the style now, not the one captured when the tree was built"
    );
}
