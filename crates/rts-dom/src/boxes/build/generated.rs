//! Where the build emits a GENERATED box (`::before`/`::after`, lot BT-5).
//!
//! **The condition is the cascade's, asked once: `Dom::pseudo_box` answers
//! `Some`.** That is where the layout's three roles already start — each one
//! calls it and then decides whether IT lays the box out (`pseudo_bloco` takes
//! a block-level one with text, `flex_pseudo` any one of a flex container,
//! `pseudo_inline` the rest). Those are role decisions, not existence, and
//! writing any of them here would be a second answer to "does this element
//! generate a box" — the question `pseudo_box` exists to own (`content` is
//! not `none`/`normal`, the pseudo is not `display: none`).
//!
//! It follows that the tree can hold a generated box no role lays out today:
//! the `::before` of a `<br>`, an `<input>` or an `<img>` (CSS generates none
//! for a replaced element; `pseudo_box` does not ask), or a block-level one of
//! an element that is not a vertical flow's owner. The tree answers what the
//! cascade says; what the layout still refuses is the layout's to fix, and it
//! gets no geometry until then.
//!
//! ## Where it goes
//!
//! The first child (`::before`) or the last child (`::after`) of the element's
//! box, outside any anonymous box the element's children needed:
//!
//! - **An anonymous TABLE** (`anonymous_table.rs`): the pseudo is not a table
//!   part, so it ends the run exactly as CSS 2.1 §17.2.1 says a non-part
//!   sibling does. The same place CSS puts it.
//! - **A container that split an inline** (§9.2.1.1): the pseudo stays a
//!   direct child, a sibling of the anonymous blocks. For a block-level pseudo
//!   that is CSS's shape; for an inline one CSS would put it INSIDE the first
//!   (or last) anonymous block, with the run it joins. Refused, not missed:
//!   the split does not look at generated content (`bolha` asks only the DOM,
//!   so a block-level `::after` inside a `<span>` does not split it either),
//!   and moving the pseudo into a run is the lot that makes the three layout
//!   roles one — the tree walk has to lay it out there first.
//! - **A split inline**: `::before` in its FIRST fragment, `::after` in its
//!   LAST, which is where CSS puts them. An inline the split consumed whole
//!   (`<span><div/></span>`) has no fragment and therefore no generated box,
//!   as it has no element box.

use super::{Construcao, FlowItem, NodeIdx};
use crate::boxes::BoxId;
use crate::style::PseudoElement;

impl Construcao<'_> {
    /// The generated box `pe` of `node` under `parent`, one of `node`'s boxes,
    /// when the cascade generates one. Called right after the parent is
    /// pushed for `::before`, after its children for `::after`: the order of
    /// `children` IS the first/last position `generated_child` reads.
    pub(super) fn gera(&mut self, node: NodeIdx, pe: PseudoElement, parent: BoxId) {
        if self.dom.pseudo_box(node, pe).is_some() {
            self.tree.push_generated(node, pe, parent);
        }
    }

    /// `::before` of a split inline, when `fragmento` is its FIRST fragment —
    /// the only moment the build knows it, since fragments are pushed in
    /// document order.
    pub(super) fn gera_no_primeiro_fragmento(&mut self, node: NodeIdx, fragmento: BoxId) {
        if self.tree.boxes_of(node).len() == 1 {
            self.gera(node, PseudoElement::Before, fragmento);
        }
    }

    /// `::after` of every inline in `partidos`, as the last child of its LAST
    /// fragment. It has to wait until the whole container is materialised: a
    /// fragment does not know whether another of its node follows.
    pub(super) fn gera_no_ultimo_fragmento(&mut self, partidos: &[NodeIdx]) {
        for &node in partidos {
            if let Some(&ultimo) = self.tree.boxes_of(node).last() {
                self.gera(node, PseudoElement::After, ultimo);
            }
        }
    }
}

/// Every node the flattened items of a container name as a `Fragment`, at
/// any depth, once each — the inlines this container's split broke.
pub(super) fn nos_partidos(itens: &[FlowItem]) -> Vec<NodeIdx> {
    fn anda(itens: &[FlowItem], out: &mut Vec<NodeIdx>) {
        for item in itens {
            if let FlowItem::Fragment { node, items } = item {
                if !out.contains(node) {
                    out.push(*node);
                }
                anda(items, out);
            }
        }
    }
    let mut out = Vec::new();
    anda(itens, &mut out);
    out
}
