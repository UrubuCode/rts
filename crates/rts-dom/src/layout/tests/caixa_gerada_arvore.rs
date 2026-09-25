//! The generated box as the LAYOUT sees it once it is a box of the tree (lot
//! BT-5): its geometry is recorded under its `BoxId`, it is laid out once, the
//! inline atom names it, and the incremental seam tells it apart.
//!
//! None of these assert a position the layout did not already produce: the
//! lot changes identity, not behaviour, and the rects asserted here are the
//! ones the three roles were already painting.

use crate::boxes::BoxId;
use crate::style::PseudoElement;
use crate::table::tests::geometria;

fn no(dom: &crate::Dom, sel: &str) -> crate::dom::NodeIdx {
    dom.resolve(dom.query(sel).expect(sel)).expect("live node")
}

fn gerada(list: &crate::layout::DisplayList, dom: &crate::Dom, sel: &str, pe: PseudoElement) -> BoxId {
    list.tree.generated_of(no(dom, sel), pe).unwrap_or_else(|| panic!("{sel} has a generated box"))
}

fn perto(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.01
}

/// A BLOCK-level pseudo answers `rect_of_box` with its border box — where
/// `pseudo_bloco` painted it: after its margin, at the top of its element,
/// its declared height tall and filling the rest of the line.
#[test]
fn a_block_pseudo_answers_rect_of_box_with_its_border_box() {
    let (dom, list) = geometria(
        "<style>p{margin:0} p::before{content:'x';display:block;height:30px;margin-left:5px}</style>\
         <p id=p>oi</p>",
        800.0,
    );
    let p = list.rect_of(no(&dom, "#p")).expect("p has geometry");
    let r = list.rect_of_box(gerada(&list, &dom, "#p", PseudoElement::Before)).expect("::before has geometry");
    assert!(perto(r.x, 5.0) && perto(r.y, p.y) && perto(r.h, 30.0), "{r:?}");
    assert!(perto(r.w, p.w - 5.0), "fills its element after the margin: {r:?} in {p:?}");
}

/// `position: relative` moves the element's subtree AFTER it was laid out,
/// walking the tree's full `children` (`relativo.rs`) — and the generated box
/// is one of them, so its rect moves with the element, once.
#[test]
fn a_generated_box_moves_with_its_relatively_positioned_element() {
    let html = |pos: &str| {
        format!(
            "<style>p{{margin:0}} p::before{{content:'x';display:block;height:30px}}</style>\
             <p id=p style='{pos}'>oi</p>"
        )
    };
    let (dom, fixo) = geometria(&html(""), 800.0);
    let (dom_rel, movido) = geometria(&html("position:relative;left:10px;top:7px"), 800.0);
    let a = fixo.rect_of_box(gerada(&fixo, &dom, "#p", PseudoElement::Before)).unwrap();
    let b = movido.rect_of_box(gerada(&movido, &dom_rel, "#p", PseudoElement::Before)).unwrap();
    assert!(perto(b.x - a.x, 10.0) && perto(b.y - a.y, 7.0), "{a:?} -> {b:?}");
}

/// A FLEX-ITEM pseudo and an `inline-block` one answer too — the three roles
/// record through the one `pseudo_caixa::pintar` they all paint with.
#[test]
fn a_flex_item_pseudo_and_an_inline_block_pseudo_answer_rect_of_box() {
    let (dom, list) = geometria(
        "<style>#c{display:flex} #c::before{content:'';width:20px;height:10px}\
         #s::after{content:'';display:inline-block;width:50px;height:15px}</style>\
         <div id=c><span>a</span></div><p><span id=s>b</span></p>",
        800.0,
    );
    let item = list.rect_of_box(gerada(&list, &dom, "#c", PseudoElement::Before)).expect("flex item");
    let c = list.rect_of(no(&dom, "#c")).unwrap();
    assert!(perto(item.x, c.x) && perto(item.w, 20.0) && perto(item.h, 10.0), "{item:?}");
    let atomo = list.rect_of_box(gerada(&list, &dom, "#s", PseudoElement::After)).expect("inline-block");
    assert!(perto(atomo.w, 50.0) && perto(atomo.h, 15.0), "{atomo:?}");
}

