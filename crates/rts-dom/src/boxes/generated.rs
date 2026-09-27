//! THE GENERATED BOX: `::before` and `::after` as boxes of the tree.
//!
//! CSS 2.1 §12.1 makes the pseudo a box "as if inserted immediately before /
//! after the element's content", and until lot BT-5 this engine had it only in
//! LAYOUT — three roles (block, flex item, inline) measured and painted through
//! `layout/pseudo_caixa.rs`, each re-deriving the pseudo from its node with
//! `Dom::pseudo_box`, and none of them nameable by the tree. The inline atom
//! carried `caixa: None`, and `rect_of_box` could not answer for one.
//!
//! **The box has a `BoxId` and NO node.** `pseudo.rs` decided not to put a
//! `NodeIdx` in the arena for a pseudo — it would be created and destroyed on
//! every re-cascade and seen by `childNodes` — and that stays right: the tree
//! is exactly the structure that decision said was missing. So `node_of`
//! answers `None`, `boxes_of` never returns one, and the pair `(originating,
//! pseudo)` in [`BoxKind::Generated`] is its identity, the same pair the
//! counters table keys it by.
//!
//! **What is NOT stored, and why.** Neither the text of `content` nor the
//! pseudo's style: both are asked of `Dom::pseudo_box` each time
//! ([`BoxTree::pseudo_box`], [`BoxTree::style`]), for the reason `BoxKind`'s
//! header gives — the tree is memoised without `anim_epoch`, so a copy taken
//! here would be a frame behind for the whole of an animation.
//!
//! **Where it sits.** The first child (`::before`) or the last child
//! (`::after`) of the originating element's box — of its FIRST box and its
//! LAST box when the §9.2.1.1 split gave the element several, which is where
//! CSS puts them. `build/generated.rs` quotes the condition and the refusals.

use super::{BoxId, BoxKind, BoxTree};
use crate::dom::NodeIdx;
use crate::style::PseudoElement;

impl BoxTree {
    /// A generated box under `parent`, which is a box of `originating`.
    ///
    /// It does not enter `by_node`: that is what keeps it out of `boxes_of`,
    /// whose callers ask "what did this ELEMENT generate" and would take a
    /// `::before` for a fragment of the element itself.
    pub fn push_generated(&mut self, originating: NodeIdx, pseudo: PseudoElement, parent: BoxId) -> BoxId {
        self.push(BoxKind::Generated { originating, pseudo }, Some(parent))
    }

    /// The generated box `pseudo` directly under `parent`: its first child for
    /// `::before`, its last for `::after`, and `None` when that child is not
    /// one. `::marker` is never a generated box here (`listitem.rs` paints it).
    pub fn generated_child(&self, parent: BoxId, pseudo: PseudoElement) -> Option<BoxId> {
        let filhos = self.children(parent);
        let candidata = match pseudo {
            PseudoElement::Before => filhos.first(),
            PseudoElement::After => filhos.last(),
            PseudoElement::Marker => None,
        }?;
        matches!(self.kind(*candidata), BoxKind::Generated { pseudo: p, .. } if p == pseudo)
            .then_some(*candidata)
    }

    /// The generated box `pseudo` of the element `originating`, looked for in
    /// its FIRST box (`::before`) or its LAST box (`::after`).
    ///
    /// For a caller that knows the element and not the box it is laying out —
    /// the line flow's `pseudo_inline`, whose callers in `linha.rs` hand it a
    /// node. A caller that HAS the box asks [`BoxTree::generated_child`]
    /// instead: for a split inline, the node's first box is not the fragment it
    /// is walking.
    pub fn generated_of(&self, originating: NodeIdx, pseudo: PseudoElement) -> Option<BoxId> {
        let caixas = self.boxes_of(originating);
        let dono = match pseudo {
            PseudoElement::After => caixas.last(),
            _ => caixas.first(),
        }?;
        self.generated_child(*dono, pseudo)
    }

