//! Building the tree: one downward pass over the DOM.
//!
//! One box per element, in document order, plus the boxes CSS generates: the
//! §9.2.1.1 split of an inline around a block-level child (below), the
//! anonymous table (`anonymous_table.rs`) and `::before`/`::after`
//! (`generated.rs`). A TEXT node gets a box of its own, because without one
//! it could not appear in a traversal of the tree at all.
//!
//! ## THE FORM, and why it is the sum of two readings and not one of them
//!
//! CSS 2.1 §9.2.1.1: *"When an inline box contains an in-flow block-level box,
//! the inline box (and its inline ancestors within the same line box) are
//! broken around the block-level box, splitting the inline box into two boxes,
//! one on each side of the block-level box. **The line boxes before the break
//! and after the break are enclosed in anonymous block boxes, and the
//! block-level box becomes a sibling of those anonymous boxes.**"*
//!
//! Two things are asked for at once, and taking either alone gives a tree that
//! answers wrongly:
//!
//! - the inline is split into **several boxes OF ITS OWN**. They are
//!   `Element` boxes and not anonymous ones, because §9.2.1.1 also says the
//!   resulting boxes keep the element's own borders and background — which an
//!   anonymous box cannot carry, having no declarations of its own.
//! - each side is **enclosed in an anonymous BLOCK box**, and the block becomes
//!   a SIBLING of those. A sibling of boxes that live inside the split inline
//!   is not a sibling at all, so the anonymous boxes cannot be children of the
//!   inline: they rise to the box the inline would otherwise have been a single
//!   child of.
//!
//! For `<p><span style="background:red">a<div>b</div>c</span></p>`:
//!
//! ```text
//! p  (element box, block container)
//! ├── anonymous block box           inherits from p
//! │   └── span  (element box, FRAGMENT 1 — keeps the red background)
//! │       └── text "a"
//! ├── div  (element box, block-level)   the sibling §9.2.1.1 asks for
//! │   └── text "b"
//! └── anonymous block box           inherits from p
//!     └── span  (element box, FRAGMENT 2)
//!         └── text "c"
//! ```
//!
//! The anonymous box inherits from **p** and not from the span: §9.2.1.1 says
//! an anonymous box's properties are inherited from the enclosing non-anonymous
//! box, and the box enclosing it is p's. The span's own inherited properties
//! are not lost — they reach the text through the span FRAGMENT, which is a
//! real element box.
//!
//! A NESTED inline splits at every level it passes through
//! (`<span><em>a<div/>c</em></span>`): [`Construcao::itens_do_inline`] recurses into a child
//! that itself needs to split and splices its runs into the parent's own, so
//! the block reaches the outermost splitting ancestor's container while each
//! inline's fragments still nest inside the fragment that encloses them.
//!
//! ## The second family: text in a block container (BT-3)
//!
//! A flow container whose OWN children mix in-flow block-level boxes with
//! inline content (`<div>text<p>x</p>more</div>`) gets the same treatment with
//! no split: each maximal run of inline-level children is enclosed in an
//! anonymous block box by the same [`Construcao::fecha_corrida`]. Until BT-3
//! finished, anonymous boxes existed only where a split happened, and the flow
//! grouped the other runs by hand — the same arithmetic with no box to answer
//! for it. A container with ONLY inline content gets no anonymous box: it IS
//! the inline formatting context. A run of collapsible whitespace and comments,
//! or of floats and absolutely positioned boxes with only such space around
//! them, is not inline content and is not wrapped either (see
//! [`run_without_line_content`]).
//!
//! Inside a container that split, every inline run is wrapped — including the
//! split inline's inline-level SIBLINGS, because leaving them out would put `x`
//! and `a` of `<p>x<span>a<div/>c</span></p>` on different lines, which no
//! browser does.
//!
use super::{BoxId, BoxTree};

mod anonymous_table;
mod items;
mod generated;
use anonymous_table::is_table_part_child;
use items::{
    run_without_line_content, e_item_de_bloco, fecha_fragmento, out_of_flow,
    is_block_level_child, is_inline_flow_box, FlowItem,
};
use crate::dom::{Dom, NodeIdx, NodeKind};
use crate::style::DisplayKind;