/// An `inline` pseudo with a surface records the union of its line fragments,
/// the way `union_rect` does for a real inline. It sits inside its element's
/// rect, and its own padding makes it wider than nothing.
#[test]
fn an_inline_pseudo_with_a_surface_answers_rect_of_box() {
    let (dom, list) = geometria(
        "<style>#s::before{content:'ab';padding:0 4px;background:#ff0000}</style><p><span id=s>c</span></p>",
        800.0,
    );
    let r = list.rect_of_box(gerada(&list, &dom, "#s", PseudoElement::Before)).expect("inline surface");
    let s = list.rect_of(no(&dom, "#s")).unwrap();
    assert!(r.w > 8.0 && r.x >= s.x - 0.01 && r.x + r.w <= s.x + s.w + 0.01, "{r:?} in {s:?}");
}

/// The block flow's sequence does NOT hold the generated box: `pseudo_bloco`
/// lays it out around the loop, and a step for it would lay it out twice.
#[test]
fn the_block_flow_does_not_step_through_a_generated_box() {
    let (dom, list) = geometria(
        "<style>p::before{content:'x';display:block} p::after{content:'y';display:block}</style><p id=p>oi</p>",
        800.0,
    );
    let p = no(&dom, "#p");
    let caixa = list.tree.boxes_of(p)[0];
    assert_eq!(list.tree.children(caixa).len(), 3, "the tree has both pseudos");
    let seq = super::super::sequencia::sequencia_do_fluxo(&dom, &list.tree, p, caixa);
    assert_eq!(seq.len(), 1, "only the text is a step of the flow: {seq:?}");
}

/// The inline atom of an `inline-block` pseudo carries its box, from the box
/// the line walk is in.
#[test]
fn the_inline_atom_of_a_generated_box_carries_its_box() {
    let (dom, list) = geometria(
        "<style>#s::before{content:'';display:inline-block;width:5px;height:5px}</style><p><span id=s>b</span></p>",
        800.0,
    );
    let s = no(&dom, "#s");
    let caixa = list.tree.boxes_of(s)[0];
    let css = dom.computed_style_idx(s).unwrap();
    let ctx = crate::layout::LayoutCtx { viewport_w: 800.0, viewport_h: 600.0, measurer: &crate::layout::ApproxMeasurer };
    let runs = super::super::runs::collect_runs(&dom, s, caixa, &list.tree, &css, 800.0, &ctx);
    let atomo = runs
        .iter()
        .find_map(|r| match r.atomic {
            Some((_, b, crate::inline_box::AtomicKind::Gerada(PseudoElement::Before, _))) => Some(b),
            _ => None,
        })
        .expect("the pseudo is an atom on the line");
    assert_eq!(Some(atomo), list.tree.generated_child(caixa, PseudoElement::Before));
}