    /// The content and style of a generated box, asked of the cascade NOW.
    ///
    /// `None` for a box that is not generated, and for one the cascade no
    /// longer generates — which, with the memo key of `dom/box_tree.rs`
    /// covering what the cascade reads, does not happen inside one layout pass.
    pub fn pseudo_box(&self, dom: &crate::dom::Dom, id: BoxId) -> Option<crate::pseudo::PseudoBox> {
        let BoxKind::Generated { originating, pseudo } = self.kind(id) else {
            return None;
        };
        dom.pseudo_box(originating, pseudo)
    }

    /// The children of `id` WITHOUT its generated boxes — what every layout
    /// walker of `tree.children` walks while generated content keeps its own
    /// three paths (`pseudo_bloco`, `flex_pseudo`, `pseudo_inline`).
    ///
    /// **Why a view and not a filter at each walker.** Seventeen call sites in
    /// twelve files walked `tree.children`, and seven of them read "`node_of`
    /// is `None`" as "anonymous box: descend into its run" — the misreading
    /// `BoxTree::kind` warned about for the day a second kind stopped naming a
    /// node, which is this lot. A generated box reaching them is laid out
    /// TWICE (`sequencia` makes it a block step beside `pseudo_bloco`'s), or
    /// panics (`runs.rs` expects no box without a node inside an inline). The
    /// other ten skip a box with no node, which is right by accident. One
    /// accessor that all seventeen call is a rule; seventeen filters are
    /// seventeen chances to forget it.
    ///
    /// A SLICE, because generated boxes are only ever the first and the last
    /// child: trimming both ends is the whole filter, with no allocation.
    ///
    /// What keeps `children`: the walks that must see every box, and a
    /// generated one is a box — `positioned/relative.rs` and `paint/transform.rs` move the
    /// rectangles of a subtree, and a `::before` moves with its element.
    pub fn children_without_generated(&self, id: BoxId) -> &[BoxId] {
        let filhos = self.children(id);
        let inicio = usize::from(self.generated_child(id, PseudoElement::Before).is_some());
        // A lone `::before` is the first AND the last child, and is not
        // trimmed twice: `generated_child` matches the pseudo, not the slot.
        let fim = filhos.len() - usize::from(self.generated_child(id, PseudoElement::After).is_some());
        &filhos[inicio..fim]
    }

    /// The box of THIS tree that `old`, a box of `from`, is — or `None` when
    /// it has no counterpart here.
    ///
    /// The address that survives a rebuild is `(node, ordinal)` for a box that
    /// names a node (`dom/chaves_cache.rs`). A generated box names none, and
    /// its address is its parent's plus which pseudo it is: the parent is a
    /// box of the originating element, translated the same way, and the
    /// pseudo picks the first or the last child. It used to fail here as an
    /// anonymous box does, and a cached fragment holding one could then never
    /// be reused across a rebuild — every fragment with a `::before` in it.
    ///
    /// The rest is the `remap_box_id` that lived beside the fragment in
    /// `layout/fragment/fragment.rs`, moved here unchanged because translating between
    /// node and box is this module's alone — including its refusal when a
    /// node's box COUNT changed: guessing which box a fragment meant is the
    /// silent answer.
    pub fn translate_from(&self, from: &BoxTree, old: BoxId) -> Option<BoxId> {
        if let BoxKind::Generated { pseudo, .. } = from.kind(old) {
            let pai = self.translate_from(from, from.parent(old)?)?;
            return self.generated_child(pai, pseudo);
        }
        let node = from.node_of(old)?;
        let old_boxes = from.boxes_of(node);
        let ordinal = old_boxes.iter().position(|&candidate| candidate == old)?;
        let new_boxes = self.boxes_of(node);
        (old_boxes.len() == new_boxes.len())
            .then(|| new_boxes.get(ordinal).copied())
            .flatten()
    }
}
