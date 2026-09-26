//! The SHAPE of the tree around a generated box (lot BT-5): where the build
//! puts `::before`/`::after`, what it names and what it does not.

use super::*;
use crate::style::PseudoElement;

fn no(dom: &crate::dom::Dom, sel: &str) -> crate::dom::NodeIdx {
    dom.resolve(dom.query(sel).unwrap_or_else(|| panic!("the fixture has {sel}")))
        .expect("live node")
}

fn e_gerada(tree: &BoxTree, b: BoxId, dono: crate::dom::NodeIdx, pe: PseudoElement) -> bool {
    tree.kind(b) == BoxKind::Generated { originating: dono, pseudo: pe }
}

/// `::before` is the FIRST child of its element's box and `::after` the LAST,
/// with the element's own content between them — CSS 2.1 §12.1's "as if
/// inserted immediately before/after the content", in the tree.
#[test]
fn before_and_after_are_the_first_and_last_child_of_the_originating_box() {
    let dom = crate::parse_html_to_dom(
        "<style>p::before{content:'['} p::after{content:']'}</style><p id=p>oi</p>",
    );
    let tree = build_mirror(&dom);
    let p = no(&dom, "#p");
    let filhos = tree.children(tree.boxes_of(p)[0]);
    assert_eq!(filhos.len(), 3, "::before, the text, ::after: {filhos:?}");
    assert!(e_gerada(&tree, filhos[0], p, PseudoElement::Before));
    assert!(matches!(tree.kind(filhos[1]), BoxKind::Text { .. }));
    assert!(e_gerada(&tree, filhos[2], p, PseudoElement::After));
}

/// A generated box has NO node: the decision of `pseudo.rs` stands. It is not
/// in `boxes_of` of its element either — that answers what the ELEMENT
/// generated, and a `::before` taken for one of its fragments is the silent
/// misreading a split inline already made possible.
#[test]
fn a_generated_box_has_no_node_and_is_not_one_of_its_elements_boxes() {
    let dom = crate::parse_html_to_dom("<style>p::before{content:'x'}</style><p id=p>oi</p>");
    let tree = build_mirror(&dom);
    let p = no(&dom, "#p");
    let gerada = tree.generated_of(p, PseudoElement::Before).expect("p::before is a box");
    assert_eq!(tree.node_of(gerada), None);
    assert_eq!(tree.boxes_of(p).len(), 1, "one element box, and not the pseudo");
    assert!(!tree.boxes_of(p).contains(&gerada));
    assert_eq!(tree.parent(gerada), Some(tree.boxes_of(p)[0]));
}

/// No `content`, `content: none` and `display: none` generate nothing — the
/// cascade's answer (`Dom::pseudo_box`), asked once, not re-decided here.
#[test]
fn no_generated_box_where_the_cascade_generates_none() {
    for css in [
        "p::before{color:red}",
        "p::before{content:none}",
        "p::before{content:'x';display:none}",
    ] {
        let dom = crate::parse_html_to_dom(&format!("<style>{css}</style><p id=p>oi</p>"));
        let tree = build_mirror(&dom);
        let p = no(&dom, "#p");
        assert_eq!(tree.generated_of(p, PseudoElement::Before), None, "{css}");
        assert_eq!(tree.children(tree.boxes_of(p)[0]).len(), 1, "{css}: only the text");
    }
}

/// A SPLIT inline (§9.2.1.1) has its `::before` in its FIRST fragment and its
/// `::after` in its LAST — and neither in the other.
#[test]
fn a_split_inline_has_before_in_its_first_fragment_and_after_in_its_last() {
    let dom = crate::parse_html_to_dom(
        "<style>span::before{content:'['} span::after{content:']'}</style>\
         <div><span id=s>a<div>b</div>c</span></div>",
    );
    let tree = build_mirror(&dom);
    let s = no(&dom, "#s");
    let [primeiro, ultimo] = tree.boxes_of(s) else {
        panic!("the span splits in two: {:?}", tree.boxes_of(s));
    };
    let (a, b) = (tree.children(*primeiro), tree.children(*ultimo));
    assert!(e_gerada(&tree, a[0], s, PseudoElement::Before));
    assert!(!a.iter().any(|&c| e_gerada(&tree, c, s, PseudoElement::After)));
    assert!(e_gerada(&tree, *b.last().unwrap(), s, PseudoElement::After));
    assert!(!b.iter().any(|&c| e_gerada(&tree, c, s, PseudoElement::Before)));
    assert_eq!(tree.generated_of(s, PseudoElement::After), b.last().copied());
}

/// Around an anonymous TABLE the pseudo stays a direct child of its element:
/// it is not a table part, so it ends the run as §17.2.1 says any non-part
/// sibling does.
#[test]
fn a_pseudo_is_a_sibling_of_the_anonymous_table_not_inside_it() {
    let dom = crate::parse_html_to_dom(
        "<style>div::before{content:'x'}</style>\
         <div id=d><span style='display:table-cell'>c</span></div>",
    );
    let tree = build_mirror(&dom);
    let d = no(&dom, "#d");
    let filhos = tree.children(tree.boxes_of(d)[0]);
    assert!(e_gerada(&tree, filhos[0], d, PseudoElement::Before));
    assert!(matches!(tree.kind(filhos[1]), BoxKind::Anonymous { role: AnonymousRole::Table, .. }));
}