/// Builds the tree: one box per element that has a computed style, with an
/// inline box split around a block-level child (CSS 2.1 §9.2.1.1).
///
/// An element with no computed style generates no box. Today that is what the
/// cascade refuses, and it is the same answer layout already gives —
/// `layout_block` asks for the style and gives up without it. It is not a new
/// decision taken by this module.
pub fn build_mirror(dom: &Dom) -> BoxTree {
    // The generation counts BUILDS, not revisions: the tree is memoised by
    // `(revision, style_epoch)`, so a style-only change yields a new tree at the
    // same revision — and an id from the old one would pass the check and read
    // the wrong arena. A build counter has no such hole.
    let mut c = Construcao {
        dom,
        tree: BoxTree::with_generation(dom.next_box_generation()),
        bolhas: crate::fasthash::FastMap::default(),
    };
    // The document root is not an element and generates no box; entry is
    // through its children, the way `layout_document` does it.
    let roots: Vec<NodeIdx> = dom.node(dom.root).children.clone();
    for node in roots {
        c.descend(node, None);
    }
    c.tree
}

/// One build, with the one memo it needs.
///
/// **The memo is not an optimisation kept just in case: without it the pass is
/// quadratic on an inline chain.** [`Construcao::bolha`] asks whether a subtree
/// reaches a block-level box through inline-flow children, and it is asked of
/// every inline child of every element — so a chain of N nested `<span>`s, which
/// is what a real bundled page is made of, walks the tail of the chain once per
/// link. Keyed by node, each answer is computed once and the whole pass is
/// linear again.
struct Construcao<'d> {
    dom: &'d Dom,
    tree: BoxTree,
    bolhas: crate::fasthash::FastMap<NodeIdx, bool>,
}

