use super::*;
use crate::dom::NodeKind;

fn empty_style() -> Rc<ComputedStyle> {
    Rc::new(ComputedStyle::default())
}

/// An element box knows which node it came from, and the node knows which
/// boxes it generated. The translation lives in one place: that is what stops
/// `NodeIdx` becoming the identity again by accident in some other consumer.
#[test]
fn the_translation_between_node_and_box_is_one_to_one_in_the_mirror() {
    let mut tree = BoxTree::default();
    let root = tree.push_element(0, empty_style(), None);
    let child = tree.push_element(1, empty_style(), Some(root));

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
    let parent = tree.push_element(0, empty_style(), None);
    let anon = tree.push_anonymous(empty_style(), parent);

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
    let first = tree.push_element(7, empty_style(), None);
    let second = tree.push_element(7, empty_style(), Some(first));

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