/// The layout walkers see the children WITHOUT the generated boxes — that is
/// what keeps a `::before` from being laid out once by its own path and once
/// more as a step of the tree walk.
#[test]
fn the_walkers_view_leaves_out_exactly_the_generated_boxes() {
    let dom = crate::parse_html_to_dom(
        "<style>p::before{content:'['} p::after{content:']'}</style><p id=p>oi</p>",
    );
    let tree = build_mirror(&dom);
    let caixa = tree.boxes_of(no(&dom, "#p"))[0];
    assert_eq!(tree.children_without_generated(caixa), &tree.children(caixa)[1..2]);
    // and a box with ONLY a `::before` is not trimmed at the other end too.
    let dom = crate::parse_html_to_dom("<style>p::before{content:'['}</style><p id=p></p>");
    let tree = build_mirror(&dom);
    let caixa = tree.boxes_of(no(&dom, "#p"))[0];
    assert_eq!(tree.children(caixa).len(), 1);
    assert!(tree.children_without_generated(caixa).is_empty());
}

/// The style of a generated box is the PSEUDO's, asked fresh — not its
/// originating element's, which is only where it inherits from.
#[test]
fn a_generated_box_answers_the_pseudos_own_style_not_its_elements() {
    let dom = crate::parse_html_to_dom(
        "<style>p{color:#00ff00} p::before{content:'x';color:#ff0000}</style><p id=p>oi</p>",
    );
    let tree = build_mirror(&dom);
    let p = no(&dom, "#p");
    let gerada = tree.generated_of(p, PseudoElement::Before).unwrap();
    assert_eq!(tree.style_source(gerada), p, "it inherits from its element");
    assert_eq!(tree.style(&dom, gerada).unwrap().color, Some(0xFF0000FF));
    assert_eq!(tree.style(&dom, tree.boxes_of(p)[0]).unwrap().color, Some(0x00FF00FF));
    assert_eq!(tree.pseudo_box(&dom, gerada).unwrap().text, "x");
}

/// The formatting context comes from the pseudo's OWN `display`: inline by
/// default (the initial value), block-level when it says so, and independent
/// when it is an item of a flex container — its element.
#[test]
fn a_generated_box_is_what_its_own_display_says() {
    let dom = crate::parse_html_to_dom(
        "<style>#a::before{content:'x'} #b::before{content:'x';display:block}\
         #c{display:flex} #c::before{content:'x'}</style>\
         <p id=a></p><p id=b></p><div id=c></div>",
    );
    let tree = build_mirror(&dom);
    let fc = |sel: &str| {
        let g = tree.generated_of(no(&dom, sel), PseudoElement::Before).unwrap();
        tree.formatting_context(&dom, g)
    };
    assert!(fc("#a").is_inline_level() && !fc("#a").independent);
    assert!(fc("#b").is_block_level() && !fc("#b").independent);
    assert!(fc("#c").independent, "a flex item establishes its own context");
    // and a block-level `::before` makes its container a stack (§9.2.1).
    let b = tree.boxes_of(no(&dom, "#b"))[0];
    assert!(!tree.runs_inline_formatting_context(&dom, b));
}

/// A cached fragment keeps `BoxId`s across a rebuild, and a generated box has
/// no `(node, ordinal)` to be found by: its address is its parent's plus which
/// pseudo it is. When the new tree no longer has it, there is no answer.
#[test]
fn a_generated_box_is_found_again_in_a_rebuilt_tree_and_only_if_it_exists() {
    let dom = crate::parse_html_to_dom("<style>p::before{content:'x'}</style><p id=p>oi</p>");
    let antiga = build_mirror(&dom);
    let nova = build_mirror(&dom);
    let p = no(&dom, "#p");
    let g_antiga = antiga.generated_of(p, PseudoElement::Before).unwrap();
    assert_eq!(nova.translate_from(&antiga, g_antiga), nova.generated_of(p, PseudoElement::Before));
    assert_ne!(nova.translate_from(&antiga, g_antiga), Some(g_antiga), "another generation");

    let sem = crate::parse_html_to_dom("<p id=p>oi</p>");
    let vazia = build_mirror(&sem);
    assert_eq!(vazia.translate_from(&antiga, g_antiga), None);
}

/// The memoised tree follows the VIEWPORT: `@media` decides whether a pseudo
/// generates a box, and whether an inline is split around a child that
/// becomes a block — and `set_viewport` bumps no revision.
#[test]
fn the_memoised_tree_follows_a_viewport_change() {
    let dom = crate::parse_html_to_dom(
        "<style>@media (max-width: 500px) { p::before{content:'x'} #b{display:block} }</style>\
         <p id=p>oi</p><div><span id=s>a<i id=b>x</i>c</span></div>",
    );
    let (p, s) = (no(&dom, "#p"), no(&dom, "#s"));
    dom.set_viewport(800.0, 600.0);
    let larga = dom.box_tree();
    assert_eq!(larga.generated_of(p, PseudoElement::Before), None);
    assert_eq!(larga.boxes_of(s).len(), 1, "an inline <i> splits nothing");
    dom.set_viewport(400.0, 600.0);
    let estreita = dom.box_tree();
    assert!(estreita.generated_of(p, PseudoElement::Before).is_some());
    assert_eq!(estreita.boxes_of(s).len(), 2, "a block <i> splits the span at this width");
}