impl Construcao<'_> {
    fn descend(&mut self, node: NodeIdx, parent: Option<BoxId>) {
        // A TEXT node gets a box, and it inherits the style of the element that
        // encloses it — text has no style of its own, `computed_style_idx`
        // answers `None` for one.
        //
        // Whitespace that collapses away is NOT filtered here. Which whitespace
        // survives is a question about `white-space` and about the neighbours in
        // a line, and `quebra.rs` owns it — deciding it twice, once here on the
        // tree and once there on the runs, is the second-truth failure this
        // module was built to avoid. The ONE place this module does look at
        // whitespace is [`corrida_so_de_espaco`], and it is about whether a BOX
        // exists, never about which characters survive.
        if let NodeKind::Text(_) = &self.dom.node(node).kind {
            let Some(p) = parent else { return };
            let source = self.tree.style_source(p);
            self.tree.push_text(node, source, p);
            return;
        }
        // An element the cascade refuses generates no box. The style itself is
        // not read here — `context.rs` asks for it fresh when someone needs it —
        // so this is a presence test and nothing more.
        if self.dom.computed_style_idx(node).is_none() {
            return;
        }
        let id = self.tree.push_element(node, parent);
        self.gera(node, crate::style::PseudoElement::Before, id);
        self.constroi_filhos(node, id);
        self.gera(node, crate::style::PseudoElement::After, id);
    }

    /// The children of an element box — the one place the split is decided.
    ///
    /// `node` reaching here is, by construction, NOT an inline box that bubbles
    /// a block: such a child is consumed by [`Construcao::itens_do_inline`]
    /// before `descend` ever sees it. So `node` is the CONTAINER the split rises
    /// to, which is exactly where §9.2.1.1 puts the anonymous boxes.
    fn constroi_filhos(&mut self, node: NodeIdx, id: BoxId) {
        // `children` is cloned because the descent re-borrows the document while
        // mutating the tree. The cost is one allocation per element with
        // children; the alternative — an index and re-reading
        // `dom.node(node).children` on every step — trades the allocation for an
        // arena hit per child. If this shows up in a profile, it is here.
        let children: Vec<NodeIdx> = self.dom.node(node).children.clone();
        let mut itens: Vec<FlowItem> = Vec::with_capacity(children.len());
        let mut partiu = false;
        for &c in &children {
            if is_inline_flow_box(self.dom, c) && self.bolha(c) {
                partiu = true;
                let netos: Vec<NodeIdx> = self.dom.node(c).children.clone();
                itens.extend(self.itens_do_inline(c, &netos));
                continue;
            }
            itens.push(FlowItem::Plain(c));
        }
        // NOTHING split here: the ordinary path, and the tree stays an exact
        // mirror of the DOM for this container. This is the branch almost every
        // element on almost every page takes, and keeping it a plain loop is what
        // makes the split cost nothing where it does not apply.
        //
        // **And the refusal beside it: only a FLOW container splits.** CSS 2.1
        // §9.2.1.1 is about an inline box inside an inline formatting context; a
        // flex, grid or table container has no such thing — its children are
        // blockified and there is no line box for an anonymous block to enclose.
        // The question is asked HERE and not per child so that it costs a style
        // read only where something was about to split; everywhere else `partiu`
        // is already `false` and the `&&` never reaches it.
        if !partiu && !self.mixes_block_and_inline(node, &children) {
            self.descend_children(node, id, children);
            return;
        }
        if !self.is_block_container(node) {
            self.descend_children(node, id, children);
            return;
        }
        self.materialize_container(node, id, itens);
    }

    /// `true` for a container whose OWN children mix in-flow block-level boxes
    /// with inline-level content — the second family of CSS 2.1 §9.2.1.1:
    /// *"if a block container box has a block-level box inside it, then we
    /// force it to have only block-level boxes inside it"*, each run of inline
    /// content enclosed in an anonymous block box.
    ///
    /// It needs no split to apply, and before BT-3 finished it applied only
    /// where one had happened; the flow then grouped the loose runs by hand
    /// (`inline_group` in `layout/vertical.rs`), which is the same arithmetic
    /// with no box to answer for it. "Inline content" is asked with the same
    /// test [`Construcao::fecha_corrida`] applies to a run
    /// ([`run_without_line_content`]), so a container of blocks separated
    /// only by source indentation, comments and floats keeps the plain mirror.
    ///
    /// A table-part child defers to the anonymous-table fixup
    /// (`anonymous_table.rs`): wrapping its text siblings here as well would
    /// be two builders deciding one child list, and that rule owns it today.
    fn mixes_block_and_inline(&self, node: NodeIdx, children: &[NodeIdx]) -> bool {
        let mut block = false;
        let mut inline: Vec<FlowItem> = Vec::new();
        for &c in children {
            if is_table_part_child(self.dom, c) {
                return false;
            }
            if is_block_level_child(self.dom, c) {
                block = true;
            } else {
                inline.push(FlowItem::Plain(c));
            }
        }
        let inline = !inline.is_empty() && !run_without_line_content(self.dom, node, &inline);
        block && inline
    }

    /// `true` for a BLOCK CONTAINER — the only box §9.2.1.1 wraps runs in. A
    /// flex, grid or table container has no line box for an anonymous block to
    /// enclose (its children are blockified). A table CELL is a block container
    /// for its content although `inner_of` files it under `Table`, the same
    /// exception `wraps_table_parts` makes.
    fn is_block_container(&self, node: NodeIdx) -> bool {
        let cell = self
            .dom
            .computed_style_idx(node)
            .and_then(|c| c.effective_display())
            == Some(DisplayKind::TableCell);
        cell
            || crate::boxes::context::element_formatting_context(self.dom, node).inner
                == crate::boxes::InnerDisplay::Flow
    }

    /// Lays the flattened items of a container that SPLIT into boxes: each run
    /// of inline-level items is enclosed in one anonymous block box, and every
    /// block-level item becomes a direct child — a sibling of those anonymous
    /// boxes, which is the words of §9.2.1.1.
    fn materialize_container(&mut self, contentor: NodeIdx, id: BoxId, itens: Vec<FlowItem>) {
        let partidos = generated::nos_partidos(&itens);
        let mut corrida: Vec<FlowItem> = Vec::new();
        for item in itens {
            if e_item_de_bloco(self.dom, &item) {
                self.fecha_corrida(contentor, id, &mut corrida);
                match item {
                    FlowItem::Block(n) => {
                        self.descend(n, Some(id));
                        self.tree.record_split(self.dom, n, contentor);
                    }
                    FlowItem::Plain(n) => self.descend(n, Some(id)),
                    // A fragment is inline-level by construction, so it never
                    // answers `true` above.
                    FlowItem::Fragment { .. } => unreachable!("a fragment is inline-level"),
                }
                continue;
            }
            corrida.push(item);
        }
        self.fecha_corrida(contentor, id, &mut corrida);
        self.gera_no_ultimo_fragmento(&partidos);
    }

    /// Encloses one accumulated run of inline-level items in a fresh anonymous
    /// block box, or does nothing when there is nothing to enclose.
    ///
    /// Two runs produce no box. An EMPTY one: a block-level item at either edge,
    /// or two of them in a row, leaves no inline content on that side. And one
    /// made only of collapsible whitespace and comments: CSS 2.1 §9.2.1.1 is
    /// explicit that white space which would collapse away generates no
    /// anonymous box, and wrapping it anyway would put an empty line box — a
    /// full line of height — between two blocks that the indentation of the
    /// source happens to separate. Those nodes then generate no box at all and
    /// are spliced back by `layout::sequencia`, which is where they were already
    /// being skipped.
    fn fecha_corrida(&mut self, contentor: NodeIdx, caixa_pai: BoxId, corrida: &mut Vec<FlowItem>) {
        if corrida.is_empty() {
            return;
        }
        let itens = std::mem::take(corrida);
        // A run of floats and absolutely positioned boxes with only collapsible
        // space around them is not wrapped (see [`run_without_line_content`]);
        // the out-of-flow boxes become direct children of the container, which
        // is where the unsplit mirror had them. The whitespace between them
        // generates no box, exactly as a whitespace-only run does.
        if run_without_line_content(self.dom, contentor, &itens) {
            for item in itens {
                if let FlowItem::Plain(n) = item {
                    if out_of_flow(self.dom, n) {
                        self.descend(n, Some(caixa_pai));
                    }
                }
            }
            return;
        }
        // `inherits_from` is the CONTAINER and not the split inline: §9.2.1.1
        // says an anonymous box inherits from the enclosing non-anonymous box,
        // and the box enclosing this one is the container's. The split inline's
        // own inherited properties are not lost — they reach its content through
        // the FRAGMENT inside, which is a real element box.
        let anonima = self.tree.push_anonymous(contentor, caixa_pai);
        for item in itens {
            self.materializa(item, anonima);
        }
    }

    /// Turns one inline-level `FlowItem` into boxes under `parent`.
    ///
    /// A `Fragment` is exactly one more `push_element` call for the node it
    /// names — this is where a split inline gets its several boxes, each entering
    /// `by_node` the same way its single box would have. A `Plain` falls back to
    /// the ordinary descent, because from here on it is ordinary content,
    /// possibly needing a split of its own further down.
    fn materializa(&mut self, item: FlowItem, parent: BoxId) {
        match item {
            FlowItem::Block(node) | FlowItem::Plain(node) => self.descend(node, Some(parent)),
            FlowItem::Fragment { node, items } => {
                let frag = self.tree.push_element(node, Some(parent));
                self.gera_no_primeiro_fragmento(node, frag);
                for item in items {
                    self.materializa(item, frag);
                }
            }
        }
    }

    /// Flattens `node`'s children into the sequence they resolve to once `node`
    /// (already known to be an inline box with a bubbling block-level
    /// descendant) splits. A child that is itself a splitting inline is spliced
    /// in rather than kept as a single `Plain` item: its `Block` items break
    /// `node`'s own run exactly where they would have anyway, and its
    /// `Fragment`/`Plain` items join `node`'s run — which is how one block
    /// reaches past more than one level of nesting while each inline's own
    /// fragments stay nested inside the fragment that encloses them.
    ///
    /// A run with nothing in it produces no `Fragment`: a block-level child as
    /// the first or last item, or two of them in a row, leaves no inline content
    /// on that side to enclose, and no browser this engine is measured against
    /// materialises an empty fragment either.
    fn itens_do_inline(&mut self, node: NodeIdx, children: &[NodeIdx]) -> Vec<FlowItem> {
        let mut out: Vec<FlowItem> = Vec::new();
        let mut run: Vec<FlowItem> = Vec::new();
        for &child in children {
            if is_block_level_child(self.dom, child) {
                fecha_fragmento(node, &mut run, &mut out);
                out.push(FlowItem::Block(child));
                continue;
            }
            if is_inline_flow_box(self.dom, child) && self.bolha(child) {
                let netos: Vec<NodeIdx> = self.dom.node(child).children.clone();
                for item in self.itens_do_inline(child, &netos) {
                    match item {
                        FlowItem::Block(_) => {
                            fecha_fragmento(node, &mut run, &mut out);
                            out.push(item);
                        }
                        aninhado => run.push(aninhado),
                    }
                }
                continue;
            }
            run.push(FlowItem::Plain(child));
        }
        fecha_fragmento(node, &mut run, &mut out);
        out
    }

    /// `true` when `node` reaches a block-level box through its own children or
    /// through a chain of inline boxes that would themselves split — the question
    /// that decides whether `node` splits at all.
    ///
    /// An inline child that is not itself block-level but is on the path to a
    /// block-level descendant counts, because [`Construcao::itens_do_inline`] is
    /// about to recurse into it rather than treat it as ordinary content.
    ///
    /// Memoised — see [`Construcao`] for the shape of page that makes the memo
    /// the difference between linear and quadratic.
    fn bolha(&mut self, node: NodeIdx) -> bool {
        if let Some(&r) = self.bolhas.get(&node) {
            return r;
        }
        let children: Vec<NodeIdx> = self.dom.node(node).children.clone();
        let mut r = false;
        for c in children {
            if is_block_level_child(self.dom, c) || (is_inline_flow_box(self.dom, c) && self.bolha(c))
            {
                r = true;
                break;
            }
        }
        self.bolhas.insert(node, r);
        r
    }
}