/// After a mutation elsewhere the tree is REBUILT — new generation — and the
/// cached fragments that hold generated boxes are taken across it
/// (`BoxTree::translate_from`). The incremental layout must still be the one
/// from scratch, and still answer `rect_of_box` for each generated box.
#[test]
fn an_incremental_relayout_with_generated_boxes_is_the_one_from_scratch() {
    const HTML: &str = "<style>.b::before{content:'x';display:block;height:10px} \
        .f{display:flex} .f::after{content:'';width:7px;height:7px} \
        .i::after{content:'';display:inline-block;width:5px;height:5px}</style>\
        <div id=a class=b>a</div><div id=f class=f><span>s</span></div>\
        <p><span id=i class=i>i</span></p><p id=t>texto</p>";
    let ctx = crate::layout::LayoutCtx { viewport_w: 800.0, viewport_h: 600.0, measurer: &crate::layout::ApproxMeasurer };
    let mut dom = crate::parse_html_to_dom(HTML);
    let _ = crate::layout::layout_cached(&dom, &ctx);
    dom.set_text(dom.query("#t").unwrap(), "outro texto");
    let mut fresco = crate::parse_html_to_dom(HTML);
    fresco.set_text(fresco.query("#t").unwrap(), "outro texto");

    let incremental = crate::layout::layout_cached(&dom, &ctx);
    let zero = crate::layout::layout_document(&fresco, &ctx);
    let (a, b) = (incremental.materialized(), zero.materialized());
    assert_eq!(a.len(), b.len(), "paint item count");
    assert!(a.iter().zip(&b).all(|(x, y)| super::itens_equivalentes(x, y)), "paint items");
    for (sel, pe) in [("#a", PseudoElement::Before), ("#f", PseudoElement::After), ("#i", PseudoElement::After)] {
        let ri = incremental.rect_of_box(gerada(&incremental, &dom, sel, pe));
        let rz = zero.rect_of_box(gerada(&zero, &fresco, sel, pe));
        match (ri, rz) {
            (Some(ri), Some(rz)) => assert!(super::rects_equivalentes(&ri, &rz), "{sel}: {ri:?} != {rz:?}"),
            other => panic!("{sel}: the generated box must answer both ways: {other:?}"),
        }
    }
}

/// The same mutation, counted: the containers whose fragments hold generated
/// boxes are REUSED across the rebuild — one patch, one miss (`#t` alone).
/// Measured the other way before this was written: with a generated box
/// untranslatable, as an anonymous one still is, the patch fails and it is
/// zero patches and four misses — the same drawing, recomputed.
#[test]
#[cfg(feature = "metrics")]
fn fragments_holding_generated_boxes_survive_a_rebuild() {
    const HTML: &str = "<style>.b::before{content:'x';display:block;height:10px} \
        .f{display:flex} .f::after{content:'';width:7px;height:7px}</style>\
        <div id=a class=b>a</div><div id=f class=f><span>s</span></div><p id=t>texto</p>";
    let ctx = crate::layout::LayoutCtx { viewport_w: 800.0, viewport_h: 600.0, measurer: &crate::layout::ApproxMeasurer };
    let mut dom = crate::parse_html_to_dom(HTML);
    let _ = crate::layout::layout_cached(&dom, &ctx);
    dom.set_text(dom.query("#t").unwrap(), "outro texto");
    crate::metrics::counters::reset();
    let _ = crate::layout::layout_cached(&dom, &ctx);
    let m = crate::metrics::counters::snapshot();
    assert_eq!((m.fragment_patches, m.fragment_misses), (1, 1), "hits={}", m.fragment_hits);
}

/// The incremental seam refuses a container whose generated box APPEARED —
/// the child sequence is the tree's, and it changed — and accepts the same
/// shape twice. Comparing the old way, without the pseudo in the tree, the
/// two sequences were equal.
#[test]
fn the_seam_sees_a_generated_box_appear() {
    let sem = crate::parse_html_to_dom("<style>p::before{color:red}</style><p id=p>oi</p>");
    let com = crate::parse_html_to_dom("<style>p::before{content:'x'}</style><p id=p>oi</p>");
    let (a, b) = (sem.box_tree(), com.box_tree());
    let (pa, pb) = (a.boxes_of(no(&sem, "#p"))[0], b.boxes_of(no(&com, "#p"))[0]);
    assert!(!super::super::costura_filhos::mesma_sequencia_de_filhos(&a, pa, &b, pb));
    let b2 = crate::boxes::build_mirror(&com);
    let pb2 = b2.boxes_of(no(&com, "#p"))[0];
    assert!(super::super::costura_filhos::mesma_sequencia_de_filhos(&b, pb, &b2, pb2));
}
